package dev.n0bridge.n0te

import android.content.Context
import android.view.View
import android.view.ViewGroup

/**
 * Where two notebooks go side by side (one above the other when the screen stands upright),
 * without any screen: the panes' rectangles, where the divider snaps to, when dragging it to
 * an edge closes a pane.
 */
/** A pane's rectangle (px), testable without Android. */
data class Area(val left: Int, val top: Int, val right: Int, val bottom: Int) {
    val isEmpty get() = right <= left || bottom <= top
    fun width() = right - left
    fun height() = bottom - top
    fun contains(x: Int, y: Int) = !isEmpty && x >= left && x < right && y >= top && y < bottom

    companion object { val NONE = Area(0, 0, 0, 0) }
}

object SplitGeometry {
    /** The divider snaps to a third, the half or two thirds. */
    val SNAPS = floatArrayOf(1f / 3f, 0.5f, 2f / 3f)
    /** Dragged closer to an edge than this (share of the room): the pane there closes. */
    const val CLOSE_AT = 0.14f

    /** Side by side on a landscape screen, above each other on an upright one. */
    fun sideBySide(w: Int, h: Int) = w >= h

    /**
     * The panes' rectangles in a [w]×[h] room: [n] panes (1 or 2), the first taking [fraction]
     * of it, [gap] px between them; [maxed] (an index, or -1) takes the whole room and the
     * others get an empty rectangle.
     */
    fun rects(w: Int, h: Int, n: Int, fraction: Float, gap: Int, maxed: Int = -1): List<Area> {
        if (n <= 0) return emptyList()
        if (n == 1) return listOf(Area(0, 0, w, h))
        if (maxed in 0 until n) return List(n) { if (it == maxed) Area(0, 0, w, h) else Area.NONE }
        val f = fraction.coerceIn(0.1f, 0.9f)
        return if (sideBySide(w, h)) {
            val cut = (w * f).toInt()
            listOf(Area(0, 0, cut - gap / 2, h), Area(cut + (gap - gap / 2), 0, w, h))
        } else {
            val cut = (h * f).toInt()
            listOf(Area(0, 0, w, cut - gap / 2), Area(0, cut + (gap - gap / 2), w, h))
        }
    }

    /** What letting go of the divider at [fraction] does. */
    sealed class Release {
        /** Rests at this share. */
        data class Snap(val fraction: Float) : Release()
        /** The pane with this index closes (dragged to its edge). */
        data class Close(val pane: Int) : Release()
    }

    fun release(fraction: Float): Release = when {
        fraction < CLOSE_AT -> Release.Close(0)
        fraction > 1f - CLOSE_AT -> Release.Close(1)
        else -> Release.Snap(SNAPS.minByOrNull { kotlin.math.abs(it - fraction) }!!)
    }

    /** While dragging: the pane that would close if let go now (or -1). */
    fun closing(fraction: Float): Int = when {
        fraction < CLOSE_AT -> 0
        fraction > 1f - CLOSE_AT -> 1
        else -> -1
    }

    /** What is kept when the activity is rebuilt: notebook ids, then "main|active|fraction". */
    fun saveState(ids: List<String>, main: Int, active: Int, fraction: Float): String =
        (ids + "$main|$active|$fraction").joinToString("\n")

    /** [saveState] read back: ids, main, active, fraction (null when unreadable). */
    fun readState(s: String?): Saved? {
        val parts = s?.split('\n')?.takeIf { it.size >= 2 } ?: return null
        val ids = parts.dropLast(1).filter { it.isNotBlank() }
        val f = parts.last().split('|')
        if (ids.isEmpty() || ids.size > 2 || f.size != 3) return null
        val main = f[0].toIntOrNull()?.takeIf { it in ids.indices } ?: 0
        val active = f[1].toIntOrNull()?.takeIf { it in ids.indices } ?: 0
        val frac = f[2].toFloatOrNull()?.takeIf { it in 0.1f..0.9f } ?: 0.5f
        return Saved(ids, main, active, frac)
    }

    data class Saved(val ids: List<String>, val main: Int, val active: Int, val fraction: Float)
}

/**
 * Lays its children (one container per pane) out as [SplitGeometry] says and tells where they
 * went, so the controls drawn over them (in one Compose layer) line up.
 */
class SplitLayout(context: Context) : ViewGroup(context) {
    var fraction = 0.5f
        set(v) { if (field != v) { field = v; requestLayout() } }
    /** The child taking the whole room (index), or -1. */
    var maxed = -1
        set(v) { if (field != v) { field = v; requestLayout() } }
    /** Room between the panes (px). */
    var gap = 0
    /** After every layout: the children's rectangles (in this view, which fills the window). */
    var onLaidOut: ((List<Area>) -> Unit)? = null
    private var rects: List<Area> = emptyList()

    override fun onMeasure(widthMeasureSpec: Int, heightMeasureSpec: Int) {
        val w = MeasureSpec.getSize(widthMeasureSpec)
        val h = MeasureSpec.getSize(heightMeasureSpec)
        setMeasuredDimension(w, h)
        rects = SplitGeometry.rects(w, h, childCount, fraction, gap, maxed)
        for (i in 0 until childCount) {
            val r = rects[i]
            val c = getChildAt(i)
            // A pane hidden while the other is shown alone is GONE (set by the activity).
            if (!r.isEmpty && c.visibility != View.GONE) c.measure(MeasureSpec.makeMeasureSpec(r.width(), MeasureSpec.EXACTLY), MeasureSpec.makeMeasureSpec(r.height(), MeasureSpec.EXACTLY))
        }
    }

    override fun onLayout(changed: Boolean, l: Int, t: Int, r: Int, b: Int) {
        for (i in 0 until childCount) {
            val rc = rects.getOrNull(i) ?: continue
            val c = getChildAt(i)
            if (!rc.isEmpty && c.visibility != View.GONE) c.layout(rc.left, rc.top, rc.right, rc.bottom)
        }
        onLaidOut?.invoke(rects)
    }
}
