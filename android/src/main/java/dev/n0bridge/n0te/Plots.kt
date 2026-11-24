package dev.n0bridge.n0te

import uniffi.n0te_ffi.FfiBackground
import uniffi.n0te_ffi.FfiPlot
import uniffi.n0te_ffi.plotStrokes

/**
 * Coordinate systems as ink: the core works out a plot's look (axes, numbers, curves, drawn
 * as if by hand) and it is drawn like strokes. Worked out in the background, kept for a while.
 */
object Plots {
    private val cache = android.util.LruCache<FfiPlot, List<InkStroke>>(48)
    /** A plot's last look by id: shown while its new one (after an edit) is worked out. */
    private val last = HashMap<String, List<InkStroke>>()
    private val busy = HashSet<FfiPlot>()

    /** Called on the main thread when a look is ready (redraw). */
    var listener: (() -> Unit)? = null

    /** The plot's strokes (page points), or its previous look / null while they are worked out. */
    fun strokes(p: FfiPlot): List<InkStroke>? {
        cache.get(p)?.let { return it }
        if (busy.add(p)) {
            N0te.io.execute {
                val s = runCatching { plotStrokes(p).map { InkStroke.of(it) } }.getOrDefault(emptyList())
                N0te.main.post {
                    busy.remove(p)
                    cache.put(p, s)
                    if (p.id.isNotEmpty()) last[p.id] = s
                    listener?.invoke()
                }
            }
        }
        return last[p.id]
    }

    /** Axes and numbers: nearly black ink. */
    const val AXES = 0xFF202428.toInt()

    /** Colours for functions: the pen colours but the black ones. */
    private fun inks(): List<Int> = Tools.inks.filter { c -> val r = (c shr 16) and 0xFF; val g = (c shr 8) and 0xFF; val b = c and 0xFF; r + g + b > 150 }.ifEmpty { Ui.inks.drop(1) }

    /** The colour for a new function: the first pen colour not used yet. */
    fun nextColor(used: List<Int>): Int {
        val inks = inks()
        return inks.firstOrNull { it !in used } ?: inks[used.size % inks.size]
    }

    /** The colour after [c] (tapping a function's colour). */
    fun colorAfter(c: Int): Int {
        val inks = inks()
        return inks[(inks.indexOf(c) + 1).mod(inks.size)]
    }

    /** A number for a range field: German comma, no trailing zeros. */
    fun format(v: Double): String {
        if (v == Math.rint(v) && kotlin.math.abs(v) < 1e12) return v.toLong().toString()
        return "%.6g".format(java.util.Locale.ROOT, v).let { if ('e' in it) it else it.trimEnd('0').trimEnd('.') }.replace('.', ',')
    }

    /** A typed range number ("-2,5", "−3"), or null. */
    fun number(s: String): Double? = s.trim().replace('−', '-').replace(',', '.').toDoubleOrNull()?.takeIf { it.isFinite() }

    /**
     * A new, empty coordinate system centred at (cx, cy) on a page [pw] × [ph] (0: a board):
     * on squared paper 12 × 8 squares with a square as the unit, snapped to the squares (the axes
     * on lines); else 240 × 160 pt with 20 pt to the unit and a grid of its own.
     */
    fun fresh(cx: Float, cy: Float, pw: Float, ph: Float, background: FfiBackground, spacing: Float): FfiPlot {
        val squares = background == FfiBackground.GRID && spacing > 2f
        val unit = if (squares) spacing else 20f
        var cols = 12; var rows = 8
        if (pw > 0f) { while (cols > 4 && cols * unit > pw * 0.85f) cols -= 2; while (rows > 4 && rows * unit > ph * 0.6f) rows -= 2 }
        val w = cols * unit; val h = rows * unit
        var x = cx - w / 2; var y = cy - h / 2
        // A square's room to the page's edges: the axes' heads and numbers reach out of the box.
        val m = if (squares) unit else 14f
        if (pw > 0f) { x = x.coerceIn(m, (pw - w - m).coerceAtLeast(m)); y = y.coerceIn(m, (ph - h - m).coerceAtLeast(m)) }
        if (squares) { x = kotlin.math.floor(x / unit) * unit; y = kotlin.math.floor(y / unit) * unit }
        return FfiPlot("", x, y, w, h, -cols / 2.0, cols / 2.0, -rows / 2.0, rows / 2.0, axes = true, grid = !squares, minor = false, clean = false,
            color = AXES.toUInt(), width = 1.2f, pi = false, xName = "", yName = "", snap = 0u, align = squares, interactive = false, objs = emptyList())
    }
}
