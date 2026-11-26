package dev.n0bridge.n0te

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import kotlin.math.cos
import kotlin.math.sin

/** The lasso's grid answers exactly like the polygon test, only faster. */
class LassoMaskTest {
    /** A hand-drawn lasso: a wobbly loop around (cx, cy), sometimes crossing itself. */
    private fun lasso(rnd: java.util.Random, cx: Float, cy: Float, r: Float, points: Int): FloatArray {
        val f = FloatArray(points * 2)
        var wob = 0f
        for (i in 0 until points) {
            wob = (wob + (rnd.nextFloat() - 0.5f) * r * 0.15f).coerceIn(-r * 0.6f, r * 0.6f)
            val a = i * 6.2832f / points * (if (rnd.nextInt(9) == 0) 1.15f else 1f)
            f[2 * i] = cx + (r + wob) * cos(a)
            f[2 * i + 1] = cy + (r * 0.7f + wob) * sin(a)
        }
        return f
    }

    @Test fun sameAnswersAsThePolygonTest() {
        val rnd = java.util.Random(3)
        repeat(40) { round ->
            val poly = lasso(rnd, 300f, 400f, 50f + rnd.nextFloat() * 400f, 20 + rnd.nextInt(600))
            val mask = LassoMask(poly, maxCells = if (round % 4 == 0) 2000 else 1 shl 20)
            // 5000 random points a lasso (200 000 in all) plus every point of it: as strict, 4× faster.
            repeat(5_000) {
                val x = -200f + rnd.nextFloat() * 1000f; val y = -200f + rnd.nextFloat() * 1200f
                assertEquals("round $round at $x, $y", NotebookView.pointInPolygon(x, y, poly), mask.contains(x, y))
            }
            // Right on and next to its points too.
            for (i in 0 until poly.size / 2) for (d in floatArrayOf(-0.01f, 0f, 0.01f)) {
                val x = poly[2 * i] + d; val y = poly[2 * i + 1] - d
                assertEquals(NotebookView.pointInPolygon(x, y, poly), mask.contains(x, y))
            }
        }
    }

    @Test fun aBoxAndNoLasso() {
        val box = LassoMask(floatArrayOf(10f, 10f, 110f, 10f, 110f, 60f, 10f, 60f))
        assertTrue(box.contains(50f, 30f))
        assertFalse(box.contains(5f, 30f))
        assertFalse(box.contains(50f, 70f))
        assertFalse(LassoMask(floatArrayOf(1f, 1f, 5f, 5f)).contains(3f, 3f))
        assertFalse(LassoMask(FloatArray(0)).contains(0f, 0f))
    }
}
