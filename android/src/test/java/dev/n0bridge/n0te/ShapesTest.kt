package dev.n0bridge.n0te

import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import kotlin.math.PI
import kotlin.math.abs
import kotlin.math.cos
import kotlin.math.hypot
import kotlin.math.sin

/** Hand-drawn-like strokes (overshoot, wobble, rounded corners, hooks) and what they become. */
class ShapesTest {
    private val rnd = java.util.Random(7)
    private fun jitter(a: Float) = ((rnd.nextFloat() - 0.5f) * 2 * a)

    private fun samples(pts: List<Pair<Float, Float>>) =
        FloatArray(pts.size * 4).also { a -> pts.forEachIndexed { i, (x, y) -> a[i * 4] = x; a[i * 4 + 1] = y; a[i * 4 + 2] = 0.5f; a[i * 4 + 3] = i * 8f } }

    private fun kind(pts: List<Pair<Float, Float>>) = Shapes.detect(samples(pts), pts.size)?.kind

    /** An ellipse drawn by hand: start angle, sweep (> 2π overshoots), wobble. */
    private fun ellipse(rx: Float, ry: Float, start: Float, sweep: Float, wobble: Float = 2f, n: Int = 90) =
        (0 until n).map { i -> val t = start + sweep * i / (n - 1); (200 + rx * cos(t) + jitter(wobble)) to (200 + ry * sin(t) + jitter(wobble)) }

    /** A polygon traced from [start] (fraction of the perimeter), [extra] of the perimeter past the end, rounded by [round]. */
    private fun polygon(corners: List<Pair<Float, Float>>, start: Float = 0f, extra: Float = 0f, wobble: Float = 1.5f, step: Float = 4f, round: Float = 0f): List<Pair<Float, Float>> {
        val segs = corners.indices.map { corners[it] to corners[(it + 1) % corners.size] }
        val lens = segs.map { (a, b) -> hypot(b.first - a.first, b.second - a.second) }
        val per = lens.sum()
        fun at(d0: Float): Pair<Float, Float> {
            var d = ((d0 % per) + per) % per
            for ((i, l) in lens.withIndex()) {
                if (d <= l) { val (a, b) = segs[i]; val f = d / l; return (a.first + (b.first - a.first) * f) to (a.second + (b.second - a.second) * f) }
                d -= l
            }
            return corners[0]
        }
        val total = per * (1 + extra)
        val raw = (0..(total / step).toInt()).map { at(per * start + it * step) }
        // Round the corners: average with neighbours.
        val k = (round / step).toInt()
        val smooth = if (k <= 0) raw else raw.indices.map { i ->
            val w = raw.subList(maxOf(0, i - k), minOf(raw.size, i + k + 1)); w.map { it.first }.average().toFloat() to w.map { it.second }.average().toFloat()
        }
        return smooth.map { (x, y) -> (x + jitter(wobble)) to (y + jitter(wobble)) }
    }

    private val rect = listOf(100f to 100f, 300f to 100f, 300f to 220f, 100f to 220f)
    private val tri = listOf(100f to 300f, 220f to 100f, 340f to 300f)

    private fun rotated(c: List<Pair<Float, Float>>, deg: Float): List<Pair<Float, Float>> {
        val a = deg * PI.toFloat() / 180; val cx = c.map { it.first }.average().toFloat(); val cy = c.map { it.second }.average().toFloat()
        return c.map { (x, y) -> (cx + (x - cx) * cos(a) - (y - cy) * sin(a)) to (cy + (x - cx) * sin(a) + (y - cy) * cos(a)) }
    }

    @Test fun circles() {
        assertEquals("exact", "ellipse", kind(ellipse(70f, 70f, 0.3f, 2 * PI.toFloat())))
        assertEquals("overshoot 10 %", "ellipse", kind(ellipse(70f, 70f, 1f, 2.2f * PI.toFloat())))
        assertEquals("short of closing 6 %", "ellipse", kind(ellipse(70f, 70f, 2f, 1.88f * PI.toFloat())))
        assertEquals("wobbly", "ellipse", kind(ellipse(70f, 64f, 0f, 2.05f * PI.toFloat(), wobble = 4f)))
        assertEquals("flat ellipse", "ellipse", kind(ellipse(110f, 45f, 0.5f, 2.1f * PI.toFloat())))
    }

    @Test fun rectangles() {
        assertEquals("from a corner", "rectangle", kind(polygon(rect)))
        assertEquals("from the middle of a side", "rectangle", kind(polygon(rect, start = 0.12f)))
        assertEquals("overshoot", "rectangle", kind(polygon(rect, start = 0.05f, extra = 0.12f)))
        assertEquals("rounded corners", "rectangle", kind(polygon(rect, round = 12f, extra = 0.05f)))
        assertEquals("wobbly", "rectangle", kind(polygon(rect, wobble = 4f, extra = 0.04f)))
        assertEquals("tilted 20°", "rectangle", kind(polygon(rotated(rect, 20f), start = 0.3f, extra = 0.06f)))
    }

    @Test fun triangles() {
        assertEquals("plain", "triangle", kind(polygon(tri)))
        assertEquals("overshoot, mid-side start", "triangle", kind(polygon(tri, start = 0.2f, extra = 0.12f)))
        assertEquals("rounded, wobbly", "triangle", kind(polygon(tri, round = 10f, wobble = 3f, extra = 0.05f)))
    }

    /** A path through [pts] (corners as drawn), filled in every 3 units, with a little wobble. */
    private fun traced(pts: List<Pair<Float, Float>>, scale: Float = 2f): List<Pair<Float, Float>> {
        val out = ArrayList<Pair<Float, Float>>()
        for (i in 0 until pts.size - 1) {
            val (ax, ay) = pts[i]; val (bx, by) = pts[i + 1]
            val steps = maxOf(1, (hypot(bx - ax, by - ay) * scale / 3f).toInt())
            for (k in 0 until steps) out += ((ax + (bx - ax) * k / steps) * scale + jitter(0.8f)) to ((ay + (by - ay) * k / steps) * scale + jitter(0.8f))
        }
        out += (pts.last().first * scale) to (pts.last().second * scale)
        return out
    }

    /** The shapes from a user's screenshot: a loose square with round corners, a triangle with a curl. */
    @Test fun looseShapesFromTheTablet() {
        val square = traced(listOf(75f to 48f, 78f to 43f, 86f to 40f, 120f to 40f, 193f to 44f, 190f to 80f, 186f to 110f, 180f to 117f, 172f to 121f,
            150f to 125f, 120f to 123f, 85f to 120f, 78f to 112f, 74f to 100f, 72f to 85f, 73f to 60f, 75f to 50f))
        assertEquals("loose square", "rectangle", kind(square))
        val triangle = traced(listOf(44f to 318f, 85f to 280f, 125f to 243f, 160f to 280f, 186f to 305f, 193f to 315f, 194f to 322f, 190f to 328f,
            140f to 328f, 90f to 327f, 52f to 325f, 44f to 320f))
        assertEquals("triangle with a curl", "triangle", kind(triangle))
        // The result is clean: three corners joined by straight sides (no curl left).
        val clean = Shapes.detect(samples(triangle), triangle.size)!!.samples
        val bottom = (0 until clean.size / 4).map { clean[it * 4 + 1] }.max()
        assertEquals("the base is level", 656f, bottom, 8f)
    }

    /**
     * A user's rectangle traced from their screenshot (round corners, one side bowed, starting at
     * the sharp corner top right), the pen trembling where it rests: a level rectangle where it was drawn.
     */
    @Test fun roundRectangleFromTheTablet() {
        val drawn = "176,29 178,35 178,42 177,48 177,54 177,60 176,65 177,70 177,75 178,81 178,87 176,92 178,98 178,103 178,108 177,114 178,120 174,124 170,128 165,131 160,133 154,134 149,136 144,136 139,137 134,137 130,137 126,138 121,137 117,137 113,137 107,138 101,138 97,138 92,139 88,140 84,139 78,139 74,139 68,140 62,138 54,139 46,138 42,134 38,130 36,125 34,119 33,113 33,107 31,102 32,96 30,91 30,84 31,77 28,70 30,65 30,59 30,53 30,46 34,42 40,38 45,36 52,34 58,33 65,33 70,31 74,30 80,31 84,30 88,30 92,30 96,30 100,31 104,28 110,30 116,28 120,30 123,30 127,30 132,29 136,29 142,29 146,30 153,29 160,29 168,29".split(" ").map { val (x, y) = it.split(","); x.toFloat() to y.toFloat() }
        for (scale in listOf(1f, 2.5f)) for (reverse in listOf(false, true)) {
            val path = (if (reverse) listOf(drawn[0]) + drawn.drop(1).reversed() else drawn).map { (x, y) -> x * scale to y * scale }
            val pts = traced(path, 1f).toMutableList()
            val (ex, ey) = pts.last()
            var jx = 0f; var jy = 0f
            repeat(80) { jx = (jx + jitter(0.7f)).coerceIn(-2f, 2f); jy = (jy + jitter(0.7f)).coerceIn(-2f, 2f); pts += (ex + jx) to (ey + jy) }
            val (k, out) = Shapes.detect(samples(pts), pts.size, still = 2.4f)!!
            assertEquals("scale $scale reverse $reverse", "rectangle", k)
            val xs = (0 until out.size / 4).map { out[it * 4] / scale }; val ys = (0 until out.size / 4).map { out[it * 4 + 1] / scale }
            assertEquals(31f, xs.min(), 3f); assertEquals(177f, xs.max(), 3f)
            assertEquals(30f, ys.min(), 3f); assertEquals(138f, ys.max(), 3f)
            // Level: every point lies on one of the four sides.
            assertTrue(xs.indices.all { i -> minOf(abs(xs[i] - xs.min()), abs(xs[i] - xs.max()), abs(ys[i] - ys.min()), abs(ys[i] - ys.max())) < 0.01f })
        }
    }

    @Test fun nearlySquareIsSquare() {
        val sq = polygon(listOf(100f to 100f, 300f to 100f, 300f to 290f, 100f to 290f), wobble = 1f, extra = 0.03f)
        val res = Shapes.detect(samples(sq), sq.size)!!.samples
        val xs = (0 until res.size / 4).map { res[it * 4] }; val ys = (0 until res.size / 4).map { res[it * 4 + 1] }
        assertEquals(xs.max() - xs.min(), ys.max() - ys.min(), 0.5f)
    }

    /** Drawn by the core (which smooths every stroke), a clean rectangle keeps all four corners sharp, not just the first. */
    @Test fun cornersStaySharpOnThePage() {
        val drawn = polygon(rect, start = 0.3f, extra = 0.03f)
        val res = Shapes.detect(samples(drawn), drawn.size)!!.samples
        val xs = (0 until res.size / 4).map { res[it * 4] }; val ys = (0 until res.size / 4).map { res[it * 4 + 1] }
        val outline = floatsOf(uniffi.n0te_ffi.strokeOutline(uniffi.n0te_ffi.FfiBrush(2f, 0f, 0xFF000000u, 0u), bytesOf(res)))
        for (cx in listOf(xs.min(), xs.max())) for (cy in listOf(ys.min(), ys.max())) {
            val near = (0 until outline.size / 2).minOf { hypot(outline[it * 2] - cx, outline[it * 2 + 1] - cy) }
            assertTrue("corner ($cx, $cy) rounded off by $near", near < 1.8f)
        }
    }

    /** Left, top, right, bottom of a result. */
    private fun box(o: FloatArray): FloatArray {
        val xs = (0 until o.size / 4).map { o[it * 4] }; val ys = (0 until o.size / 4).map { o[it * 4 + 1] }
        return floatArrayOf(xs.min(), ys.min(), xs.max(), ys.max())
    }

    /** A snapped shape held under the pen: the corner by the pen follows it, the opposite corner stays. */
    @Test fun heldShapesChangeSize() {
        // A level rectangle 100..300 × 100..220, the pen resting at its top-left corner.
        val drawn = polygon(rect, extra = 0.02f)
        val held = Shapes.HeldShape(Shapes.detect(samples(drawn), drawn.size)!!, 100f, 100f)
        var b = box(held.drag(-20f, -10f, 0f))
        assertArrayEquals(floatArrayOf(80f, 90f, 300f, 220f), b, 1.5f)
        // Only sideways: only the width changes.
        b = box(held.drag(50f, 0f, 0f))
        assertArrayEquals(floatArrayOf(150f, 100f, 300f, 220f), b, 1.5f)
        assertTrue(!held.square)
        // Sides nearly alike: a square (the side being pulled gives way), flagged for the tick.
        b = box(held.drag(76f, 0f, 0f))
        assertTrue(held.square)
        assertEquals(b[2] - b[0], b[3] - b[1], 0.01f)
        assertArrayEquals(floatArrayOf(300f, 220f), floatArrayOf(b[2], b[3]), 1.5f)

        // A circle (radius 70 round 200, 200), the pen at its start lower right: pulled right, an ellipse.
        val circle = ellipse(70f, 70f, 0.3f, 2 * PI.toFloat())
        val c = Shapes.detect(samples(circle), circle.size)!!
        assertEquals("ellipse", c.kind)
        val (px, py) = circle.last()
        b = box(Shapes.HeldShape(c, px, py).drag(30f, 0f, 0f))
        assertArrayEquals(floatArrayOf(130f, 130f, 300f, 270f), b, 4f)

        // A rectangle drawn at 20° keeps its angle while it grows.
        val tilted = polygon(rotated(rect, 20f), start = 0.3f, extra = 0.06f)
        val t = Shapes.detect(samples(tilted), tilted.size)!!
        assertEquals("rectangle", t.kind)
        val k = t.corners!!
        val out = Shapes.HeldShape(t, k[0], k[1]).drag(-15f, 10f, 0f)
        val before = Math.toDegrees(kotlin.math.atan2((k[3] - k[1]).toDouble(), (k[2] - k[0]).toDouble()))
        val after = Math.toDegrees(kotlin.math.atan2((out[5] - out[1]).toDouble(), (out[4] - out[0]).toDouble()))
        assertEquals(before, after, 0.5)
    }

    @Test fun lines() {
        val straight = (0..60).map { (100f + it * 4f + jitter(1.5f)) to (200f + it * 1.2f + jitter(1.5f)) }
        assertEquals("straight", "line", kind(straight))
        val sag = (0..60).map { val f = it / 60f; (100f + it * 4f) to (200f + 10f * sin(PI.toFloat() * f) + jitter(1f)) }
        assertEquals("slight sag", "line", kind(sag))
        val hook = (0..60).map { (100f + it * 4f) to 200f + jitter(1f) } + (1..4).map { (340f + it * 2f) to (200f + it * 4f) }
        assertEquals("hook at the end", "line", kind(hook))
    }

    @Test fun scribblesStayAsTheyAre() {
        val wave = (0..120).map { (100f + it * 3f) to (200f + 30f * sin(it / 6f)) }
        assertEquals(null, kind(wave))
        val spiral = (0..150).map { val t = it / 10f; (200 + t * 6 * cos(t)) to (200 + t * 6 * sin(t)) }
        assertEquals(null, kind(spiral))
    }
}
