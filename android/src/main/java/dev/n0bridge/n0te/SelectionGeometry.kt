package dev.n0bridge.n0te

import kotlin.math.ceil
import kotlin.math.sqrt

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
