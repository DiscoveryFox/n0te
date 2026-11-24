package dev.n0bridge.n0te

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import kotlin.math.PI
import kotlin.math.cos
import kotlin.math.sin

class ScribbleTest {
    /** Samples (x, y, pressure, t) from points, [msPerPoint] apart. */
    private fun samples(pts: List<Pair<Float, Float>>, msPerPoint: Float = 8f) =
        FloatArray(pts.size * 4).also { a -> pts.forEachIndexed { i, (x, y) -> a[i * 4] = x; a[i * 4 + 1] = y; a[i * 4 + 2] = 0.5f; a[i * 4 + 3] = i * msPerPoint } }

    private fun zigzag(width: Float, height: Float, strokes: Int, perStroke: Int = 8): List<Pair<Float, Float>> =
        (0 until strokes * perStroke).map { i ->
            val leg = i / perStroke; val f = (i % perStroke) / perStroke.toFloat()
            val x = if (leg % 2 == 0) f * width else (1 - f) * width
            x to height * i / (strokes * perStroke)
        }

    @Test fun quickZigzagIsAScribble() {
        val z = zigzag(40f, 12f, 8)
        assertTrue(NotebookView.isScribble(samples(z), z.size))
    }

    @Test fun handwritingLoopsAreNot() {
        // "eeee": loops moving right.
        val loops = (0 until 160).map { i -> val t = i / 40f * 2 * PI.toFloat(); (i * 0.5f + 6 * cos(t)) to (6 * sin(t)) }
        assertFalse(NotebookView.isScribble(samples(loops), loops.size))
    }

    @Test fun straightLineIsNot() {
        val line = (0 until 60).map { it * 2f to 0f }
        assertFalse(NotebookView.isScribble(samples(line), line.size))
    }

    @Test fun slowHatchingIsDrawingNotScribbling() {
        val z = zigzag(40f, 12f, 8)
        assertFalse(NotebookView.isScribble(samples(z, msPerPoint = 60f), z.size))
    }
}
