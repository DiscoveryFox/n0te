package dev.n0bridge.n0te

import android.app.Activity
import android.os.Debug
import android.os.Handler
import android.os.HandlerThread
import android.os.SystemClock
import android.util.Log
import android.view.Choreographer
import android.view.FrameMetrics
import android.view.Window
import uniffi.n0te_ffi.FfiBrush
import kotlin.math.exp
import kotlin.math.ln
import kotlin.math.sin

/**
 * Frame times for performance work, driven over adb (the emulator; nothing in the UI starts it):
 *
 *     adb shell am start -n dev.n0bridge.n0te/.NotebookActivity --es notebook_id ID \
 *         --es sheet_id SHEET --es n0te.bench open,fit,pan,zoom,dtap,write,board
 *
 * Each step moves the view frame by frame as a hand would (no input events, so every run is the
 * same) and logs a line to logcat (tag N0TE, "bench …"): the intervals between frames, the time
 * in NotebookView.onDraw (and in it re-recording ink), FrameMetrics' total per frame, memory.
 * Compare runs on the same device and build setup only.
 */
object PerfBench {
    const val EXTRA = "n0te.bench"

    /** While a bench runs: NotebookView adds up its time in onDraw and in re-recording ink. */
    @JvmField var on = false
    @JvmField var drawNs = 0L
    @JvmField var recordNs = 0L
    /** Recordings made (pieces of ink, tiles) since the step began. */
    @JvmField var recorded = 0
    /** Why pieces of ink were recorded: new or changed, dropped by Android, pencil grain of a new zoom. */
    @JvmField val why = IntArray(5)

    /** A step of [frames] frames, or until [until] (uptime ms, set when it starts) when [seconds] > 0. */
    private class Step(val name: String, val frames: Int, val seconds: Int = 0, val frame: (Int) -> Unit) {
        var until = 0L
    }
    /** The page view the bench runs in (for the strokes held). */
    private var target: NotebookView? = null

    private val intervals = ArrayList<Float>()
    private val draws = ArrayList<Float>()
    private val records = ArrayList<Float>()
    private val totals = ArrayList<Float>()
    private var metricsThread: HandlerThread? = null

    /** Runs [script] (steps separated by commas) on [view] once its notebook is laid out; [createdAt]: the activity's onCreate. */
    fun start(activity: Activity, view: NotebookView, sheetId: String?, script: String, createdAt: Long) {
        val names = script.split(',').map { it.trim() }.filter { it.isNotEmpty() }
        target = view
        on = true
        listenFrameMetrics(activity.window)
        val choreo = Choreographer.getInstance()
        var last = 0L
        var steps = ArrayDeque<Step>()
        var cur: Step? = null
        var i = 0
        var opened = false
        var ready = false
        var openWorst = 0f
        fun page(): Placed? = view.model?.placed?.let { pl -> pl.firstOrNull { it.sheetId == sheetId } ?: pl.firstOrNull() }
        fun build() {
            val p = page() ?: return
            for (spec in names) {
                // "pan" or "pan:60": a step and how many frames it takes (fewer on a slow emulator).
                val n = spec.substringBefore(':')
                val len = spec.substringAfter(':', "").toIntOrNull()
                fun frames(d: Int) = (len ?: d).coerceAtLeast(2)
                when (n) {
                    "fit" -> steps += Step("fit", frames(20)) { if (it == 0) view.fitSheet(p, whole = true, animate = false) }
                    "idle" -> steps += Step("idle", frames(60)) {}
                    "tour" -> {
                        // Every page in turn, whole, a few frames each (all of them loaded).
                        val per = frames(20)
                        val pages = view.model?.placed?.filter { !it.isBoard }.orEmpty()
                        steps += Step("tour", per * pages.size) { f -> if (f % per == 0) pages.getOrNull(f / per)?.let { view.fitSheet(it, whole = true, animate = false) } }
                    }
                    "rest" -> {
                        // Back on the page and staying there for a while (pages out of view may let go of their strokes).
                        steps += Step("rest", Int.MAX_VALUE, seconds = len ?: 30) { f -> if (f == 0) view.fitSheet(p, whole = true, animate = false) }
                    }
                    "pan" -> {
                        // Zoomed in 4× from the whole page, then across and down the page.
                        var s = 0f; var x0 = 0f; var y0 = 0f
                        val k = frames(120)
                        steps += Step("pan", k) { f ->
                            if (f == 0) {
                                view.fitSheet(p, whole = true, animate = false)
                                s = view.camera()[0] * 4f
                                x0 = view.width / 2f - (p.rect.left + p.rect.width() * 0.3f) * s
                                y0 = view.height / 2f - (p.rect.top + p.rect.height() * 0.2f) * s
                            }
                            val t = f * 120f / k
                            view.setCamera(floatArrayOf(s, x0 + 900f * sin(t / 20f), y0 - t * 28f), animate = false)
                        }
                    }
                    "zoom" -> {
                        // A pinch from the whole page to 16× and back about a point of the page.
                        var s0 = 0f
                        val k = frames(120)
                        val wx = p.rect.left + p.rect.width() * 0.4f; val wy = p.rect.top + p.rect.height() * 0.35f
                        steps += Step("zoom", k) { f ->
                            if (f == 0) { view.fitSheet(p, whole = true, animate = false); s0 = view.camera()[0] }
                            val tri = if (f < k / 2) f / (k / 2f) else (k - f) / (k / 2f)
                            val s = s0 * exp(ln(16f) * tri)
                            view.setCamera(floatArrayOf(s, view.width / 2f - wx * s, view.height / 2f - wy * s), animate = false)
                        }
                    }
                    "dtap" -> {
                        // Zoomed in, then a double tap's fit (animated, 260 ms), again and again.
                        val k = frames(120)
                        steps += Step("dtap", k) { f ->
                            when (f % 30) {
                                0 -> {
                                    view.fitSheet(p, whole = true, animate = false)
                                    val s = view.camera()[0] * 6f
                                    val wx = p.rect.left + p.rect.width() * (0.2f + 0.15f * (f / 30 % 4)); val wy = p.rect.top + p.rect.height() * 0.3f
                                    view.setCamera(floatArrayOf(s, view.width / 2f - wx * s, view.height / 2f - wy * s), animate = false)
                                }
                                6 -> view.fitSheet(p, whole = true)
                            }
                        }
                    }
                    "write" -> {
                        // A stroke in the middle of the screen every 8 frames, as the pen leaves them.
                        val brush = FfiBrush(1.6f, 0.6f, 0xFF111111u, 0u)
                        steps += Step("write", frames(120)) { f ->
                            if (f == 0) {
                                view.fitSheet(p, whole = true, animate = false)
                                val s = view.camera()[0] * 4f
                                view.setCamera(floatArrayOf(s, view.width / 2f - (p.rect.left + p.rect.width() * 0.5f) * s, view.height / 2f - (p.rect.top + p.rect.height() * 0.5f) * s), animate = false)
                            }
                            if (f % 8 == 4) {
                                val c = view.camera()
                                val x = (view.width / 2f - c[1]) / c[0] - p.rect.left + (f / 8 % 10) * 14f
                                val y = (view.height / 2f - c[2]) / c[0] - p.rect.top + (f / 80) * 24f
                                val m = 40
                                val samples = FloatArray(m * 4) { q ->
                                    val j = q / 4; val t = j / (m - 1f)
                                    when (q % 4) { 0 -> x + 10f * t + 3f * sin(t * 12f); 1 -> y + 8f * sin(t * 9f); 2 -> 0.5f; else -> j * 8f }
                                }
                                val bytes = bytesOf(samples)
                                val preview = floatsOf(uniffi.n0te_ffi.strokeOutline(brush, bytes))
                                view.model?.addStroke(p.sheetId, brush, bytes, preview)
                            }
                        }
                    }
                    "board" -> {
                        val b = view.model?.placed?.firstOrNull { it.isBoard }
                        if (b != null) {
                            var s0 = 0f
                            val k = frames(120)
                            steps += Step("board", k + 20) { f0 ->
                                // Into the board (its own 260 ms animation), then out to a tenth (every
                                // cluster in view) and back, then across; the minimap shows as when panning by hand.
                                if (f0 == 0) view.enterBoard(b)
                                if (f0 < 20) { s0 = view.camera()[0]; return@Step }
                                val f = f0 - 20
                                val a = k * 2 / 3
                                val s = if (f < a) s0 * exp(ln(0.1f) * (if (f < a / 2) f / (a / 2f) else (a - f) / (a / 2f))) else s0
                                val wx = 700f + (if (f >= a) (f - a) * 90f * 120f / k else 0f); val wy = 500f
                                view.setCamera(floatArrayOf(s, view.width / 2f - wx * s, view.height / 2f - wy * s), animate = false)
                                view.showMinimap()
                            }
                        }
                    }
                }
            }
        }
        val t0 = SystemClock.uptimeMillis()
        val tick = object : Choreographer.FrameCallback {
            override fun doFrame(now: Long) {
                val dt = if (last == 0L) 0f else (now - last) / 1e6f
                last = now
                val d = drawNs / 1e6f; val r = recordNs / 1e6f
                drawNs = 0L; recordNs = 0L
                if (!opened) {
                    openWorst = maxOf(openWorst, d)
                    val st = sheetId?.let { view.model?.sheets?.get(it) }
                    if (ready && d > 0f) {
                        // The frame after the one that drew the loaded page.
                        opened = true
                        val strokes = st?.strokes?.size ?: 0
                        log("open", "${SystemClock.uptimeMillis() - createdAt} ms since onCreate ($strokes strokes on the page), first frame with its ink ${"%.1f".format(d)} ms, worst frame on the way ${"%.1f".format(openWorst)} ms; ${memory()}")
                        build()
                    } else if (!ready && view.width > 0 && view.model?.placed?.isNotEmpty() == true && (sheetId == null || st?.handle != null)) {
                        ready = true
                    } else if (SystemClock.uptimeMillis() - t0 > 90_000) {
                        log("open", "timed out"); finish(); return
                    }
                    choreo.postFrameCallback(this); return
                }
                val s = cur
                if (s != null && i > 0) {
                    intervals += dt; draws += d; records += r
                }
                if (s != null && s.seconds > 0 && i == 0) s.until = SystemClock.uptimeMillis() + s.seconds * 1000L
                if (s == null || i >= s.frames || (s.seconds > 0 && SystemClock.uptimeMillis() > s.until)) {
                    if (s != null) report(s.name)
                    cur = steps.removeFirstOrNull()
                    i = 0
                    if (cur == null) { finish(); return }
                    intervals.clear(); draws.clear(); records.clear(); recorded = 0; why.fill(0); synchronized(totals) { totals.clear() }
                }
                cur?.frame?.invoke(i)
                i++
                view.invalidate()
                choreo.postFrameCallback(this)
            }
        }
        choreo.postFrameCallback(tick)
    }

    private fun finish() {
        on = false
        metricsThread?.quitSafely(); metricsThread = null
        log("done", "")
    }

    private fun listenFrameMetrics(w: Window) {
        val t = HandlerThread("n0te-bench").also { it.start(); metricsThread = it }
        w.addOnFrameMetricsAvailableListener({ _, fm, _ ->
            if (on) synchronized(totals) { totals += fm.getMetric(FrameMetrics.TOTAL_DURATION) / 1e6f }
        }, Handler(t.looper))
    }

    private fun report(name: String) {
        val fm = synchronized(totals) { ArrayList(totals) }
        val jank = intervals.count { it > 20f }
        log(name, "frames ${intervals.size}, interval ${stats(intervals)}, janky ${if (intervals.isEmpty()) 0 else jank * 100 / intervals.size}%; onDraw ${stats(draws)} (re-record ${stats(records)}, sum ${"%.0f".format(records.sum())} ms, $recorded recordings: ${why[0]} new, ${why[1]} dropped, ${why[2]} grain; ${why[3]} plans, ${why[4]} chunks made); FrameMetrics total ${stats(fm)}; ${memory()}")
    }

    private fun stats(v: List<Float>): String {
        if (v.isEmpty()) return "-"
        val s = v.sorted()
        fun q(p: Float) = s[((s.size - 1) * p).toInt()]
        return "p50 %.1f p90 %.1f p99 %.1f max %.1f ms".format(q(0.5f), q(0.9f), q(0.99f), s.last())
    }

    private fun memory(): String {
        val mi = Debug.MemoryInfo()
        Debug.getMemoryInfo(mi)
        fun mb(k: String) = (mi.getMemoryStat(k)?.toIntOrNull() ?: 0) / 1024
        val held = target?.model?.sheets?.values?.sumOf { it.strokes.size } ?: 0
        return "PSS ${mb("summary.total-pss")} MB (graphics ${mb("summary.graphics")}, native ${mb("summary.native-heap")}, java ${mb("summary.java-heap")}), $held strokes held"
    }

    private fun log(step: String, msg: String) = Log.i(N0te.TAG, "bench $step: $msg")
}
