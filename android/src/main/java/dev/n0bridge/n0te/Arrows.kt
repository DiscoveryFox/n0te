package dev.n0bridge.n0te

import kotlin.math.abs
import kotlin.math.atan2
import kotlin.math.cos
import kotlin.math.hypot
import kotlin.math.sin

/**
 * Arrows, tidied but still hand-drawn. The head comes out symmetric about the line it sits on,
 * each barb keeping a third of how it differed from the other, the barbs' ends tapering off;
 * a straight shaft comes out straight but keeps a third of the hand's sway and the pen's
 * pressure; a curved shaft stays exactly as drawn. Ways to draw one:
 *  - a line (straight or curved) first, then a "V" — or a small closed triangle — at one of
 *    its ends: that becomes the head as the pen lifts ([head]);
 *  - in one stroke, shaft then head (one barb, both, back and forth): tidied as the pen lifts
 *    when both barbs are there, or held at the end like any shape ([fromStroke]).
 * Samples are x, y, pressure, t quads, as everywhere.
 */
object Arrows {
    /** How far a head drawn on its own sits past the line's end (page points), covering its round end. */
    private const val POINT_PAST = 0.35f

    /** Tests: why a stroke wasn't taken as an arrow. */
    internal var trace: ((String) -> Unit)? = null

    /** A drawn barb: from the tip, as a vector, and how hard the pen pressed on it. */
    private class Barb(val dx: Float, val dy: Float, val p: Float)

    /** A piece of the result: points (x, y, pressure) to follow, ends that taper off. */
    private class Leg(val pts: List<FloatArray>, val taperStart: Boolean, val taperEnd: Boolean) {
        constructor(x0: Float, y0: Float, x1: Float, y1: Float, p: Float, taperStart: Boolean, taperEnd: Boolean) :
            this(listOf(floatArrayOf(x0, y0, p), floatArrayOf(x1, y1, p)), taperStart, taperEnd)
    }

    // --- a head drawn on its own --------------------------------------------------------------

    /**
     * The stroke [s] (samples [from]..[to]) as the head of an arrow at a line's end: its tip at
     * ([tipX], [tipY]), the line running from there in the direction ([backX], [backY]) (unit)
     * and [shaft] long. Whatever order it was drawn in — a V with its point in the middle, from
     * the point out and back and out again, with hooks, or a small closed triangle — it is a
     * head if it has its point at that end and one straight barb on each side of the line
     * pointing back along it. A round bracket (curved arms) or a small circle (not a triangle)
     * at a line's end is no head: number lines keep their ends. Null if it is no head.
     */
    fun head(s: FloatArray, from: Int, to: Int, tipX: Float, tipY: Float, backX: Float, backY: Float, shaft: Float): FloatArray? {
        val n = to - from + 1
        if (n < 4) return null
        val xs = FloatArray(n) { s[(from + it) * 4] }
        val ys = FloatArray(n) { s[(from + it) * 4 + 1] }
        val ps = FloatArray(n) { s[(from + it) * 4 + 2] }
        val total = pathLen(xs, ys, 0, n - 1)
        if (total < 8f) return null
        val size = maxOf(xs.max() - xs.min(), ys.max() - ys.min())
        if (size > maxOf(12f, shaft * 0.7f)) { trace?.invoke("head too big $size vs $shaft"); return null }
        val t0 = s[from * 4 + 3]; val t1 = s[to * 4 + 3]
        // Its point: its foremost part, the way the line points. Near the line's end and on
        // the line's way on (a "v" written beside the end is no head).
        val ti = (0 until n).minBy { (xs[it] - tipX) * backX + (ys[it] - tipY) * backY }
        val dEnd = hypot(xs[ti] - tipX, ys[ti] - tipY)
        val aside = abs(backX * (ys[ti] - tipY) - backY * (xs[ti] - tipX))
        if (dEnd > maxOf(8f, size * 0.6f) || aside > maxOf(4f, size * 0.35f)) { trace?.invoke("point $dEnd from the end, $aside aside"); return null }
        // Its barbs: on each side of the line the farthest part pointing back along it (within
        // 85°: an arm may curl out at its end), seen from its own point (which may overshoot
        // the line's end a little). A part reaching forward is something else (a "<" after the line).
        val px = xs[ti]; val py = ys[ti]
        var left = -1; var right = -1; var lBest = 0f; var rBest = 0f
        for (i in 0 until n) {
            val vx = xs[i] - px; val vy = ys[i] - py
            val l = hypot(vx, vy)
            if (l < size * 0.3f) continue
            val ang = Math.toDegrees(kotlin.math.acos(((vx * backX + vy * backY) / l).coerceIn(-1f, 1f).toDouble()))
            if (ang > 85.0) { if (ang > 100.0 && l > size * 0.35f) { trace?.invoke("reaches out ${ang.toInt()}°"); return null }; continue }
            val side = backX * vy - backY * vx
            if (side >= 0f && l > lBest) { lBest = l; left = i } else if (side < 0f && l > rBest) { rBest = l; right = i }
        }
        if (left < 0 || right < 0) { trace?.invoke("barbs l=${left >= 0} r=${right >= 0}"); return null }
        val lx = xs[left] - px; val ly = ys[left] - py; val rx = xs[right] - px; val ry = ys[right] - py
        val ll = hypot(lx, ly); val rl = hypot(rx, ry)
        if (minOf(ll, rl) < 5f || maxOf(ll, rl) > 3f * minOf(ll, rl)) { trace?.invoke("barbs $ll $rl"); return null }
        val opening = Math.toDegrees(kotlin.math.acos(((lx * rx + ly * ry) / (ll * rl)).coerceIn(-1f, 1f).toDouble()))
        if (opening < 15.0 || opening > 125.0) { trace?.invoke("opening $opening"); return null }
        // Straight barbs (a hook at the far end is fine): the way from the point out to each,
        // up to three quarters of it, stays near the straight line.
        val nearTip = (0 until n).filter { hypot(xs[it] - xs[ti], ys[it] - ys[ti]) < maxOf(3f, size * 0.2f) }
        fun straight(b: Int): Boolean {
            val from0 = nearTip.minBy { abs(it - b) }
            val bx = xs[b]; val by = ys[b]
            val bl = hypot(bx - xs[from0], by - ys[from0]).coerceAtLeast(1e-3f)
            val range = if (from0 <= b) from0..b else b..from0
            for (i in range) {
                val along = ((xs[i] - xs[from0]) * (bx - xs[from0]) + (ys[i] - ys[from0]) * (by - ys[from0])) / bl
                if (along > bl * 0.75f) continue
                if (NotebookView.segDist(xs[i], ys[i], xs[from0], ys[from0], bx, by) > maxOf(1.5f, bl * 0.12f)) return false
            }
            return true
        }
        val closed = hypot(xs[0] - xs[n - 1], ys[0] - ys[n - 1]) < total * 0.3f
        val tri = abs(lx * ry - ly * rx) / 2f
        val area = if (closed) abs((0 until n).sumOf { i -> val j = (i + 1) % n; (xs[i] * ys[j] - xs[j] * ys[i]).toDouble() }.toFloat()) / 2f else 0f
        val triangle = closed && area > tri * 0.3f
        if (triangle && area > tri * 1.6f) { trace?.invoke("round, not a triangle"); return null }
        if (!triangle && !(straight(left) && straight(right))) { trace?.invoke("curved barbs"); return null }
        // It must open back along the line (roughly); the clean head then lies exactly about the
        // line's way at its end, whichever way the drawn one leaned.
        val mx = lx / ll + rx / rl; val my = ly / ll + ry / rl
        val ml = hypot(mx, my)
        if (ml < 1e-3f) return null
        val off = Math.toDegrees(kotlin.math.acos(((mx * backX + my * backY) / ml).coerceIn(-1f, 1f).toDouble()))
        if (off > 50.0) { trace?.invoke("opens $off off the line"); return null }
        val hx = backX; val hy = backY
        val p = ps.average().toFloat()
        val barbs = listOf(Barb(lx, ly, p), Barb(rx, ry, p))
        // The point a third of a point past the line's end: it covers the line's round end.
        val legs = headLegs(tipX - backX * POINT_PAST, tipY - backY * POINT_PAST, hx, hy, barbs, shaft, joined = true)
        if (!triangle) return sample(legs, t0, t1)
        // Closed: from the last barb's end back to the first.
        val b1 = legs.first().pts.first(); val b2 = legs.last().pts.last()
        return sample(legs.map { Leg(it.pts, false, false) } + Leg(b2[0], b2[1], b1[0], b1[1], p, false, false), t0, t1)
    }

    /**
     * A small closed triangle drawn at a line's end, however it was turned: the head of an arrow
     * pointing exactly the way the line runs there ([backX], [backY] point back into the line),
     * as big as the drawn one (a third of the line, 30 pt at most). Closed (its end back near its
     * start) and three-cornered, not round: 4π·area/perimeter² between a thin triangle's and a
     * square's (a circle at a line's end stays a circle). Its middle lies within its size of the
     * end, not back along the line. Null if it is no such triangle.
     */
    fun closedHead(s: FloatArray, from: Int, to: Int, tipX: Float, tipY: Float, backX: Float, backY: Float, shaft: Float): FloatArray? {
        val n = to - from + 1
        if (n < 5) return null
        val xs = FloatArray(n) { s[(from + it) * 4] }
        val ys = FloatArray(n) { s[(from + it) * 4 + 1] }
        val total = pathLen(xs, ys, 0, n - 1)
        if (total < 10f) return null
        val size = maxOf(xs.max() - xs.min(), ys.max() - ys.min())
        if (size > maxOf(12f, shaft * 0.7f)) { trace?.invoke("triangle too big $size vs $shaft"); return null }
        if (hypot(xs[0] - xs[n - 1], ys[0] - ys[n - 1]) > total * 0.3f) { trace?.invoke("not closed"); return null }
        val area = abs((0 until n).sumOf { i -> val j = (i + 1) % n; (xs[i] * ys[j] - xs[j] * ys[i]).toDouble() }.toFloat()) / 2f
        val compact = (4.0 * Math.PI * area / (total * total)).toFloat()
        if (compact < 0.18f || compact > 0.74f) { trace?.invoke("not a triangle (${"%.2f".format(compact)})"); return null }
        val cx = xs.average().toFloat(); val cy = ys.average().toFloat()
        val along = (cx - tipX) * backX + (cy - tipY) * backY
        if (hypot(cx - tipX, cy - tipY) > maxOf(8f, size * 0.9f) || along > size * 0.8f) { trace?.invoke("triangle not at the end"); return null }
        val p = (0 until n).map { s[(from + it) * 4 + 2] }.average().toFloat()
        val l = size * 0.95f
        val a = Math.toRadians(24.0).toFloat()
        val barbs = listOf(a, -a).map { t -> Barb((backX * cos(t) - backY * sin(t)) * l, (backX * sin(t) + backY * cos(t)) * l, p) }
        val legs = headLegs(tipX - backX * POINT_PAST, tipY - backY * POINT_PAST, backX, backY, barbs, shaft, joined = true)
        val b1 = legs.first().pts.first(); val b2 = legs.last().pts.last()
        return sample(legs.map { Leg(it.pts, false, false) } + Leg(b2[0], b2[1], b1[0], b1[1], p, false, false), s[from * 4 + 3], s[to * 4 + 3])
    }

    /**
     * A single barb drawn on its own at a line's end (the head drawn as two strokes): a short,
     * straight stroke with one end at the tip, pointing back along the line (8–70° off it).
     * Its vector from the tip and pressure (dx, dy, p), or null.
     */
    fun barb(s: FloatArray, from: Int, to: Int, tipX: Float, tipY: Float, backX: Float, backY: Float, shaft: Float): FloatArray? {
        val n = to - from + 1
        if (n < 3) return null
        val x0 = s[from * 4]; val y0 = s[from * 4 + 1]; val x1 = s[to * 4]; val y1 = s[to * 4 + 1]
        var len = 0f
        for (i in from until to) len += hypot(s[(i + 1) * 4] - s[i * 4], s[(i + 1) * 4 + 1] - s[i * 4 + 1])
        val p = (from..to).map { s[it * 4 + 2] }.average().toFloat()
        return barbFrom(x0, y0, x1, y1, len, tipX, tipY, backX, backY, shaft)?.let { floatArrayOf(it[0], it[1], p) }
    }

    /** A straight stroke from (x0, y0) to (x1, y1), [len] long, as a barb at the tip: its vector from the tip, or null. */
    fun barbFrom(x0: Float, y0: Float, x1: Float, y1: Float, len: Float, tipX: Float, tipY: Float, backX: Float, backY: Float, shaft: Float): FloatArray? {
        val chord = hypot(x1 - x0, y1 - y0)
        if (chord < 5f || chord < len * 0.85f || chord > shaft * 0.6f) return null
        val d0 = hypot(x0 - tipX, y0 - tipY); val d1 = hypot(x1 - tipX, y1 - tipY)
        val (fx, fy) = if (d0 <= d1) x1 to y1 else x0 to y0
        if (minOf(d0, d1) > maxOf(8f, chord * 0.5f)) return null
        val vx = fx - tipX; val vy = fy - tipY
        val vl = hypot(vx, vy)
        if (vl < 3f) return null
        val ang = Math.toDegrees(kotlin.math.acos(((vx * backX + vy * backY) / vl).coerceIn(-1f, 1f).toDouble()))
        if (ang < 8.0 || ang > 70.0) return null
        return floatArrayOf(vx, vy)
    }

    /**
     * A head from two barbs drawn one by one ((dx, dy, p) from the tip each, on either side of
     * the line): one V, tidied like any head. Null if both lie on the same side.
     */
    fun headFromBarbs(b1: FloatArray, b2: FloatArray, tipX: Float, tipY: Float, backX: Float, backY: Float, shaft: Float, t0: Float, t1: Float): FloatArray? {
        val s1 = backX * b1[1] - backY * b1[0]; val s2 = backX * b2[1] - backY * b2[0]
        if (s1 * s2 >= 0f) return null
        val barbs = listOf(Barb(b1[0], b1[1], b1[2]), Barb(b2[0], b2[1], b2[2]))
        return sample(headLegs(tipX - backX * POINT_PAST, tipY - backY * POINT_PAST, backX, backY, barbs, shaft, joined = true), t0, t1)
    }

    // --- an arrow in one stroke ---------------------------------------------------------------

    /**
     * One stroke that is an arrow (a head at one end or both), or null. [s] the samples
     * [from]..[to] (inclusive), trembling trimmed. [strict] (no hold, as the pen lifts): only
     * with both barbs drawn and a shaft of 30 pt or more.
     */
    fun fromStroke(s: FloatArray, from: Int, to: Int, strict: Boolean = false): FloatArray? {
        val n = to - from + 1
        if (n < 6) return null
        val xs = FloatArray(n) { s[(from + it) * 4] }
        val ys = FloatArray(n) { s[(from + it) * 4 + 1] }
        fun pAt(i: Int) = s[(from + i) * 4 + 2]
        val cum = FloatArray(n).also { for (i in 1 until n) it[i] = it[i - 1] + hypot(xs[i] - xs[i - 1], ys[i] - ys[i - 1]) }
        val total = cum[n - 1]
        if (total < 30f) return null
        /** How sharply the path turns at sample [i], over 6 pt (at least 2 samples) either side (degrees). */
        fun turnAt(i: Int): Double {
            var j = i; while (j > 0 && (cum[i] - cum[j] < 6f || i - j < 2)) j--
            var k = i; while (k < n - 1 && (cum[k] - cum[i] < 6f || k - i < 2)) k++
            if (j == i || k == i) return 0.0
            val ax = xs[i] - xs[j]; val ay = ys[i] - ys[j]; val bx = xs[k] - xs[i]; val by = ys[k] - ys[i]
            return abs(Math.toDegrees(atan2((ax * by - ay * bx).toDouble(), (ax * bx + ay * by).toDouble())))
        }
        /** The sharpest turn in the run of samples around [i] that turn more than 110°. */
        fun peak(i: Int): Int {
            var best = i; var k = i
            while (k < n && turnAt(k) > 110.0) { if (turnAt(k) > turnAt(best)) best = k; k++ }
            return best
        }
        // The tips: where the path doubles back into a head that takes at most 45 % of it.
        val endTip = (0 until n).firstOrNull { cum[it] >= total * 0.55f && total - cum[it] >= 3f && turnAt(it) > 110.0 }?.let { peak(it) }
        val startTip = (n - 1 downTo 0).firstOrNull { cum[it] <= total * 0.45f && cum[it] >= 3f && turnAt(it) > 110.0 && (endTip == null || it < endTip) }
            ?.let { i -> var j = i; while (j > 0 && turnAt(j - 1) > 110.0) j--; peak(j) }
        val a = startTip ?: 0
        val b = endTip ?: (n - 1)
        val shaftLen = cum[b] - cum[a]
        trace?.invoke("endTip=$endTip startTip=$startTip shaft=$shaftLen total=$total")
        if (shaftLen < (if (strict) 30f else 24f) || shaftLen < total * 0.38f) { trace?.invoke("shaft short"); return null }
        // The shaft: straight or a smooth curve (no sharp turns: not writing).
        val inner = simplify(xs.copyOfRange(a, b + 1), ys.copyOfRange(a, b + 1), maxOf(2.5f, shaftLen * 0.04f)).map { it + a }
        var turning = 0.0
        for (k in 1 until inner.size - 1) {
            val i0 = inner[k - 1]; val i1 = inner[k]; val i2 = inner[k + 1]
            val ax = xs[i1] - xs[i0]; val ay = ys[i1] - ys[i0]; val bx = xs[i2] - xs[i1]; val by = ys[i2] - ys[i1]
            val t = abs(Math.toDegrees(atan2((ax * by - ay * bx).toDouble(), (ax * bx + ay * by).toDouble())))
            if (t > 50.0) { trace?.invoke("sharp turn $t"); return null }
            turning += t
        }
        if (turning > 270.0) return null
        val chord = hypot(xs[b] - xs[a], ys[b] - ys[a])
        val straight = (a..b).all { NotebookView.segDist(xs[it], ys[it], xs[a], ys[a], xs[b], ys[b]) <= maxOf(3f, chord * 0.05f) }
        // The shaft's ends (straight: on its fitted line) and its points.
        val tipA: FloatArray; val tipB: FloatArray
        val shaftPts: List<FloatArray>
        if (straight && chord >= 10f) {
            val line = fitLine(xs, ys, a, b)
            tipA = line.first; tipB = line.second
            shaftPts = shaftLeg(xs, ys, a, b, tipA[0], tipA[1], tipB[0], tipB[1]) { pAt(it) }
        } else {
            tipA = floatArrayOf(xs[a], ys[a]); tipB = floatArrayOf(xs[b], ys[b])
            // Curved: as drawn (the pressure smoothed a little).
            shaftPts = (a..b).map { i -> floatArrayOf(xs[i], ys[i], (maxOf(a, i - 2)..minOf(b, i + 2)).map { pAt(it) }.average().toFloat()) }
        }
        /** The way into the shaft from its end [i] (unit), read over its last stretch. */
        fun into(i: Int, tip: FloatArray, inward: Int): FloatArray {
            val reach = minOf(14f, shaftLen * 0.2f)
            var j = i
            while (j in a..b && abs(cum[j] - cum[i]) < reach) j += inward
            j = j.coerceIn(a, b)
            val dx = xs[j] - tip[0]; val dy = ys[j] - tip[1]
            val l = hypot(dx, dy).coerceAtLeast(1e-3f)
            return floatArrayOf(dx / l, dy / l)
        }
        // A head: the corners beyond the tip, each a barb if it points back along the shaft;
        // the farthest on each side counts. Null if something else was drawn there.
        fun headAt(tip: FloatArray, back: FloatArray, idx: List<Int>, tipIdx: Int): List<Barb>? {
            var left: Barb? = null; var right: Barb? = null; var leftL = 0f; var rightL = 0f
            var stray = 0f
            for (i in idx) {
                val dx = xs[i] - tip[0]; val dy = ys[i] - tip[1]
                val l = hypot(dx, dy)
                if (l < maxOf(3f, shaftLen * 0.04f)) continue
                val ang = Math.toDegrees(kotlin.math.acos(((dx * back[0] + dy * back[1]) / l).coerceIn(-1f, 1f).toDouble()))
                if (ang > 70.0 || l > shaftLen * 0.6f) { stray = maxOf(stray, l); continue }
                val side = back[0] * dy - back[1] * dx
                val p = (minOf(tipIdx, i)..maxOf(tipIdx, i)).map { pAt(it) }.average().toFloat()
                if (side >= 0f && l > leftL) { left = Barb(dx, dy, p); leftL = l } else if (side < 0f && l > rightL) { right = Barb(dx, dy, p); rightL = l }
            }
            if (stray > shaftLen * 0.12f) { trace?.invoke("stray $stray"); return null }
            trace?.invoke("barbs l=${left != null} r=${right != null}")
            return listOfNotNull(left, right)
        }
        // The way back into the shaft at each tip: along the fitted line when straight, else
        // along the curve's last stretch.
        val (backA, backB) = if (straight && chord >= 10f) {
            val l = hypot(tipB[0] - tipA[0], tipB[1] - tipA[1]).coerceAtLeast(1e-3f)
            val ux = (tipB[0] - tipA[0]) / l; val uy = (tipB[1] - tipA[1]) / l
            floatArrayOf(ux, uy) to floatArrayOf(-ux, -uy)
        } else into(a, tipA, 1) to into(b, tipB, -1)
        val endHead = if (endTip == null) emptyList() else headAt(tipB, backB, (b + 1 until n).toList(), b) ?: return null
        val startHead = if (startTip == null) emptyList() else headAt(tipA, backA, (0 until a).toList(), a) ?: return null
        if (endHead.isEmpty() && startHead.isEmpty()) return null
        if (strict && listOf(endHead, startHead).any { it.size == 1 }) return null
        val legs = ArrayList<Leg>()
        if (startHead.isNotEmpty()) legs += headLegs(tipA[0], tipA[1], backA[0], backA[1], startHead, shaftLen)
        legs += Leg(shaftPts, taperStart = startHead.isEmpty(), taperEnd = endHead.isEmpty())
        if (endHead.isNotEmpty()) legs += headLegs(tipB[0], tipB[1], backB[0], backB[1], endHead, shaftLen)
        return sample(legs, s[from * 4 + 3], s[to * 4 + 3])
    }

    /** A line through the points [a]..[b]: its ends where the drawn ends lie on it, exactly level, upright or diagonal within 3°. */
    private fun fitLine(xs: FloatArray, ys: FloatArray, a: Int, b: Int): Pair<FloatArray, FloatArray> {
        val mx = (a..b).map { xs[it] }.average().toFloat(); val my = (a..b).map { ys[it] }.average().toFloat()
        var sxx = 0f; var syy = 0f; var sxy = 0f
        for (i in a..b) { val dx = xs[i] - mx; val dy = ys[i] - my; sxx += dx * dx; syy += dy * dy; sxy += dx * dy }
        var ang = 0.5 * atan2(2.0 * sxy, (sxx - syy).toDouble())
        val near = Math.round(ang / (Math.PI / 4)) * (Math.PI / 4)
        if (abs(ang - near) < Math.toRadians(3.0)) ang = near
        val ux = cos(ang).toFloat(); val uy = sin(ang).toFloat()
        fun on(i: Int): FloatArray { val t = (xs[i] - mx) * ux + (ys[i] - my) * uy; return floatArrayOf(mx + ux * t, my + uy * t) }
        return on(a) to on(b)
    }

    /**
     * The shaft from (ax, ay) to (bx, by) (on its fitted line), keeping a third of how the hand
     * swayed off that line (smoothed: the sway, not the tremble; none at the ends, so the head
     * sits right) and the pen's pressure along it (smoothed too).
     */
    private fun shaftLeg(xs: FloatArray, ys: FloatArray, a: Int, b: Int, ax: Float, ay: Float, bx: Float, by: Float, p: (Int) -> Float): List<FloatArray> {
        val l = hypot(bx - ax, by - ay)
        if (l < 1e-3f || b - a < 3) return listOf(floatArrayOf(ax, ay, p(a)), floatArrayOf(bx, by, p(b)))
        val ux = (bx - ax) / l; val uy = (by - ay) / l
        val idx = (a..b).toList()
        val t = idx.map { ((xs[it] - ax) * ux + (ys[it] - ay) * uy).coerceIn(0f, l) }
        val o = idx.map { -(xs[it] - ax) * uy + (ys[it] - ay) * ux }
        val pr = idx.map { p(it) }
        val win = maxOf(2, idx.size / 10)
        fun smooth(v: List<Float>, i: Int) = (maxOf(0, i - win)..minOf(v.size - 1, i + win)).map { v[it] }.average().toFloat()
        val out = ArrayList<FloatArray>(idx.size)
        for (i in idx.indices) {
            val ti = if (i == 0) 0f else if (i == idx.size - 1) l else t[i]
            val fade = minOf(1f, ti / (0.12f * l), (l - ti) / (0.12f * l)).coerceAtLeast(0f)
            val off = 0.33f * smooth(o, i) * fade
            out += floatArrayOf(ax + ux * ti - uy * off, ay + uy * ti + ux * off, smooth(pr, i))
        }
        // Along the line in order (a wobble backwards would fold the stroke).
        val sorted = out.sortedBy { (it[0] - ax) * ux + (it[1] - ay) * uy }
        return listOf(floatArrayOf(ax, ay, sorted.first()[2])) + sorted.drop(1).dropLast(1) + listOf(floatArrayOf(bx, by, sorted.last()[2]))
    }

    // --- shared -------------------------------------------------------------------------------

    /**
     * The barbs of a head at the tip, the shaft running from it along (bx, by): symmetric about
     * the shaft, each keeping a third of how it differed from the other; one barb drawn gets
     * its mirror image. [joined]: the head alone, as one V (barb, tip, barb); else drawn out
     * from the tip and back for each barb (to follow on from the shaft).
     */
    private fun headLegs(tx: Float, ty: Float, bx: Float, by: Float, drawn: List<Barb>, shaft: Float, joined: Boolean = false): List<Leg> {
        val bl = hypot(bx, by)
        val ux = bx / bl; val uy = by / bl
        val angs = drawn.map { atan2(ux * it.dy - uy * it.dx, it.dx * ux + it.dy * uy) }
        val lens = drawn.map { hypot(it.dx, it.dy) }
        val meanA = angs.map { abs(it) }.average().toFloat()
        val meanL = lens.average().toFloat()
        fun angle(i: Int) = meanA + 0.33f * (abs(angs[i]) - meanA)
        fun length(i: Int) = meanL + 0.33f * (lens[i] - meanL)
        val minA = Math.toRadians(16.0).toFloat(); val maxA = Math.toRadians(50.0).toFloat()
        val barbs = if (drawn.size == 1) {
            val sg = if (angs[0] >= 0f) 1f else -1f
            listOf(sg, -sg).map { sign -> Triple(sign * angle(0).coerceIn(minA, maxA), length(0), drawn[0].p) }
        } else drawn.indices.map { i -> Triple((if (angs[i] >= 0f) 1f else -1f) * angle(i).coerceIn(minA, maxA), length(i), drawn[i].p) }
        val ends = barbs.map { (a, l0, p) ->
            // Not bigger than a third of the line (a big drawn head is made smaller), 30 pt at most.
            val l = l0.coerceIn(4f, maxOf(4f, minOf(shaft * 0.3f, 30f)))
            val ca = cos(a); val sa = sin(a)
            Triple(tx + (ux * ca - uy * sa) * l, ty + (ux * sa + uy * ca) * l, p)
        }
        return if (joined) {
            val (x1, y1, p1) = ends[0]; val (x2, y2, p2) = ends[1]
            listOf(Leg(x1, y1, tx, ty, p1, taperStart = true, taperEnd = false), Leg(tx, ty, x2, y2, p2, taperStart = false, taperEnd = true))
        } else ends.flatMap { (x, y, p) -> listOf(Leg(tx, ty, x, y, p, taperStart = false, taperEnd = true), Leg(x, y, tx, ty, p, taperStart = true, taperEnd = false)) }
    }

    /**
     * The legs one after the other as samples every 2 pt, the pressure easing off at tapered
     * ends. Close together at each leg's ends (0.25, 0.5, 1 pt …): the renderer smooths a stroke
     * (each point pulled halfway toward the next), which would round the head's point inward by
     * about the spacing and let the line poke out through it.
     */
    private fun sample(legs: List<Leg>, t0: Float, t1: Float): FloatArray {
        val out = ArrayList<Float>()
        for ((li, g) in legs.withIndex()) {
            val seg = (0 until g.pts.size - 1).map { hypot(g.pts[it + 1][0] - g.pts[it][0], g.pts[it + 1][1] - g.pts[it][1]) }
            val l = seg.sum()
            val ease = minOf(4f, l / 3f).coerceAtLeast(1e-3f)
            val near = floatArrayOf(0.25f, 0.5f, 1f, 2f)
            val stops = (listOf(0f) + near.filter { it < l / 2 } + (1..maxOf(1, (l / 2f).toInt() - 1)).map { it * 2f + 2f }.filter { it < l - 2f } +
                near.filter { it < l / 2 }.reversed().map { l - it } + listOf(l)).distinct().sorted()
            var j = 0; var acc = 0f
            for ((k, d) in stops.withIndex()) {
                if (k == 0 && li > 0) continue
                while (j < seg.size - 1 && acc + seg[j] < d) { acc += seg[j]; j++ }
                val f = if (seg.isEmpty() || seg[j] <= 0f) 0f else ((d - acc) / seg[j]).coerceIn(0f, 1f)
                val q0 = g.pts[j]; val q1 = g.pts[minOf(j + 1, g.pts.size - 1)]
                var p = q0[2] + (q1[2] - q0[2]) * f
                // The last 4 pt (or a third) of a free end: down to 55 % of the pressure.
                if (g.taperStart && d < ease) p *= 0.55f + 0.45f * d / ease
                if (g.taperEnd && l - d < ease) p *= 0.55f + 0.45f * (l - d) / ease
                out += q0[0] + (q1[0] - q0[0]) * f; out += q0[1] + (q1[1] - q0[1]) * f; out += p; out += 0f
            }
            // A corner (the head's point, a barb's end): the same point a few times more, so the
            // smoothing settles right on it.
            if (li < legs.size - 1) {
                val e = g.pts.last(); val p = out[out.size - 2]
                repeat(3) { out += e[0]; out += e[1]; out += p; out += 0f }
            }
        }
        val n = out.size / 4
        return FloatArray(out.size) { out[it] }.also { a -> for (i in 0 until n) a[i * 4 + 3] = t0 + (t1 - t0) * i / maxOf(1, n - 1) }
    }

    private fun pathLen(xs: FloatArray, ys: FloatArray, a: Int, b: Int): Float {
        var l = 0f
        for (i in a until b) l += hypot(xs[i + 1] - xs[i], ys[i + 1] - ys[i])
        return l
    }

    /** Douglas–Peucker: the indices of the corners that keep every point within [tol] of the polyline. */
    private fun simplify(xs: FloatArray, ys: FloatArray, tol: Float): List<Int> {
        val keep = BooleanArray(xs.size)
        keep[0] = true; keep[xs.size - 1] = true
        val stack = ArrayDeque<Pair<Int, Int>>()
        stack.addLast(0 to xs.size - 1)
        while (stack.isNotEmpty()) {
            val (i, j) = stack.removeLast()
            var best = -1; var far = tol
            for (q in i + 1 until j) {
                val d = NotebookView.segDist(xs[q], ys[q], xs[i], ys[i], xs[j], ys[j])
                if (d > far) { far = d; best = q }
            }
            if (best >= 0) { keep[best] = true; stack.addLast(i to best); stack.addLast(best to j) }
        }
        return keep.indices.filter { keep[it] }
    }
}
