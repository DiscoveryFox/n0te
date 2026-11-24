package dev.n0bridge.n0te

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import kotlin.math.atan2
import kotlin.math.hypot

/** A held line is selected alone; its end grips change its length and turn it only when pulled sideways. */
class LineGripTest {
    /** FfiStroke.ends for a stroke from (x0, y0) to (x1, y1) that is [length] long along its way. */
    private fun ends(x0: Float, y0: Float, x1: Float, y1: Float, length: Float = hypot(x1 - x0, y1 - y0)) =
        floatArrayOf(x0, y0, 1f, 0f, x1, y1, -1f, 0f, length)

    @Test fun aStraightLineIsOneButACurveOrALetterIsNot() {
        assertTrue("a ruled line", LineGrip.straight(ends(10f, 10f, 210f, 12f), LineGrip.HOLD_MIN))
        assertTrue("a slanted line drawn a bit wobbly", LineGrip.straight(ends(0f, 0f, 80f, 60f, length = 104f), LineGrip.HOLD_MIN))
        assertFalse("an arc: much longer than its chord", LineGrip.straight(ends(0f, 0f, 100f, 0f, length = 160f), LineGrip.HOLD_MIN))
        assertFalse("an \"l\" in a word is too short to be taken alone", LineGrip.straight(ends(0f, 0f, 2f, 14f), LineGrip.HOLD_MIN))
        assertTrue("…but it still gets end grips when it is selected alone", LineGrip.straight(ends(0f, 0f, 2f, 14f)))
        assertFalse("a dot", LineGrip.straight(ends(0f, 0f, 1f, 1f)))
        assertFalse("no ends (a dot, a stroke still being saved)", LineGrip.straight(FloatArray(0)))
        // The grips' own rule (chord from the first and last sample).
        assertTrue(LineGrip.straight(100f, 105f))
        assertFalse(LineGrip.straight(100f, 120f))
        assertTrue("length not known yet", LineGrip.straight(100f, null))
        assertFalse(LineGrip.straight(3f, 3f))
    }

    @Test fun anglesSnapLikeAHeldLine() {
        assertEquals(45.0, LineGrip.snap(40.5)!!, 0.0)
        assertEquals(90.0, LineGrip.snap(85.0)!!, 0.0)
        assertEquals(30.0, LineGrip.snap(32.5)!!, 0.0)
        assertNull("between steps", LineGrip.snap(37.0))
        assertEquals(-45.0, LineGrip.snap(-41.0)!!, 0.0)
        assertEquals(180.0, LineGrip.snap(176.0)!!, 0.0)
    }

    @Test fun pullingAlongTheLineKeepsItsAngle() {
        // A line at 37° (no snap angle), 100 long; the pen a little off it, further out.
        val ox = 79.86f; val oy = 60.18f
        val d = LineGrip.drag(ox, oy, 160f, 128f, turning = false, sideways = 20f)
        assertFalse(d.turning)
        assertEquals("the angle stays", atan2(oy, ox), atan2(d.y, d.x), 1e-4f)
        assertEquals("as long as the pen is far along it", (160f * ox + 128f * oy) / 100f, hypot(d.x, d.y), 0.01f)
        // Shorter: the pen back towards the fixed end.
        val s = LineGrip.drag(ox, oy, 40f, 25f, turning = false, sideways = 20f)
        assertEquals(atan2(oy, ox), atan2(s.y, s.x), 1e-4f)
        assertTrue(hypot(s.x, s.y) < 50f)
        // Past the fixed end: it doesn't flip round, it stays a short line.
        val past = LineGrip.drag(ox, oy, -30f, -20f, turning = false, sideways = 20f)
        assertEquals(atan2(oy, ox), atan2(past.y, past.x), 1e-4f)
        assertEquals(4f, hypot(past.x, past.y), 0.01f)
    }

    @Test fun pullingClearlySidewaysTurnsItWithSnaps() {
        // A flat line, 100 long; the pen far below its end.
        val d = LineGrip.drag(100f, 0f, 100f, 97f, turning = false, sideways = 20f)
        assertTrue(d.turning)
        assertEquals("44° snaps to 45°", 45, d.snapDeg)
        assertEquals(d.x, d.y, 0.01f)
        assertEquals(hypot(100f, 97f), hypot(d.x, d.y), 0.01f)
        // No step near: it follows the pen.
        val free = LineGrip.drag(100f, 0f, 100f, 75f, turning = true, sideways = 20f)
        assertNull(free.snapDeg)
        assertEquals(100f, free.x, 0f); assertEquals(75f, free.y, 0f)
        // Turning goes on until the pen comes clearly back to the line (ahead of the fixed end).
        assertTrue(LineGrip.drag(100f, 0f, 150f, 15f, turning = true, sideways = 20f).turning)
        assertFalse(LineGrip.drag(100f, 0f, 150f, 8f, turning = true, sideways = 20f).turning)
        assertTrue("on the other side of the fixed end", LineGrip.drag(100f, 0f, -150f, 2f, turning = true, sideways = 20f).turning)
    }
}
