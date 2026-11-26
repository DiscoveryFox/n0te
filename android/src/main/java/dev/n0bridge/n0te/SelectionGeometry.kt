package dev.n0bridge.n0te

import kotlin.math.abs
import kotlin.math.atan2
import kotlin.math.ceil
import kotlin.math.cos
import kotlin.math.hypot
import kotlin.math.max
import kotlin.math.sin
import kotlin.math.sqrt

/**
 * [v] held between [lo] and [hi]; when there is no room between them ([hi] < [lo]: a strip of a
 * page only a few dp high, a short split pane) it goes to [lo] instead of throwing like
 * coerceIn, which crashed onDraw.
 */
fun clampTo(v: Float, lo: Float, hi: Float): Float = if (hi < lo) lo else v.coerceIn(lo, hi)

/**
 * A selection as a picture for the laptop ("An den Laptop", the LaTeX and graph readers): how
 * big it gets. Three pixels per point where that fits, else as many as fit: the whole selection
 * always goes, however long or wide it is (a lasso over three pages, a wide board).
 */
object SelectionPicture {
    const val MAX_SCALE = 3f
    const val MAX_SIDE = 8192
    /** 64 MB as ARGB: about two A4 pages at full scale. */
    const val MAX_PIXELS = 16L shl 20

    /** [scale] pixels per point; the picture [width] × [height] pixels holds all of it. */
    class Size(val scale: Float, val width: Int, val height: Int)

    /**
     * The white around the ink (points): half the widest brush in the selection (a marker's line
     * keeps some room around it), at least 6 pt.
     */
    fun margin(widestBrush: Float): Float = maxOf(6f, widestBrush * 0.5f)

    /** The picture for [w] × [h] points (margin included). */
    fun size(w: Float, h: Float, maxSide: Int = MAX_SIDE, maxPixels: Long = MAX_PIXELS): Size {
        val ww = w.coerceAtLeast(1f); val hh = h.coerceAtLeast(1f)
        val k = minOf(MAX_SCALE, maxSide / maxOf(ww, hh), sqrt(maxPixels / (ww.toDouble() * hh)).toFloat())
        return Size(k, ceil(ww * k).toInt().coerceIn(16, maxSide), ceil(hh * k).toInt().coerceIn(16, maxSide))
    }
}

/**
 * A straight line's ends as grips (a line held with the finger, or tapped with the lasso):
 * dragging one makes the line longer or shorter and keeps its angle; pulled clearly sideways,
 * the line turns, snapping like a line held while drawing (45° steps within 6°, 15° steps
 * within 3°, core `snap::HeldLine`).
 */
object LineGrip {
    /** Held, a straight line this long (points) or longer is selected alone, not with the writing beside it. */
    const val HOLD_MIN = 24f

    /** A stroke whose ends lie [chord] apart is a straight line when that is at least 92 % of its [length] (more than 4 pt). */
    fun straight(chord: Float, length: Float?): Boolean = chord > 4f && (length == null || chord >= length * 0.92f)

    /** The same from a stroke's ends (FfiStroke.ends: x0, y0, dx0, dy0, x1, y1, dx1, dy1, length), at least [minChord] long. */
    fun straight(ends: FloatArray, minChord: Float = 4f): Boolean {
        if (ends.size < 9) return false
        val chord = hypot(ends[4] - ends[0], ends[5] - ends[1])
        return chord >= minChord && straight(chord, ends[8])
    }

    /** An angle (degrees) snapped like a held line: to 45° steps within 6°, to 15° steps within 3°; null if neither. */
    fun snap(deg: Double): Double? {
        val s45 = Math.round(deg / 45.0) * 45.0
        val s15 = Math.round(deg / 15.0) * 15.0
        return when {
            abs(deg - s45) < 6.0 -> s45
            abs(deg - s15) < 3.0 -> s15
            else -> null
        }
    }

    /** Where the dragged end goes (relative to the fixed end); [turning]: the line turns; [snapDeg]: the step it snapped to. */
    class Drag(val x: Float, val y: Float, val turning: Boolean, val snapDeg: Int?)

    /**
     * The end of a line from its fixed end to ([ox], [oy]) dragged with the pen at ([px], [py])
     * (both relative to the fixed end). Along the line, the end slides to where the pen is along
     * it ([minLen] at least): only the length changes. More than [sideways] across the line, it
     * turns: the end follows the pen, snapping like a held line. Back within half of that (and
     * ahead of the fixed end) it keeps its angle again. [turning]: whether it turned until now.
     */
    fun drag(ox: Float, oy: Float, px: Float, py: Float, turning: Boolean, sideways: Float, minLen: Float = 4f): Drag {
        val l = hypot(ox, oy)
        if (l < 1e-3f) return Drag(px, py, true, null)
        val ux = ox / l; val uy = oy / l
        val along = px * ux + py * uy
        val across = abs(px * uy - py * ux)
        val turn = if (turning) !(across < sideways * 0.5f && along > 0f) else across > sideways
        if (!turn) {
            val t = max(along, minLen)
            return Drag(ux * t, uy * t, false, null)
        }
        val len = hypot(px, py)
        val s = snap(Math.toDegrees(atan2(py, px).toDouble())) ?: return Drag(px, py, true, null)
        return Drag((len * cos(Math.toRadians(s))).toFloat(), (len * sin(Math.toRadians(s))).toFloat(), true, Math.round(s).toInt())
    }
}
