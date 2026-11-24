package dev.n0bridge.n0te

import kotlin.math.ceil
import kotlin.math.floor
import kotlin.math.max
import kotlin.math.min
import kotlin.math.sqrt

/**
 * "Inside the lasso?" for many points at once (every outline edge of every stroke on a page
 * full of handwriting): the lasso is laid on a grid once; a point in a cell wholly inside or
 * outside is answered from the grid, only one in a cell the lasso's line crosses is tested
 * against the polygon itself. The same answers as [NotebookView.pointInPolygon], over ten times
 * faster for a hand-drawn lasso of hundreds of points around a page of handwriting.
 */
class LassoMask(private val poly: FloatArray, maxCells: Int = 1 shl 20) {
    private val n = poly.size / 2
    /** A lasso of a few points (a box) is tested directly: quicker than laying out a grid. */
    private val direct = n < 24
    private val x0: Float
    private val y0: Float
    private val cell: Float
    private val cols: Int
    private val rows: Int
    /** 0 outside, 1 inside, 2 the line crosses it (test exactly). */
    private val grid: ByteArray

    init {
        var ax = Float.MAX_VALUE; var ay = Float.MAX_VALUE; var bx = -Float.MAX_VALUE; var by = -Float.MAX_VALUE
        for (i in 0 until n) { ax = min(ax, poly[2 * i]); ay = min(ay, poly[2 * i + 1]); bx = max(bx, poly[2 * i]); by = max(by, poly[2 * i + 1]) }
        if (n < 3) { ax = 0f; ay = 0f; bx = 1f; by = 1f }
        val w = max(bx - ax, 1f); val h = max(by - ay, 1f)
        // About 2 pt cells, fewer for a huge lasso.
        cell = max(2f, sqrt(w * h / maxCells))
        cols = ceil(w / cell).toInt() + 1
        rows = ceil(h / cell).toInt() + 1
        x0 = ax; y0 = ay
        grid = ByteArray(if (direct) 0 else cols * rows)
        if (!direct) {
            // Cells the lasso's edges pass through (walked in small steps).
            for (i in 0 until n) {
                val j = (i + 1) % n
                val px = poly[2 * i]; val py = poly[2 * i + 1]; val qx = poly[2 * j]; val qy = poly[2 * j + 1]
                val steps = (max(kotlin.math.abs(qx - px), kotlin.math.abs(qy - py)) / (cell * 0.5f)).toInt() + 1
                for (s in 0..steps) {
                    val t = s / steps.toFloat()
                    mark(px + (qx - px) * t, py + (qy - py) * t)
                }
            }
            // Every other cell: inside or not by its middle (a line no edge crosses: all of the
            // cell is on the same side), row by row between the edges' crossings (even-odd).
            val xs = FloatArray(n)
            for (r in 0 until rows) {
                val yc = y0 + (r + 0.5f) * cell
                var m = 0
                for (i in 0 until n) {
                    val j = if (i == 0) n - 1 else i - 1
                    val xi = poly[2 * i]; val yi = poly[2 * i + 1]; val xj = poly[2 * j]; val yj = poly[2 * j + 1]
                    if ((yi > yc) != (yj > yc)) xs[m++] = (xj - xi) * (yc - yi) / (yj - yi) + xi
                }
                java.util.Arrays.sort(xs, 0, m)
                var k = 0
                while (k + 1 < m) {
                    val from = max(0, ceil((xs[k] - x0) / cell - 0.5f).toInt())
                    val to = min(cols - 1, floor((xs[k + 1] - x0) / cell - 0.5f).toInt())
                    for (c in from..to) if (grid[r * cols + c].toInt() == 0) grid[r * cols + c] = 1
                    k += 2
                }
            }
        }
    }

    /** The cell of (x, y) and its neighbours count as crossed (a step can graze a corner). */
    private fun mark(x: Float, y: Float) {
        val c = floor((x - x0) / cell).toInt(); val r = floor((y - y0) / cell).toInt()
        for (dr in -1..1) for (dc in -1..1) {
            val rr = r + dr; val cc = c + dc
            if (rr in 0 until rows && cc in 0 until cols) grid[rr * cols + cc] = 2
        }
    }

    fun contains(x: Float, y: Float): Boolean {
        if (n < 3) return false
        if (direct) return NotebookView.pointInPolygon(x, y, poly)
        val c = floor((x - x0) / cell).toInt(); val r = floor((y - y0) / cell).toInt()
        if (c < 0 || r < 0 || c >= cols || r >= rows) return false
        return when (grid[r * cols + c].toInt()) {
            0 -> false
            1 -> true
            else -> NotebookView.pointInPolygon(x, y, poly)
        }
    }
}
