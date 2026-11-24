package dev.n0bridge.n0te

import org.junit.Assert.assertTrue
import org.junit.Test
import kotlin.math.PI
import kotlin.math.cos
import kotlin.math.hypot
import kotlin.math.sin

/**
 * Many shapes drawn the way a hand draws them: corners moved a little, rounded by a random
 * radius, uneven speed, a slow wobble, a hook where the pen lands, the end short of the start
 * or past it, and the pen resting at the end (that is when recognition runs). Each kind must
 * mostly come out as itself, and squares must not turn into circles (nor circles into squares).
 */
class ShapesFuzzTest {
    private val rnd = java.util.Random(42)
    private fun r(a: Float, b: Float) = a + rnd.nextFloat() * (b - a)

    private fun samples(pts: List<Pair<Float, Float>>) =
        FloatArray(pts.size * 4).also { a -> pts.forEachIndexed { i, (x, y) -> a[i * 4] = x; a[i * 4 + 1] = y; a[i * 4 + 2] = 0.5f; a[i * 4 + 3] = i * 8f } }

    /** A closed outline as a function of the way round (0..1). */
    private fun hand(outline: (Float) -> Pair<Float, Float>, size: Float, round: Float): List<Pair<Float, Float>> {
        val start = r(0f, 1f)
        val end = r(-0.12f, 0.18f) // short of closing … past the start
        val total = 1f + end
        val out = ArrayList<Pair<Float, Float>>()
        // Uneven speed: steps from 1 % to 3 % of the size.
        var f = 0f
        val ph1 = r(0f, 6f); val ph2 = r(0f, 6f); val amp = r(0.3f, 1.4f) * size / 100f
        while (f < total) {
            val (x, y) = outline((start + f) % 1f)
            out += (x + amp * sin(f * 9f + ph1)) to (y + amp * cos(f * 7f + ph2))
            f += r(0.004f, 0.012f)
        }
        // Rounded corners: average over a window of the path.
        val k = (round * out.size).toInt()
        val sm = if (k < 1) out else out.indices.map { i -> val w = out.subList(maxOf(0, i - k), minOf(out.size, i + k + 1)); w.map { it.first }.average().toFloat() to w.map { it.second }.average().toFloat() }
        val res = ArrayList(sm)
        // A hook where the pen landed.
        if (rnd.nextFloat() < 0.4f) { val (x0, y0) = res[0]; val a = r(0f, 6.28f); val l = r(2f, 6f); res.add(0, (x0 + l * cos(a)) to (y0 + l * sin(a))) }
        // The pen resting at the end: a small cluster.
        val (xe, ye) = res.last()
        repeat(15) { res += (xe + r(-0.6f, 0.6f)) to (ye + r(-0.6f, 0.6f)) }
        return res
    }

    private fun polygonOutline(c: List<Pair<Float, Float>>): (Float) -> Pair<Float, Float> {
        val lens = c.indices.map { hypot(c[(it + 1) % c.size].first - c[it].first, c[(it + 1) % c.size].second - c[it].second) }
        val per = lens.sum()
        return { t ->
            var d = t * per; var i = 0
            while (i < c.size - 1 && d > lens[i]) { d -= lens[i]; i++ }
            val a = c[i]; val b = c[(i + 1) % c.size]; val q = (d / lens[i]).coerceIn(0f, 1f)
            (a.first + (b.first - a.first) * q) to (a.second + (b.second - a.second) * q)
        }
    }

    private fun place(c: List<Pair<Float, Float>>, size: Float, rot: Float, jit: Float) = c.map { (x, y) ->
        val xr = x * cos(rot) - y * sin(rot); val yr = x * sin(rot) + y * cos(rot)
        (300f + xr * size + r(-jit, jit) * size) to (300f + yr * size + r(-jit, jit) * size)
    }

    private fun rate(name: String, expect: Set<String>, n: Int = 200, make: () -> List<Pair<Float, Float>>): Float {
        var ok = 0
        val got = HashMap<String?, Int>()
        var traced = 0
        repeat(n) {
            val pts = make()
            var why = ""
            val log = StringBuilder()
            Shapes.trace = { why = it; log.append(it).append(" ; ") }
            val res = Shapes.detect(samples(pts), pts.size, still = 2.4f)
            val k = res?.kind
            Shapes.trace = null
            if (k !in expect && traced++ < 6) {
                println("  $name -> $k: $why")
                System.getenv("SHAPES_DUMP")?.let { java.io.File("$it.fuzz").appendText("$name|$k|$log|${pts.joinToString(" ") { "%.1f,%.1f".format(it.first, it.second) }}|${res?.samples?.let { o -> (0 until o.size / 4).joinToString(" ") { "%.1f,%.1f".format(o[it * 4], o[it * 4 + 1]) } } ?: ""}\n") }
            }
            got[k] = (got[k] ?: 0) + 1
            if (k in expect) ok++
        }
        val share = ok.toFloat() / n
        println("$name: ${(share * 100).toInt()} %  $got")
        return share
    }

    @Test fun handDrawnShapes() {
        val square = rate("square", setOf("rectangle")) {
            val s = r(50f, 220f)
            hand(polygonOutline(place(listOf(-0.5f to -0.5f, 0.5f to -0.5f, 0.5f to 0.5f, -0.5f to 0.5f), s, r(-0.08f, 0.08f), 0.05f)), s, r(0f, 0.035f))
        }
        val rect = rate("rectangle", setOf("rectangle")) {
            val s = r(50f, 200f); val ar = r(0.4f, 0.75f)
            hand(polygonOutline(place(listOf(-0.5f to -ar / 2, 0.5f to -ar / 2, 0.5f to ar / 2, -0.5f to ar / 2), s, r(-0.6f, 0.6f), 0.04f)), s, r(0f, 0.03f))
        }
        val tri = rate("triangle", setOf("triangle")) {
            val s = r(50f, 220f)
            hand(polygonOutline(place(listOf(0f to -0.55f, 0.55f to 0.4f, -0.55f to 0.4f), s, r(-0.4f, 0.4f), 0.07f)), s, r(0f, 0.03f))
        }
        val circle = rate("circle", setOf("ellipse")) {
            val s = r(50f, 220f); val e = r(0.75f, 1f)
            hand({ t -> (300f + s / 2 * cos(t * 2 * PI.toFloat())) to (300f + s / 2 * e * sin(t * 2 * PI.toFloat())) }, s, 0f)
        }
        assertTrue("squares $square", square >= 0.97f)
        assertTrue("rectangles $rect", rect >= 0.97f)
        assertTrue("triangles $tri", tri >= 0.97f)
        assertTrue("circles $circle", circle >= 0.97f)
    }
}
