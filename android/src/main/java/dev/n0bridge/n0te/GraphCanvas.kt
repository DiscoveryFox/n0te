package dev.n0bridge.n0te

import android.annotation.SuppressLint
import android.content.Context
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.Paint
import android.view.MotionEvent
import android.view.View
import android.view.ViewConfiguration
import androidx.compose.foundation.layout.fillMaxSize
import kotlin.math.hypot

/**
 * "Groß": the graph being edited fills the whole room, like GeoGebra's graphics view. It is
 * drawn here in a box the size of the room (the session is reframed to it: the same scale and
 * middle, more around); fingers and the pen act on it as on the page: tools tap, points and
 * sliders drag, empty space pans, two fingers zoom.
 */
@SuppressLint("ViewConstructor")
class GraphCanvasView(context: Context, var session: GraphSession, private val frameFrom: () -> android.graphics.RectF?) : View(context) {
    private val ink = Paint(Paint.ANTI_ALIAS_FLAG).apply { style = Paint.Style.FILL }
    private val paper = Paint().apply { color = Color.WHITE }
    /** The page's squares behind the graph (the colour of the paper's ruling). */
    private val ruling = Paint(Paint.ANTI_ALIAS_FLAG).apply { color = 0xFFD6DCE6.toInt(); style = Paint.Style.STROKE }
    private val density = resources.displayMetrics.density
    private val slop = ViewConfiguration.get(context).scaledTouchSlop
    /**
     * Pixels per page point: the graph's box on the page fits the room, but lines and numbers
     * never get bigger than on a page at about full width (like GeoGebra: more of the plane, not
     * bigger writing).
     */
    private var k = 1f

    private var capture = false
    private var pinch = 0f
    private var downX = 0f
    private var downY = 0f
    private var downAt = 0L
    private var moved = false
    private var hit: String? = null

    override fun onSizeChanged(w: Int, h: Int, oldw: Int, oldh: Int) {
        super.onSizeChanged(w, h, oldw, oldh)
        // Only the height changed (the graph keyboard came or went): the graph stays where it is.
        if (w == oldw && oldh > 0 && h > 0 && k > 0f) {
            session.reframe(0f, 0f, width / k, height / k, keepTop = true)
            invalidate()
            return
        }
        frame()
    }

    /** The session in a box the size of this view (again when the room changes). */
    fun frame() {
        if (width <= 0 || height <= 0) return
        val stored = frameFrom() ?: return
        k = minOf(width / stored.width(), height / stored.height(), 1.9f * density).coerceAtLeast(0.05f)
        session.reframe(0f, 0f, width / k, height / k)
        session.bigAt = android.graphics.PointF(stored.centerX() - width / k / 2f, stored.centerY() - height / k / 2f)
        invalidate()
    }

    override fun onDraw(c: Canvas) {
        // Inside Compose nothing clips this view: keep to its own room (drawColor would paint the panel over).
        c.save()
        c.clipRect(0, 0, width, height)
        c.drawRect(0f, 0f, width.toFloat(), height.toFloat(), paper)
        val p = session.shown()
        // Drawn only once it is in this view's box (right after a change of room it may not be yet).
        if (p.x == 0f && p.y == 0f) {
            drawSquares(c)
            c.scale(k, k)
            Plots.strokes(p)?.forEach { s -> ink.color = s.color; c.drawPath(s.path, ink) }
        }
        c.restore()
    }

    /**
     * Squared paper: its squares behind the graph, where they lie on the page (the room is the
     * page shifted by [GraphSession.bigAt]) — the graph keeps to them like on the page.
     */
    private fun drawSquares(c: Canvas) {
        val s = session.squares
        val at = session.bigAt ?: return
        if (s <= 0f || s * k < 4f) return
        ruling.strokeWidth = kotlin.math.max(1f, density * 0.6f)
        // Page lines at multiples of s; in the room at x with x + at.x = n·s.
        var x = (Math.floorMod(Math.round(-at.x * 1000).toLong(), Math.round(s * 1000).toLong()) / 1000f)
        while (x * k <= width) { c.drawLine(x * k, 0f, x * k, height.toFloat(), ruling); x += s }
        var y = (Math.floorMod(Math.round(-at.y * 1000).toLong(), Math.round(s * 1000).toLong()) / 1000f)
        while (y * k <= height) { c.drawLine(0f, y * k, width.toFloat(), y * k, ruling); y += s }
    }

    @SuppressLint("ClickableViewAccessibility")
    override fun onTouchEvent(ev: MotionEvent): Boolean {
        val s = session
        fun px(i: Int) = ev.getX(i) / k
        fun py(i: Int) = ev.getY(i) / k
        when (ev.actionMasked) {
            MotionEvent.ACTION_DOWN -> {
                capture = true; pinch = 0f; moved = false
                downX = ev.x; downY = ev.y; downAt = ev.eventTime
                hit = uniffi.n0te_ffi.plotHit(s.draft, px(0), py(0), 14f * density / k)
                s.down(px(0), py(0), hit)
            }
            MotionEvent.ACTION_POINTER_DOWN -> if (capture && ev.pointerCount == 2) {
                pinch = hypot(ev.getX(0) - ev.getX(1), ev.getY(0) - ev.getY(1))
                moved = true
                s.up()
            }
            MotionEvent.ACTION_MOVE -> if (capture) {
                if (ev.pointerCount >= 2 && pinch > 0f) {
                    val d = hypot(ev.getX(0) - ev.getX(1), ev.getY(0) - ev.getY(1))
                    if (d > 1f) s.pinch(d / pinch, (px(0) + px(1)) / 2f, (py(0) + py(1)) / 2f)
                    pinch = d
                } else if (pinch == 0f) {
                    if (hypot(ev.x - downX, ev.y - downY) > slop) moved = true
                    s.move(px(0), py(0))
                }
            }
            MotionEvent.ACTION_UP, MotionEvent.ACTION_CANCEL -> if (capture) {
                capture = false
                if (pinch == 0f) {
                    s.up()
                    if (!moved && ev.eventTime - downAt < 500 && ev.actionMasked == MotionEvent.ACTION_UP) s.tap(px(0), py(0), hit)
                }
                s.settle()
                pinch = 0f
            }
        }
        return true
    }
}

/** The graph filling its room ("Groß"); [stamp] changes whenever it must be drawn again. */
@androidx.compose.runtime.Composable
fun GraphCanvas(s: GraphSession, stamp: Int, frameFrom: () -> android.graphics.RectF?, modifier: androidx.compose.ui.Modifier = androidx.compose.ui.Modifier) {
    androidx.compose.ui.viewinterop.AndroidView(
        factory = { ctx -> GraphCanvasView(ctx, s, frameFrom) },
        update = { v ->
            if (v.session !== s) { v.session = s; v.frame() }
            v.tag = stamp
            v.invalidate()
        },
        modifier = modifier.fillMaxSize(),
    )
}
