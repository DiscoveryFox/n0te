package dev.n0bridge.n0te

import kotlin.math.PI
import kotlin.math.abs
import kotlin.math.atan2
import kotlin.math.cos
import kotlin.math.hypot
import kotlin.math.sin
import kotlin.math.sqrt

/**
 * Turns a hand-drawn stroke into a clean shape when the pen is held still at its end:
 * a straight line, a triangle, a rectangle or an ellipse. Input and output are samples as
 * x, y, pressure, t quads; returns null when the stroke is no recognisable shape.
 */
object Shapes {
    /** Tests: why a shape wasn't taken (reason per rejection). */
    internal var trace: ((String) -> Unit)? = null

    fun recognize(s: FloatArray, n: Int, still: Float = 0f, arrows: Boolean = true): FloatArray? = detect(s, n, still, arrows)?.samples

    /**
     * A recognised shape: its kind ("line", "triangle", "rectangle", "quad", "polygon",
     * "ellipse"), its clean samples, and what it is made of: the corners (x, y pairs; a line's
     * two ends) or the ellipse.
     */
    class Shape(val kind: String, val samples: FloatArray, val corners: FloatArray? = null, val ellipse: Fit? = null) {
        operator fun component1() = kind
        operator fun component2() = samples
    }

    /**
     * The shape a stroke is, or null. [still]: how far (page points) the pen may stray and still
     * count as resting, as the view decides when it held still; what it did while resting is left out.
     */
    fun detect(s: FloatArray, n1: Int, still: Float = 0f, arrows: Boolean = true): Shape? {
        val n0 = if (still > 0f) restAt(s, n1, still) + 1 else n1
        if (n0 < 4) return null
        var p = 0f
        for (i in 0 until n0) p += s[i * 4 + 2]
        p /= n0
        val t0 = s[3]; val t1 = s[(n0 - 1) * 4 + 3]
        // Where the pen lands and where it rests at the end (that is when this runs), it trembles
        // on the spot: tiny back-and-forths that would read as sharp corners. They tell nothing
        // about the shape, so the path starts where the pen sets off and ends where it came to rest.
        val keep = steady(s, n0)
        val n = keep.last - keep.first + 1
        if (n < 4) return null
        val xs = FloatArray(n) { s[(keep.first + it) * 4] }
        val ys = FloatArray(n) { s[(keep.first + it) * 4 + 1] }
        val len = pathLength(xs, ys)
        if (len < 24f) return null
        val gap = hypot(xs[0] - xs[n - 1], ys[0] - ys[n - 1])
        // Closed also when the end stops a bit short of the start (hands rarely meet exactly).
        val closed = gap < len * 0.2f
        if (!closed) {
            // Open: a line if every point is near the chord.
            val chord = hypot(xs[n - 1] - xs[0], ys[n - 1] - ys[0])
            val straight = chord >= 24f && (1 until n - 1).all { NotebookView.segDist(xs[it], ys[it], xs[0], ys[0], xs[n - 1], ys[n - 1]) <= chord * 0.06f + 2f }
            if (!straight) {
                // Not straight: an arrow (shaft and head in one stroke), or nothing.
                return if (arrows) Arrows.fromStroke(s, keep.first, keep.last)?.let { Shape("arrow", it) } else null
            }
            val ends = floatArrayOf(xs[0], ys[0], xs[n - 1], ys[n - 1])
            return Shape("line", poly(ends, p, t0, t1, close = false), ends)
        }
        // The path as drawn, without what ran past the start.
        val cut = overshootCut(xs, ys)
        val tx = xs.copyOf(cut + 1); val ty = ys.copyOf(cut + 1)
        // Straight sides with corners win, unless it is almost perfectly round (a circle drawn
        // with a little wobble can look like it has corners; a rounded square can look round).
        val (ex, ey) = resampleOpen(tx, ty, 120)
        val fit = fitEllipse(ex, ey)
        val corners = sideCorners(tx, ty)
        if (corners != null && !(fit != null && fit.err < 0.045f)) {
            val (kind, c) = when (corners.size) {
                3 -> "triangle" to corners.flatMap { listOf(it[0], it[1]) }.toFloatArray()
                4 -> squareUp(corners)
                else -> "polygon" to corners.flatMap { listOf(it[0], it[1]) }.toFloatArray()
            }
            return Shape(kind, poly(c, p, t0, t1, close = true), c)
        }
        return fit?.takeIf { it.err < 0.15f }?.let { Shape("ellipse", ellipseSamples(it, p, t0, t1), ellipse = it) }
    }

    /** The samples that say something about the shape: up to where the pen came to rest ([still], see [detect]), trembling trimmed. */
    fun trimmed(s: FloatArray, n: Int, still: Float): IntRange? {
        val n0 = if (still > 0f) restAt(s, n, still) + 1 else n
        if (n0 < 2) return null
        return steady(s, n0)
    }

    /** The sample where the pen came to rest: the last one more than [still] from the rest before it (as the view tracks it). */
    fun restAt(s: FloatArray, n: Int, still: Float): Int {
        var a = 0
        for (i in 1 until n) if (hypot(s[i * 4] - s[a * 4], s[i * 4 + 1] - s[a * 4 + 1]) > still) a = i
        return a
    }

    /** The samples from where the pen set off to where it came to rest (trembling on the spot trimmed). */
    private fun steady(s: FloatArray, n: Int): IntRange {
        var x0 = Float.MAX_VALUE; var y0 = Float.MAX_VALUE; var x1 = -Float.MAX_VALUE; var y1 = -Float.MAX_VALUE
        for (i in 0 until n) { x0 = minOf(x0, s[i * 4]); x1 = maxOf(x1, s[i * 4]); y0 = minOf(y0, s[i * 4 + 1]); y1 = maxOf(y1, s[i * 4 + 1]) }
        val r = maxOf(1.5f, hypot(x1 - x0, y1 - y0) * 0.03f)
        fun d(i: Int, j: Int) = hypot(s[i * 4] - s[j * 4], s[i * 4 + 1] - s[j * 4 + 1])
        fun path(i: Int, j: Int) = (i until j).sumOf { d(it, it + 1).toDouble() }.toFloat()
        // Only where the pen wandered back and forth on the spot (its path much longer than the
        // way it got): a pen that sets off cleanly keeps its first and last points.
        var e = n - 1
        while (e > 0 && d(e - 1, n - 1) < r) e--
        if (path(e, n - 1) < 2f * d(e, n - 1) + 1f) e = n - 1
        var b = 0
        while (b < e && d(b + 1, 0) < r) b++
        if (path(0, b) < 2f * d(0, b) + 1f) b = 0
        return b..e
    }

    /** Where a closed stroke that ran past its start should end: where it comes back closest to the start. */
    private fun overshootCut(xs: FloatArray, ys: FloatArray): Int {
        val n = xs.size
        var end = n - 1
        var best = hypot(xs[end] - xs[0], ys[end] - ys[0])
        for (j in (n * 0.75f).toInt() until n) { val d = hypot(xs[j] - xs[0], ys[j] - ys[0]); if (d < best) { best = d; end = j } }
        return end
    }

    /**
     * Corners of a closed stroke drawn as straight sides, also with rounded corners, a hook, the
     * end short of the start or past it. Read round as a loop: corners where it turns sharply over
     * a window; the gap where it closes is no side, a corner inside it comes from the directions
     * at the start and the end. Corners that barely turn are bends; a straight line is fitted
     * through each side (nearly level or upright: exactly so) and the corners are where
     * neighbouring sides meet. Null if the sides aren't straight.
     */
    private fun sideCorners(xs: FloatArray, ys: FloatArray): List<FloatArray>? {
        val k0 = xs.size
        val seg = FloatArray(k0) { i -> val j = (i + 1) % k0; hypot(xs[j] - xs[i], ys[j] - ys[i]) }
        val per = seg.sum().coerceAtLeast(1e-3f)
        val drawn = per - seg[k0 - 1] // without the closing gap
        val m = 120
        val rx = FloatArray(m); val ry = FloatArray(m)
        run {
            var i = 0; var acc = 0f
            for (q in 0 until m) {
                val d = per * q / m
                while (i < k0 - 1 && acc + seg[i] < d) { acc += seg[i]; i++ }
                val f = if (seg[i] > 0f) ((d - acc) / seg[i]).coerceIn(0f, 1f) else 0f
                val j = (i + 1) % k0
                rx[q] = xs[i] + (xs[j] - xs[i]) * f; ry[q] = ys[i] + (ys[j] - ys[i]) * f
            }
        }
        // Samples on the closing gap (only when there is a real gap).
        val gapFrom = if (per - drawn > per * 0.03f) kotlin.math.ceil(drawn / per * m).toInt() else m
        fun inGap(q: Int) = q >= gapFrom
        val w = 5
        fun at(q: Int) = ((q % m) + m) % m
        val hasGap = gapFrom < m
        val last = gapFrom - 1 // the last sample drawn
        // How sharply it turns at each sample, over a window either side. With a gap, the window
        // stays on what was drawn (the gap's straight line would cut a corner there in two).
        val turn = FloatArray(m) { q ->
            if (!hasGap) abs(angleBetween(rx[q] - rx[at(q - w)], ry[q] - ry[at(q - w)], rx[at(q + w)] - rx[q], ry[at(q + w)] - ry[q]))
            else if (q > last) 0f
            else {
                val a = maxOf(0, q - w); val b = minOf(last, q + w)
                if (q - a < 2 || b - q < 2) 0f else abs(angleBetween(rx[q] - rx[a], ry[q] - ry[a], rx[b] - rx[q], ry[b] - ry[q]))
            }
        }
        // Runs of sharp turning (outside the gap), one corner each.
        val sharp = BooleanArray(m) { !inGap(it) && turn[it] > 40f }
        val cand = ArrayList<Int>()
        if (sharp.any { !it }) {
            val start = (0 until m).first { !sharp[it] }
            var i = 0
            while (i < m) {
                val q = at(start + i)
                if (sharp[q]) {
                    var best = q
                    while (i < m && sharp[at(start + i)]) { val r = at(start + i); if (turn[r] > turn[best]) best = r; i++ }
                    cand += best
                } else i++
            }
        } else { trace?.invoke("all sharp"); return null }
        cand.sort()
        trace?.invoke("sharp ${cand}")
        val turns = HashMap<Int, Float>()
        for (c in cand) turns[c] = turn[c]
        // The gap: one corner in it if the way the path ends and the way it set off differ (each
        // read up to a corner drawn right there, if any).
        if (hasGap) {
            val first = cand.firstOrNull()?.takeIf { it < w } ?: w
            val lastC = cand.lastOrNull()?.takeIf { it > last - w } ?: (last - w)
            if (first >= 2 && last - lastC >= 2) {
                val across = abs(angleBetween(rx[last] - rx[lastC], ry[last] - ry[lastC], rx[first] - rx[0], ry[first] - ry[0]))
                if (across > 40f) { val g = (gapFrom + m) / 2; cand += g; turns[g] = across }
            }
            cand.sort()
        }
        // Corners closer than 8 % of the way are one rounded corner.
        var merged = true
        while (merged && cand.size > 3) {
            merged = false
            for (c in cand.indices) {
                val a = cand[c]; val b = cand[(c + 1) % cand.size]
                if (at(b - a) < m * 0.08f) { if (turns[a]!! >= turns[b]!!) cand.removeAt((c + 1) % cand.size) else cand.removeAt(c); merged = true; break }
            }
        }
        trace?.invoke("merged ${cand.map { "$it/${turns[it]!!.toInt()}" }} gapFrom=$gapFrom")
        // Corners that barely turn are bends in a side.
        while (cand.size > 3) {
            val weakest = cand.minBy { turns[it]!! }
            if (turns[weakest]!! >= 52f) break
            cand.remove(weakest)
        }
        if (cand.size < 2) { trace?.invoke("corners ${cand.size}"); return null }
        /** Samples of side [c] without [share] of its length at each end (where a corner rounds off). */
        fun sidePoints(c: Int, share: Float): List<Int> {
            val a = cand[c]; val b = cand[(c + 1) % cand.size]
            val span = at(b - a)
            val inset = minOf(maxOf(2, (span * share).toInt()), span / 2 - 1).coerceAtLeast(0)
            return (inset..span - inset).map { at(a + it) }.filter { !inGap(it) }
        }
        val diag = hypot(rx.max() - rx.min(), ry.max() - ry.min())
        // The line of a side comes from its middle: the ends bend into rounded corners.
        fun fitSides(): List<FloatArray>? = cand.indices.map { c ->
            // A side left out (the pen stopped short of it) runs along the gap.
            val pts = sidePoints(c, 0.2f).takeIf { it.size >= 2 } ?: (0..at(cand[(c + 1) % cand.size] - cand[c])).map { at(cand[c] + it) }
            if (pts.size < 2) return null
            fitLine(pts.map { rx[it] }, pts.map { ry[it] })
        }
        /** Points of side [c] off its line (a few, from a hook or wobble, are fine; its ends may round off). */
        fun crooked(c: Int, l: FloatArray): Boolean {
            val pts = sidePoints(c, 0.15f)
            if (pts.isEmpty()) return false
            return pts.count { abs((rx[it] - l[0]) * l[3] - (ry[it] - l[1]) * l[2]) > diag * 0.06f } > pts.size * 0.12f
        }
        /** How much of the way round side [c] takes (0..1). */
        fun share(c: Int) = at(cand[(c + 1) % cand.size] - cand[c]) / m.toFloat()
        // A side that isn't straight hides a corner drawn too round to notice: split it where it
        // lies farthest from the chord between its ends, and look again.
        trace?.invoke("strong ${cand.map { "$it/${turns[it]!!.toInt()}" }}")
        var lines = fitSides() ?: run { trace?.invoke("side too short"); return null }
        for (round in 0 until 4) {
            val bent = cand.indices.firstOrNull { crooked(it, lines[it]) } ?: break
            if (cand.size >= 6) break
            val a = cand[bent]; val b = cand[(bent + 1) % cand.size]
            val pts = sidePoints(bent, 0.15f)
            if (pts.isEmpty()) break
            val q = pts.maxBy { NotebookView.segDist(rx[it], ry[it], rx[a], ry[a], rx[b], ry[b]) }
            cand += q; turns[q] = 90f
            cand.sort()
            lines = fitSides() ?: run { trace?.invoke("side too short"); return null }
        }
        trace?.invoke("split ${cand.map { "$it/${turns[it]!!.toInt()}" }}")
        // Two neighbouring sides running nearly the same way are one side (a bend, not a corner).
        while (cand.size > 3) {
            val c = cand.indices.firstOrNull { k ->
                val l1 = lines[(k - 1 + cand.size) % cand.size]; val l2 = lines[k]
                abs(l1[2] * l2[3] - l1[3] * l2[2]) < 0.45f && (l1[2] * l2[2] + l1[3] * l2[3]) > 0f // under ~27°, same direction
            } ?: break
            cand.removeAt(c)
            lines = fitSides() ?: run { trace?.invoke("side too short"); return null }
        }
        trace?.invoke("collinear ${cand.map { "$it/${turns[it]!!.toInt()}" }}")
        // A short side between two corners is a corner drawn round (cut off), not a side of its own:
        // it goes, with the weaker of its corners.
        while (cand.size > 4) {
            val k = cand.indices.minBy { share(it) }
            if (share(k) >= 0.11f) break
            val a = cand[k]; val b = cand[(k + 1) % cand.size]
            cand.remove(if (turns[a]!! >= turns[b]!!) b else a)
            lines = fitSides() ?: run { trace?.invoke("side too short"); return null }
        }
        if (cand.size !in 3..6) { trace?.invoke("corners ${cand.size}: ${cand.map { "$it/${turns[it]!!.toInt()}" }} gapFrom=$gapFrom"); return null }
        for (c in cand.indices) if (crooked(c, lines[c])) { trace?.invoke("side $c not straight, corners=${cand.map { "$it/${turns[it]!!.toInt()}" }}"); return null }
        val corners = cand.indices.map { c ->
            intersect(lines[(c - 1 + cand.size) % cand.size], lines[c]) ?: floatArrayOf(rx[cand[c]], ry[cand[c]])
        }
        // Corners far outside what was drawn: two sides nearly parallel, no real shape.
        val minX = rx.min() - diag * 0.25f; val maxX = rx.max() + diag * 0.25f; val minY = ry.min() - diag * 0.25f; val maxY = ry.max() + diag * 0.25f
        if (corners.any { it[0] < minX || it[0] > maxX || it[1] < minY || it[1] > maxY }) { trace?.invoke("corner outside"); return null }
        return corners
    }

    /** [n] points evenly along the path as drawn (first and last point kept). */
    private fun resampleOpen(xs: FloatArray, ys: FloatArray, n: Int): Pair<FloatArray, FloatArray> {
        val k = xs.size
        val seg = FloatArray(k - 1) { i -> hypot(xs[i + 1] - xs[i], ys[i + 1] - ys[i]) }
        val per = seg.sum().coerceAtLeast(1e-3f)
        val ox = FloatArray(n); val oy = FloatArray(n)
        var i = 0; var acc = 0f
        for (q in 0 until n) {
            val d = per * q / (n - 1)
            while (i < k - 2 && acc + seg[i] < d) { acc += seg[i]; i++ }
            val f = if (seg[i] > 0f) ((d - acc) / seg[i]).coerceIn(0f, 1f) else 0f
            ox[q] = xs[i] + (xs[i + 1] - xs[i]) * f; oy[q] = ys[i] + (ys[i + 1] - ys[i]) * f
        }
        return ox to oy
    }

    private fun angleBetween(ax: Float, ay: Float, bx: Float, by: Float): Float {
        var d = Math.toDegrees((atan2(by, bx) - atan2(ay, ax)).toDouble()).toFloat()
        while (d > 180f) d -= 360f
        while (d < -180f) d += 360f
        return d
    }

    /** A line through points: a point on it and its direction (unit); nearly level/upright: exactly so. */
    private fun fitLine(px: List<Float>, py: List<Float>): FloatArray {
        val mx = px.average().toFloat(); val my = py.average().toFloat()
        var sxx = 0f; var syy = 0f; var sxy = 0f
        for (i in px.indices) { val dx = px[i] - mx; val dy = py[i] - my; sxx += dx * dx; syy += dy * dy; sxy += dx * dy }
        var ang = 0.5f * atan2(2 * sxy, sxx - syy)
        val deg = Math.toDegrees(ang.toDouble())
        val axis = Math.round(deg / 90.0) * 90.0
        if (abs(deg - axis) < 6.0) ang = Math.toRadians(axis).toFloat()
        return floatArrayOf(mx, my, cos(ang), sin(ang))
    }

    /** Where two lines (point, direction) meet; null if nearly parallel. */
    private fun intersect(a: FloatArray, b: FloatArray): FloatArray? {
        val den = a[2] * b[3] - a[3] * b[2]
        if (abs(den) < 0.17f) return null // under ~10°
        val t = ((b[0] - a[0]) * b[3] - (b[1] - a[1]) * b[2]) / den
        return floatArrayOf(a[0] + a[2] * t, a[1] + a[3] * t)
    }

    private fun pathLength(xs: FloatArray, ys: FloatArray): Float {
        var l = 0f
        for (i in 1 until xs.size) l += hypot(xs[i] - xs[i - 1], ys[i] - ys[i - 1])
        return l
    }

    private fun angle(a: FloatArray, b: FloatArray, c: FloatArray): Float {
        val a1 = atan2(a[1] - b[1], a[0] - b[0]); val a2 = atan2(c[1] - b[1], c[0] - b[0])
        var d = Math.toDegrees((a1 - a2).toDouble()).toFloat()
        while (d > 180f) d -= 360f
        while (d < -180f) d += 360f
        return d
    }

    /** A four-cornered shape: a clean rectangle if the corners are close to right angles, else the quad as drawn. */
    private fun squareUp(c: List<FloatArray>): Pair<String, FloatArray> {
        val right = c.indices.all { i -> abs(abs(angle(c[(i + 3) % 4], c[i], c[(i + 1) % 4])) - 90f) < 22f }
        if (!right) return "quad" to c.flatMap { listOf(it[0], it[1]) }.toFloatArray()
        // Fit a rotated rectangle: centre, the way all four sides lie (a quarter turn is the
        // same way, so their angles are averaged four times over, weighted by length), half sides.
        val cx = c.sumOf { it[0].toDouble() }.toFloat() / 4; val cy = c.sumOf { it[1].toDouble() }.toFloat() / 4
        var sx = 0.0; var sy = 0.0
        for (i in 0 until 4) {
            val dx = (c[(i + 1) % 4][0] - c[i][0]).toDouble(); val dy = (c[(i + 1) % 4][1] - c[i][1]).toDouble()
            val a4 = 4 * atan2(dy, dx); val l = hypot(dx, dy)
            sx += l * cos(a4); sy += l * sin(a4)
        }
        var ang = (atan2(sy, sx) / 4).toFloat()
        // Drawn about level (within 10°): level.
        if (abs(Math.toDegrees(ang.toDouble())) < 10.0) ang = 0f
        val ux = cos(ang); val uy = sin(ang)
        var hw = 0f; var hh = 0f
        for (p in c) {
            val dx = p[0] - cx; val dy = p[1] - cy
            hw += abs(dx * ux + dy * uy); hh += abs(-dx * uy + dy * ux)
        }
        hw /= 4; hh /= 4
        // Sides within 7 % of each other: a square (as a nearly round ellipse is a circle).
        if (abs(hw - hh) < 0.07f * maxOf(hw, hh)) { val h = (hw + hh) / 2; hw = h; hh = h }
        val out = FloatArray(8)
        val signs = arrayOf(floatArrayOf(-1f, -1f), floatArrayOf(1f, -1f), floatArrayOf(1f, 1f), floatArrayOf(-1f, 1f))
        for (i in 0 until 4) {
            val a = signs[i][0] * hw; val b = signs[i][1] * hh
            out[2 * i] = cx + a * ux - b * uy
            out[2 * i + 1] = cy + a * uy + b * ux
        }
        return "rectangle" to out
    }

    /** A straight line from (x0, y0) to (x1, y1) as samples (dragging a snapped line's end). */
    fun line(x0: Float, y0: Float, x1: Float, y1: Float, p: Float, t0: Float, t1: Float): FloatArray = poly(floatArrayOf(x0, y0, x1, y1), p, t0, t1, close = false)

    /**
     * A closed shape that just snapped while the pen is still down: dragging the pen changes its
     * size. The corner of the shape's box nearest the pen ([penX], [penY]) follows the pen, the
     * opposite corner stays where it is, so pulling sideways changes only the width. A rectangle
     * or ellipse keeps its angle (its box lies along its own sides), and turns into a square or
     * circle while its sides nearly match ([square]); other shapes stretch in a level box.
     */
    class HeldShape(shape: Shape, penX: Float, penY: Float) {
        private val kind = shape.kind
        private val ellipse = shape.ellipse
        private val p = shape.samples[2]
        private val t0 = shape.samples[3]
        // The frame the box lies in: an origin and its x axis (unit).
        private val ox: Float; private val oy: Float; private val ux: Float; private val uy: Float
        /** The corners in the frame (x, y pairs; empty for an ellipse). */
        private val local: FloatArray
        /** The held corner and the one opposite it, in the frame. */
        private val hx: Float; private val hy: Float; private val ax: Float; private val ay: Float
        /** Whether the last drag made it a square or a circle. */
        var square = false; private set

        init {
            val c = shape.corners
            var x0: Float; var x1: Float; var y0: Float; var y1: Float
            if (ellipse != null) {
                ox = ellipse.cx; oy = ellipse.cy; ux = ellipse.ux; uy = ellipse.uy
                local = FloatArray(0)
                x0 = -ellipse.ra; x1 = ellipse.ra; y0 = -ellipse.rb; y1 = ellipse.rb
            } else {
                requireNotNull(c)
                val m = c.size / 2
                ox = (0 until m).sumOf { c[2 * it].toDouble() }.toFloat() / m
                oy = (0 until m).sumOf { c[2 * it + 1].toDouble() }.toFloat() / m
                if (kind == "rectangle") {
                    val l = hypot(c[2] - c[0], c[3] - c[1]).coerceAtLeast(1e-3f)
                    ux = (c[2] - c[0]) / l; uy = (c[3] - c[1]) / l
                } else { ux = 1f; uy = 0f }
                local = FloatArray(c.size)
                for (i in 0 until m) { val dx = c[2 * i] - ox; val dy = c[2 * i + 1] - oy; local[2 * i] = dx * ux + dy * uy; local[2 * i + 1] = -dx * uy + dy * ux }
                x0 = (0 until m).minOf { local[2 * it] }; x1 = (0 until m).maxOf { local[2 * it] }
                y0 = (0 until m).minOf { local[2 * it + 1] }; y1 = (0 until m).maxOf { local[2 * it + 1] }
            }
            val dx = penX - ox; val dy = penY - oy
            val px = dx * ux + dy * uy; val py = -dx * uy + dy * ux
            if (abs(px - x0) < abs(px - x1)) { hx = x0; ax = x1 } else { hx = x1; ax = x0 }
            if (abs(py - y0) < abs(py - y1)) { hy = y0; ay = y1 } else { hy = y1; ay = y0 }
        }

        /** The shape with the held corner moved by ([dx], [dy]) page points; [t1]: its time (ms). */
        fun drag(dx: Float, dy: Float, t1: Float): FloatArray {
            val ldx = dx * ux + dy * uy; val ldy = -dx * uy + dy * ux
            // Sizes from the corner that stays (signed: dragging past it mirrors the shape).
            fun keep(v: Float, was: Float) = if (abs(v) >= MIN_SIDE) v else if (v != 0f) Math.copySign(MIN_SIDE, v) else Math.copySign(MIN_SIDE, was)
            var w = keep(hx + ldx - ax, hx - ax)
            var h = keep(hy + ldy - ay, hy - ay)
            // Sides nearly alike: a square (a circle); the side being pulled more gives way.
            square = (kind == "rectangle" || kind == "ellipse") && abs(abs(w) - abs(h)) < SQUARE_SNAP * maxOf(abs(w), abs(h))
            if (square) { if (abs(ldx) >= abs(ldy)) w = Math.copySign(abs(h), w) else h = Math.copySign(abs(w), h) }
            fun page(a: Float, b: Float) = floatArrayOf(ox + a * ux - b * uy, oy + a * uy + b * ux)
            if (ellipse != null) {
                val c = page(ax + w / 2, ay + h / 2)
                return ellipseSamples(Fit(c[0], c[1], ux, uy, abs(w) / 2, abs(h) / 2, 0f), p, t0, t1)
            }
            val kx = w / (hx - ax); val ky = h / (hy - ay)
            val out = FloatArray(local.size)
            for (i in 0 until local.size / 2) {
                val q = page(ax + (local[2 * i] - ax) * kx, ay + (local[2 * i + 1] - ay) * ky)
                out[2 * i] = q[0]; out[2 * i + 1] = q[1]
            }
            return poly(out, p, t0, t1, close = true)
        }
    }

    /** The smallest side a held shape can be pulled to (page points). */
    private const val MIN_SIDE = 3f
    /** A held rectangle or ellipse is a square or circle while its sides differ by less than this share. */
    private const val SQUARE_SNAP = 0.04f

    /** Centre, axis angle, radii and the mean radial error of the points (0 = on the ellipse). */
    class Fit(val cx: Float, val cy: Float, val ux: Float, val uy: Float, val ra: Float, val rb: Float, val err: Float)

    /** An ellipse through the points (principal axes; radii from the mean extent, robust to wobble). */
    private fun fitEllipse(xs: FloatArray, ys: FloatArray): Fit? {
        val cx = xs.average().toFloat(); val cy = ys.average().toFloat()
        var sxx = 0f; var syy = 0f; var sxy = 0f
        for (i in xs.indices) { val dx = xs[i] - cx; val dy = ys[i] - cy; sxx += dx * dx; syy += dy * dy; sxy += dx * dy }
        val ang = 0.5f * atan2(2 * sxy, sxx - syy)
        val ux = cos(ang); val uy = sin(ang)
        // For points evenly around an ellipse, mean(a²) = ra²/2.
        var saa = 0f; var sbb = 0f
        for (i in xs.indices) {
            val dx = xs[i] - cx; val dy = ys[i] - cy
            val a = dx * ux + dy * uy; val b = -dx * uy + dy * ux
            saa += a * a; sbb += b * b
        }
        var ra = sqrt(2 * saa / xs.size); var rb = sqrt(2 * sbb / xs.size)
        if (ra < 8f || rb < 8f) return null
        var err = 0f
        for (i in xs.indices) {
            val dx = xs[i] - cx; val dy = ys[i] - cy
            val a = (dx * ux + dy * uy) / ra; val b = (-dx * uy + dy * ux) / rb
            err += abs(sqrt(a * a + b * b) - 1f)
        }
        var fx = ux; var fy = uy
        if (abs(ra - rb) / maxOf(ra, rb) < 0.12f) { val r = (ra + rb) / 2; ra = r; rb = r; fx = 1f; fy = 0f } // a circle (level, for stretching)
        else {
            // Drawn about level or upright (within 10°): exactly so, like a rectangle.
            val deg = Math.toDegrees(ang.toDouble()); val axis = Math.round(deg / 90.0) * 90.0
            if (abs(deg - axis) < 10.0) { fx = cos(Math.toRadians(axis)).toFloat(); fy = sin(Math.toRadians(axis)).toFloat() }
        }
        return Fit(cx, cy, fx, fy, ra, rb, err / xs.size)
    }

    private fun ellipseSamples(f: Fit, p: Float, t0: Float, t1: Float): FloatArray {
        val steps = 72
        val out = FloatArray((steps + 1) * 4)
        for (k in 0..steps) {
            val th = 2 * PI.toFloat() * k / steps
            val a = f.ra * cos(th); val b = f.rb * sin(th)
            out[k * 4] = f.cx + a * f.ux - b * f.uy
            out[k * 4 + 1] = f.cy + a * f.uy + b * f.ux
            out[k * 4 + 2] = p
            out[k * 4 + 3] = t0 + (t1 - t0) * k / steps
        }
        return out
    }

    /**
     * Polyline through the corners (closed: back to the first). Samples lie close together next
     * to each corner: the renderer smooths a stroke (each point is pulled halfway toward the
     * next), which rounds a corner off by about the spacing there, so 6 pt apart would cut every
     * corner but the first visibly.
     */
    private fun poly(c: FloatArray, p: Float, t0: Float, t1: Float, close: Boolean): FloatArray {
        val pts = ArrayList<Float>()
        val m = c.size / 2
        val segs = if (close) m else m - 1
        for (i in 0 until segs) {
            val ax = c[2 * i]; val ay = c[2 * i + 1]
            val bx = c[2 * ((i + 1) % m)]; val by = c[2 * ((i + 1) % m) + 1]
            val len = hypot(bx - ax, by - ay)
            if (len < 1e-3f) continue
            for (d in sideStops(len)) { pts += ax + (bx - ax) * d / len; pts += ay + (by - ay) * d / len }
        }
        if (close) { pts += c[0]; pts += c[1] } else { pts += c[2 * (m - 1)]; pts += c[2 * (m - 1) + 1] }
        val n = pts.size / 2
        return FloatArray(n * 4).also { out ->
            for (i in 0 until n) {
                out[i * 4] = pts[2 * i]; out[i * 4 + 1] = pts[2 * i + 1]; out[i * 4 + 2] = p
                out[i * 4 + 3] = t0 + (t1 - t0) * i / maxOf(1, n - 1)
            }
        }
    }

    /** Where samples go along a side [len] long, from 0 (the corner) up to but not including the far corner. */
    private fun sideStops(len: Float): List<Float> {
        val near = floatArrayOf(0f, 0.25f, 0.5f, 1f, 2f, 4f)
        val out = ArrayList<Float>()
        for (d in near) if (d < len / 2) out += d
        var d = 9f
        while (d < len - 9f) { out += d; d += 6f }
        for (k in near.indices.reversed()) if (near[k] > 0f && len - near[k] > len / 2) out += len - near[k]
        return out
    }
}
