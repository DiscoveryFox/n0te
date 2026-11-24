package dev.n0bridge.n0te

import android.graphics.Canvas
import android.graphics.Color
import android.graphics.Paint
import android.graphics.Path
import android.graphics.PorterDuff
import android.os.Bundle
import android.os.SystemClock
import android.util.Log
import android.view.Gravity
import android.view.MotionEvent
import android.view.SurfaceView
import android.view.ViewGroup
import android.widget.FrameLayout
import android.widget.TextView
import androidx.activity.ComponentActivity
import androidx.core.view.WindowCompat
import androidx.core.view.WindowInsetsCompat
import androidx.core.view.WindowInsetsControllerCompat
import androidx.graphics.lowlatency.CanvasFrontBufferedRenderer
import androidx.input.motionprediction.MotionEventPredictor
import uniffi.n0te_ffi.FfiBrush
import uniffi.n0te_ffi.FfiStroke
import uniffi.n0te_ffi.NoteLibrary
import uniffi.n0te_ffi.SheetHandle
import uniffi.n0te_ffi.strokeOutline
import java.io.File
import java.nio.ByteBuffer
import java.nio.ByteOrder

/**
 * M0 spike 2: low-latency ink with the shared core's stroke geometry.
 *
 * The stroke being drawn goes to the front buffer (drawn straight into the buffer the display
 * scans out, no double-buffer wait), extended by a predicted point; its outline comes from the
 * Rust core on every pen sample. On pen-up the stroke is stored in a core sheet and the full
 * scene is redrawn in the multi-buffered layer. Pen only: fingers are ignored. The overlay
 * shows the per-sample processing time and how old a sample is when it reaches us.
 */
class InkSpikeActivity : ComponentActivity() {

    private lateinit var surface: SurfaceView
    private lateinit var stats: TextView
    private var renderer: CanvasFrontBufferedRenderer<Path>? = null
    private var predictor: MotionEventPredictor? = null
    private var sheet: SheetHandle? = null

    private val brush = FfiBrush(width = 2.2f, thinning = 0.6f, color = 0xFF111111u, kind = 0u)
    private val paint = Paint(Paint.ANTI_ALIAS_FLAG).apply { color = 0xFF111111.toInt(); style = Paint.Style.FILL }
    private val finished = ArrayList<Path>()
    private var samples = FloatArray(4 * 512)
    private var count = 0
    private var startTime = 0L
    private var pxPerPt = 1f

    // metrics (main thread only)
    private var procSumNs = 0L
    private var procMaxNs = 0L
    private var ageSumMs = 0L
    private var events = 0
    private var lastStatsAt = 0L
    private var lastSaveMs = 0.0

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        WindowCompat.setDecorFitsSystemWindows(window, false)
        WindowInsetsControllerCompat(window, window.decorView).apply {
            hide(WindowInsetsCompat.Type.systemBars())
            systemBarsBehavior = WindowInsetsControllerCompat.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE
        }
        pxPerPt = resources.displayMetrics.density * 1.25f
        surface = SurfaceView(this)
        stats = TextView(this).apply {
            setTextColor(0xFFD71921.toInt())
            textSize = 12f
            typeface = android.graphics.Typeface.MONOSPACE
            setPadding(24, 16, 24, 16)
            setBackgroundColor(0xCCFFFFFF.toInt())
            text = "Stift: zeichnen · Finger werden ignoriert"
        }
        setContentView(FrameLayout(this).apply {
            addView(surface, ViewGroup.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT))
            addView(stats, FrameLayout.LayoutParams(ViewGroup.LayoutParams.WRAP_CONTENT, ViewGroup.LayoutParams.WRAP_CONTENT, Gravity.TOP or Gravity.END))
        })
        runCatching {
            val dir = File(filesDir, "spike-library")
            val lib = NoteLibrary.open(dir.absolutePath)
            val nb = lib.createNotebook("Ink spike")
            val ids = nb.addSheet(null, 0u, uniffi.n0te_ffi.FfiSheetSpec(0f, 0f, uniffi.n0te_ffi.FfiBackground.BLANK, 24f, null, 0u))
            sheet = nb.openSheet(ids[1])
        }.onFailure { Log.e(MainActivity.TAG, "core open failed", it) }

        renderer = CanvasFrontBufferedRenderer(surface, object : CanvasFrontBufferedRenderer.Callback<Path> {
            override fun onDrawFrontBufferedLayer(canvas: Canvas, bufferWidth: Int, bufferHeight: Int, param: Path) {
                canvas.drawColor(Color.TRANSPARENT, PorterDuff.Mode.CLEAR)
                canvas.drawPath(param, paint)
            }

            override fun onDrawMultiBufferedLayer(canvas: Canvas, bufferWidth: Int, bufferHeight: Int, params: Collection<Path>) {
                canvas.drawColor(Color.WHITE)
                for (p in finished) canvas.drawPath(p, paint)
            }
        })
        predictor = MotionEventPredictor.newInstance(surface)
        surface.setOnTouchListener { v, ev -> onTouch(v, ev) }
        surface.post { renderer?.renderMultiBufferedLayer(emptyList()) }
    }

    override fun onDestroy() {
        renderer?.release(true)
        renderer = null
        super.onDestroy()
    }

    private fun onTouch(v: android.view.View, ev: MotionEvent): Boolean {
        val stylus = (0 until ev.pointerCount).indexOfFirst {
            val t = ev.getToolType(it)
            t == MotionEvent.TOOL_TYPE_STYLUS || t == MotionEvent.TOOL_TYPE_ERASER
        }
        if (stylus < 0) return true // fingers and palms never draw
        val t0 = SystemClock.elapsedRealtimeNanos()
        predictor?.record(ev)
        when (ev.actionMasked) {
            MotionEvent.ACTION_DOWN, MotionEvent.ACTION_POINTER_DOWN -> {
                v.requestUnbufferedDispatch(ev)
                count = 0
                startTime = ev.eventTime
                add(ev.getX(stylus), ev.getY(stylus), ev.getPressure(stylus), ev.eventTime)
                drawLive(withPrediction = false)
            }
            MotionEvent.ACTION_MOVE -> {
                for (h in 0 until ev.historySize) {
                    add(ev.getHistoricalX(stylus, h), ev.getHistoricalY(stylus, h), ev.getHistoricalPressure(stylus, h), ev.getHistoricalEventTime(h))
                }
                add(ev.getX(stylus), ev.getY(stylus), ev.getPressure(stylus), ev.eventTime)
                drawLive(withPrediction = true)
            }
            MotionEvent.ACTION_UP, MotionEvent.ACTION_POINTER_UP, MotionEvent.ACTION_CANCEL -> {
                if (ev.actionMasked != MotionEvent.ACTION_CANCEL && count > 0) commitStroke()
                count = 0
                renderer?.commit()
            }
        }
        val dt = SystemClock.elapsedRealtimeNanos() - t0
        procSumNs += dt
        procMaxNs = maxOf(procMaxNs, dt)
        ageSumMs += SystemClock.uptimeMillis() - ev.eventTime
        events++
        maybeShowStats()
        return true
    }

    private fun add(x: Float, y: Float, p: Float, time: Long) {
        if (count * 4 + 4 > samples.size) samples = samples.copyOf(samples.size * 2)
        samples[count * 4] = x / pxPerPt
        samples[count * 4 + 1] = y / pxPerPt
        samples[count * 4 + 2] = p.coerceIn(0.05f, 1f)
        samples[count * 4 + 3] = (time - startTime).toFloat()
        count++
    }

    /** Outline of the live stroke (plus a predicted sample) into the front buffer. */
    private fun drawLive(withPrediction: Boolean) {
        var n = count
        if (withPrediction) {
            predictor?.predict()?.let { pe ->
                val i = (0 until pe.pointerCount).indexOfFirst { pe.getToolType(it) == MotionEvent.TOOL_TYPE_STYLUS }
                if (i >= 0) {
                    add(pe.getX(i), pe.getY(i), pe.getPressure(i), pe.eventTime)
                    n = count
                    count-- // not part of the stored stroke
                }
                pe.recycle()
            }
        }
        val bytes = strokeOutline(brush, pack(samples, n * 4))
        renderer?.renderFrontBufferedLayer(pathOf(bytes))
    }

    private fun commitStroke() {
        val s = sheet ?: return
        val t = SystemClock.elapsedRealtimeNanos()
        val stroke: FfiStroke = runCatching { s.addStroke(brush, pack(samples, count * 4)) }.getOrNull() ?: return
        finished += pathOf(stroke.outline)
        lastSaveMs = (SystemClock.elapsedRealtimeNanos() - t) / 1e6
    }

    private fun maybeShowStats() {
        val now = SystemClock.uptimeMillis()
        if (now - lastStatsAt < 300 || events == 0) return
        lastStatsAt = now
        val line = "Verarbeitung Ø %.2f ms · max %.2f ms · Eingang Ø %d ms · Speichern %.2f ms · %d Striche".format(
            procSumNs / events / 1e6, procMaxNs / 1e6, ageSumMs / events, lastSaveMs, finished.size
        )
        stats.text = line
        Log.i(MainActivity.TAG, "ink: $line")
        procSumNs = 0; procMaxNs = 0; ageSumMs = 0; events = 0
    }

    /** First `n` floats as packed little-endian bytes for the core. */
    private fun pack(src: FloatArray, n: Int): ByteArray {
        val bb = ByteBuffer.allocate(n * 4).order(ByteOrder.LITTLE_ENDIAN)
        bb.asFloatBuffer().put(src, 0, n)
        return bb.array()
    }

    /** Packed outline (page units) → screen path. */
    private fun pathOf(bytes: ByteArray): Path {
        val f = ByteBuffer.wrap(bytes).order(ByteOrder.LITTLE_ENDIAN).asFloatBuffer()
        val path = Path()
        val n = f.remaining() / 2
        if (n == 0) return path
        path.moveTo(f.get(0) * pxPerPt, f.get(1) * pxPerPt)
        for (i in 1 until n) path.lineTo(f.get(2 * i) * pxPerPt, f.get(2 * i + 1) * pxPerPt)
        path.close()
        return path
    }
}
