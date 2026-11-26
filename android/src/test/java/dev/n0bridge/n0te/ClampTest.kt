package dev.n0bridge.n0te

import org.junit.Assert.assertEquals
import org.junit.Test

/**
 * Clamping where the room can vanish: the "Mitwandern" glyph on a page strip a few dp high, the
 * magnifier's height handle in a short pane, the space label on a tiny page. coerceIn threw
 * (inside onDraw: the app crashed while writing).
 */
class ClampTest {
    @Test fun withRoomItClampsAsUsual() {
        assertEquals(5f, clampTo(5f, 0f, 10f), 0f)
        assertEquals(0f, clampTo(-3f, 0f, 10f), 0f)
        assertEquals(10f, clampTo(12f, 0f, 10f), 0f)
    }

    @Test fun withoutRoomItTakesTheLowEndInsteadOfThrowing() {
        val density = 2.5f
        // A band 20 dp high: glyph between top + 30 dp and bottom - 10 dp, which cross.
        val top = 100f; val bottom = top + 20f * density
        assertEquals(top + 30f * density, clampTo(110f, top + 30f * density, bottom - 10f * density), 0f)
        // The magnifier in a 200 dp pane: at least 140 dp, at most 60 % (120 dp).
        assertEquals(140f * density, clampTo(500f, 140f * density, 200f * density * 0.6f), 0f)
    }
}
