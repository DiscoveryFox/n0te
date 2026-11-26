package dev.n0bridge.n0te

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.n0te_ffi.FfiBrush
import uniffi.n0te_ffi.FfiScribbleTarget
import uniffi.n0te_ffi.scribbleJudge
import uniffi.n0te_ffi.scribbleMoves
import uniffi.n0te_ffi.strokeOutline
import kotlin.math.PI
import kotlin.math.cos
import kotlin.math.sin

/** Scribbling out as the tablet asks the core: the gesture's samples, the strokes' outlines. */
class ScribbleTest {
    private val pen = FfiBrush(1.5f, 0.5f, 0xFF111111u, 0u)

    /** Samples (x, y, pressure, t) from points, [msPerPoint] apart. */
    private fun samples(pts: List<Pair<Float, Float>>, msPerPoint: Float = 8f) =
        FloatArray(pts.size * 4).also { a -> pts.forEachIndexed { i, (x, y) -> a[i * 4] = x; a[i * 4 + 1] = y; a[i * 4 + 2] = 0.5f; a[i * 4 + 3] = i * msPerPoint } }

    private fun bytes(pts: List<Pair<Float, Float>>, msPerPoint: Float = 8f) = bytesOf(samples(pts, msPerPoint))

    /** A zigzag over the box (x, y, w, h): passes across its width, moving down. */
    private fun zigzag(x: Float, y: Float, w: Float, h: Float, passes: Int, perPass: Int = 6): List<Pair<Float, Float>> =
        (0 until passes * perPass).map { i ->
            val leg = i / perPass; val f = (i % perPass) / perPass.toFloat()
            val px = if (leg % 2 == 0) x + f * w else x + (1 - f) * w
            px to y + h * i / (passes * perPass)
        }

    /** A stroke as the tablet keeps it: its outline. */
    private fun target(id: String, pts: List<Pair<Float, Float>>) = FfiScribbleTarget(id, strokeOutline(pen, bytes(pts)), false)

    @Test fun quickZigzagMovesLikeAScribble() {
        assertTrue(scribbleMoves(bytes(zigzag(0f, 0f, 40f, 12f, 8))))
    }

    @Test fun straightLineAndSlowHatchingDoNot() {
        assertFalse(scribbleMoves(bytes((0 until 60).map { it * 2f to 0f })))
        assertFalse(scribbleMoves(bytes(zigzag(0f, 0f, 40f, 12f, 8), msPerPoint = 80f)))
    }

    @Test fun scribblingOverALetterDeletesItButNotItsNeighbour() {
        val letter = listOf(50f to 45f, 50f to 50f, 51f to 55f)
        val neighbour = listOf(70f to 45f, 70f to 55f)
        val v = scribbleJudge(bytes(zigzag(44f, 43f, 13f, 14f, 10)), pen.width, listOf(target("l", letter), target("n", neighbour)))
        assertEquals(listOf("l"), v.delete)
        assertTrue(v.cut.isEmpty())
    }

    @Test fun colouringInsideAnOutlineKeepsTheOutlineAndIsInk() {
        // Drawn by hand: samples close together, as the pen reports them.
        val corners = listOf(0f to 0f, 60f to 0f, 60f to 40f, 0f to 40f, 0f to 0f)
        val box = corners.zipWithNext().flatMap { (a, b) -> (0 until 40).map { k -> val t = k / 40f; a.first + (b.first - a.first) * t to a.second + (b.second - a.second) * t } }
        // Passes from edge to edge, overshooting it a little.
        val colouring = (0 until 18).map { i -> (if (i % 2 == 0) -1f else 61f) to 2f + i * 2f }
        val v = scribbleJudge(bytes(colouring, msPerPoint = 40f), pen.width, listOf(target("box", box)))
        assertTrue("colouring keeps the outline", v.delete.isEmpty() && v.cut.isEmpty())
    }

    @Test fun loopsOverNothingAreInk() {
        // "eeee": loops moving right, nothing under them.
        val loops = (0 until 160).map { i -> val t = i / 40f * 2 * PI.toFloat(); (i * 0.5f + 6 * cos(t)) to (6 * sin(t)) }
        assertTrue(scribbleJudge(bytes(loops), pen.width, emptyList()).delete.isEmpty())
    }
}
