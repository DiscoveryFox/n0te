package dev.n0bridge.n0te

import android.content.Context
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.Paint
import android.graphics.Path
import android.graphics.PixelFormat
import android.graphics.PorterDuff
import android.util.Log
import android.view.SurfaceView
import androidx.graphics.lowlatency.CanvasFrontBufferedRenderer

/**
 * The stroke being drawn, with the lowest latency the device allows: a transparent surface
 * above the pages whose front buffer is drawn straight into what the display scans out. The
 * pages themselves stay ordinary (vsync'd) views; when a stroke is finished it is added there
 * and this layer is cleared.
 */
class InkOverlay(context: Context) : SurfaceView(context) {

    /** [paths]: the outline, then (pencil) its middle band and core; [pencil]: grain size and sheet→screen. */
    private class Live(val paths: List<Path>, val color: Int, val pencil: Pair<Float, android.graphics.Matrix>?)

    private val paint = Paint(Paint.ANTI_ALIAS_FLAG).apply { style = Paint.Style.FILL }
    private var renderer: CanvasFrontBufferedRenderer<Live>? = null

    /** False until the renderer works (then [NotebookView] draws the live stroke itself). */
    var active = false
        private set

    init {
        setZOrderOnTop(true)
        holder.setFormat(PixelFormat.TRANSLUCENT)
        // Touches go to the notebook view underneath.
        isClickable = false
        isFocusable = false
        runCatching {
            renderer = CanvasFrontBufferedRenderer(this, object : CanvasFrontBufferedRenderer.Callback<Live> {
                override fun onDrawFrontBufferedLayer(canvas: Canvas, bufferWidth: Int, bufferHeight: Int, param: Live) {
                    canvas.drawColor(Color.TRANSPARENT, PorterDuff.Mode.CLEAR)
                    val pencil = param.pencil
                    if (pencil != null) {
                        Pencil.draw(canvas, Pencil.livePaints(pencil.first, pencil.second), param.paths[0], param.paths.drop(1), param.color)
                    } else {
                        paint.color = param.color
                        canvas.drawPath(param.paths[0], paint)
                    }
                }

                override fun onDrawMultiBufferedLayer(canvas: Canvas, bufferWidth: Int, bufferHeight: Int, params: Collection<Live>) {
                    canvas.drawColor(Color.TRANSPARENT, PorterDuff.Mode.CLEAR)
                }
            })
            active = true
        }.onFailure { Log.w(N0te.TAG, "low-latency ink unavailable: ${it.message}") }
    }

    /** Draws the live stroke (screen coordinates). */
    fun draw(paths: List<Path>, color: Int, pencil: Pair<Float, android.graphics.Matrix>? = null) {
        renderer?.renderFrontBufferedLayer(Live(paths, color, pencil))
    }

    /** Removes the live stroke (after the page shows the finished one). */
    fun clear() {
        renderer?.commit()
    }

    fun release() {
        renderer?.release(true)
        renderer = null
        active = false
    }

    override fun onTouchEvent(event: android.view.MotionEvent?): Boolean = false
}
