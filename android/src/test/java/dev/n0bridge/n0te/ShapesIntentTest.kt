package dev.n0bridge.n0te

import org.junit.Assert.assertTrue
import org.junit.Test
import kotlin.math.PI
import kotlin.math.abs
import kotlin.math.atan2
import kotlin.math.cos
import kotlin.math.hypot
import kotlin.math.sin

/**
 * What a hand means, not just how many corners it drew: a sloppy rectangle (very round corners,
 * bowed sides, a hook, the pen trembling while it rests) must come out as a rectangle that is
 * level when it was drawn about level, at the angle it was drawn otherwise, with its corners
 * where the drawn ones were.
 */
class ShapesIntentTest {
    private val rnd = java.util.Random(11)
    private fun r(a: Float, b: Float) = a + rnd.nextFloat() * (b - a)

    private fun samples(pts: List<Pair<Float, Float>>) =
        FloatArray(pts.size * 4).also { a -> pts.forEachIndexed { i, (x, y) -> a[i * 4] = x; a[i * 4 + 1] = y; a[i * 4 + 2] = 0.5f; a[i * 4 + 3] = i * 4f } }

    class Drawn(val pts: List<Pair<Float, Float>>, val corners: List<Pair<Float, Float>>, val tilt: Float, val size: Float)

    /** A rectangle w × h at [tilt] (radians) drawn sloppily. */
    private fun sloppy(w: Float, h: Float, tilt: Float, maxRound: Float, bow: Float): Drawn {
        val base = listOf(-w / 2 to -h / 2, w / 2 to -h / 2, w / 2 to h / 2, -w / 2 to h / 2)
        val c = base.map { (x, y) ->
            val jx = x + r(-0.025f, 0.025f) * w; val jy = y + r(-0.025f, 0.025f) * w
            (300f + jx * cos(tilt) - jy * sin(tilt)) to (300f + jx * sin(tilt) + jy * cos(tilt))
        }
        // The outline: bowed sides, corners rounded by a quadratic curve cut back d on both sides.
        val outline = ArrayList<Pair<Float, Float>>()
        val cut = FloatArray(4) { r(0f, maxRound) * minOf(w, h) }
        for (i in 0 until 4) {
            val a = c[i]; val b = c[(i + 1) % 4]
            val len = hypot(b.first - a.first, b.second - a.second)
            val ux = (b.first - a.first) / len; val uy = (b.second - a.second) / len
            val sag = r(-bow, bow) * len
            val d0 = cut[i]; val d1 = cut[(i + 1) % 4]
            // The side from a + d0 to b - d1, bowed.
            val steps = 60
            for (k in 0..steps) {
                val u = k / steps.toFloat()
                val dist = d0 + (len - d0 - d1) * u
                val off = sag * sin(PI.toFloat() * dist / len)
                outline += (a.first + ux * dist - uy * off) to (a.second + uy * dist + ux * off)
            }
            // The corner at b, rounded.
            val n = c[(i + 2) % 4]
            val l2 = hypot(n.first - b.first, n.second - b.second)
            val vx = (n.first - b.first) / l2; val vy = (n.second - b.second) / l2
            val p0 = (b.first - ux * d1) to (b.second - uy * d1); val p2 = (b.first + vx * d1) to (b.second + vy * d1)
            for (k in 1 until 12) {
                val t = k / 12f
                val x = (1 - t) * (1 - t) * p0.first + 2 * (1 - t) * t * b.first + t * t * p2.first
                val y = (1 - t) * (1 - t) * p0.second + 2 * (1 - t) * t * b.second + t * t * p2.second
                outline += x to y
            }
        }
        // Walk it by length from a random start, uneven speed, short of or past the start.
        val seg = outline.indices.map { val j = (it + 1) % outline.size; hypot(outline[j].first - outline[it].first, outline[j].second - outline[it].second) }
        val per = seg.sum()
        fun at(d0: Float): Pair<Float, Float> {
            var d = ((d0 % per) + per) % per
            for (i in seg.indices) { if (d <= seg[i]) { val a = outline[i]; val b = outline[(i + 1) % outline.size]; val f = d / seg[i].coerceAtLeast(1e-3f); return (a.first + (b.first - a.first) * f) to (a.second + (b.second - a.second) * f) }; d -= seg[i] }
            return outline[0]
        }
        val start = r(0f, per); val end = per * (1f + r(-0.08f, 0.12f))
        val pts = ArrayList<Pair<Float, Float>>()
        val size = maxOf(w, h)
        // Landing: a few trembles and maybe a hook.
        val s0 = at(start)
        repeat(rnd.nextInt(20)) { pts += (s0.first + r(-1f, 1f)) to (s0.second + r(-1f, 1f)) }
        if (rnd.nextFloat() < 0.4f) { val a = r(0f, 6.28f); val l = r(2f, 6f); pts += (s0.first + l * cos(a)) to (s0.second + l * sin(a)) }
        var d = 0f
        val wob = r(0.2f, 1.2f) * size / 100f; val ph = r(0f, 6f)
        while (d < end) { val (x, y) = at(start + d); pts += (x + wob * sin(d / 30f + ph)) to (y + wob * cos(d / 23f + ph)); d += r(0.6f, 3.5f) }
        // Resting: the pen trembles about the end (a slow random walk).
        val e = pts.last(); var jx = 0f; var jy = 0f; val amp = r(0.5f, 2.5f)
        repeat(30 + rnd.nextInt(100)) { jx = (jx + r(-0.7f, 0.7f)).coerceIn(-amp, amp); jy = (jy + r(-0.7f, 0.7f)).coerceIn(-amp, amp); pts += (e.first + jx) to (e.second + jy) }
        return Drawn(pts, c, tilt, size)
    }

    /** The four corners of a result (where its direction turns). */
    private fun cornersOf(o: FloatArray): List<Pair<Float, Float>> {
        val m = o.size / 4
        return (0 until m - 1).filter { i ->
            val p = if (i == 0) m - 2 else i - 1
            val ax = o[i * 4] - o[p * 4]; val ay = o[i * 4 + 1] - o[p * 4 + 1]; val bx = o[(i + 1) * 4] - o[i * 4]; val by = o[(i + 1) * 4 + 1] - o[i * 4 + 1]
            abs(ax * by - ay * bx) > 0.2f * hypot(ax, ay) * hypot(bx, by)
        }.map { o[it * 4] to o[it * 4 + 1] }
    }

    private val dump: StringBuilder? = System.getenv("SHAPES_DUMP")?.let { StringBuilder() }

    private fun rate(name: String, n: Int, level: Boolean, make: () -> Drawn): Float {
        var ok = 0
        val why = HashMap<String, Int>()
        repeat(n) {
            val dr = make()
            val log = StringBuilder()
            Shapes.trace = { log.append(it).append(" ; ") }
            val res = Shapes.detect(samples(dr.pts), dr.pts.size, still = 2.4f)
            Shapes.trace = null
            val bad = when {
                res == null -> "none"
                res.kind != "rectangle" -> res.kind
                else -> {
                    val cs = cornersOf(res.samples)
                    val ang = atan2(cs[1].second - cs[0].second, cs[1].first - cs[0].first)
                    val off = ((Math.toDegrees((ang - if (level) 0f else dr.tilt).toDouble()) % 90.0) + 135.0) % 90.0 - 45.0
                    val far = dr.corners.maxOf { t -> cs.minOf { hypot(it.first - t.first, it.second - t.second) } } / dr.size
                    when {
                        cs.size != 4 -> "corners ${cs.size}"
                        level && abs(off) > 0.01 -> "not level"
                        !level && abs(off) > 6.0 -> "angle off"
                        far > 0.09f -> "corner off"
                        else -> null
                    }
                }
            }
            if (bad == null) ok++ else {
                why[bad] = (why[bad] ?: 0) + 1
                if (dump != null && (why[bad] ?: 0) <= 4) dump.append("$name|$bad|$log|${dr.pts.joinToString(" ") { "%.1f,%.1f".format(it.first, it.second) }}|${res?.samples?.let { o -> (0 until o.size / 4).joinToString(" ") { "%.1f,%.1f".format(o[it * 4], o[it * 4 + 1]) } } ?: ""}\n")
            }
        }
        println("$name: ${ok * 100 / n} %  $why")
        return ok.toFloat() / n
    }

    @Test fun sloppyRectanglesComeOutClean() {
        val level = rate("level, round corners", 300, true) {
            val w = r(60f, 220f); sloppy(w, w * r(0.45f, 1f), Math.toRadians(r(-5f, 5f).toDouble()).toFloat(), 0.22f, 0.03f)
        }
        val tilted = rate("tilted 15–35°", 200, false) {
            val w = r(60f, 220f); val s = if (rnd.nextBoolean()) 1 else -1
            sloppy(w, w * r(0.45f, 1f), s * Math.toRadians(r(15f, 35f).toDouble()).toFloat(), 0.15f, 0.025f)
        }
        System.getenv("SHAPES_DUMP")?.let { java.io.File(it).writeText(dump.toString()) }
        assertTrue("level $level", level >= 0.95f)
        assertTrue("tilted $tilted", tilted >= 0.9f)
    }
}
