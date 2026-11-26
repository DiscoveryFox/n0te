package dev.n0bridge.n0te

import org.junit.Assert.assertArrayEquals
import org.junit.Test

/** The box around a stroke's outline as it shows before the core has stored it. */
class PointBoundsTest {
    @Test fun theFirstPointCountsOnEverySide() {
        // A stroke drawn right to left: its outline starts at its right end. RectF.union(x, y)
        // from an inverted box only moved the left edge for it, so the right edge came from a
        // later point (or stayed at -MAX for a single point).
        assertArrayEquals(floatArrayOf(10f, 5f, 100f, 50f), pointBounds(floatArrayOf(100f, 50f, 60f, 30f, 10f, 5f)), 0f)
        assertArrayEquals(floatArrayOf(7f, 8f, 7f, 8f), pointBounds(floatArrayOf(7f, 8f)), 0f)
    }

    @Test fun anyOrderGivesTheSameBox() {
        val pts = floatArrayOf(3f, 9f, -4f, 2f, 8f, -1f, 0f, 0f)
        val want = floatArrayOf(-4f, -1f, 8f, 9f)
        assertArrayEquals(want, pointBounds(pts), 0f)
        assertArrayEquals(want, pointBounds(floatArrayOf(8f, -1f, 0f, 0f, 3f, 9f, -4f, 2f)), 0f)
    }

    @Test fun nothingAndAnOddLastNumber() {
        assertArrayEquals(floatArrayOf(0f, 0f, 0f, 0f), pointBounds(FloatArray(0)), 0f)
        assertArrayEquals(floatArrayOf(1f, 2f, 1f, 2f), pointBounds(floatArrayOf(1f, 2f, 99f)), 0f)
    }
}
