package dev.n0bridge.n0te

import org.junit.Assert.assertTrue
import org.junit.Test

class FountainTest {
    /** A straight stroke from (0, 0) in direction (dx, dy), [n] samples [step] pt apart, [ms] apart, pressure 0.5. */
    private fun line(dx: Float, dy: Float, n: Int = 40, step: Float = 1f, ms: Float = 8f): FloatArray {
        val s = FloatArray(n * 4)
        for (i in 0 until n) { s[i * 4] = dx * step * i; s[i * 4 + 1] = dy * step * i; s[i * 4 + 2] = 0.5f; s[i * 4 + 3] = i * ms }
        return s
    }
    private fun p(s: FloatArray, i: Int) = s[i * 4 + 2]

    @Test
    fun startsAndLiftsOffThin() {
        val s = line(1f, 0f)
        Fountain.apply(s, 40, Fountain.Options(0.65f, 0f, "round", true))
        assertTrue("start thin: ${p(s, 0)} < ${p(s, 20)}", p(s, 0) < p(s, 20) * 0.6f)
        assertTrue("end thin: ${p(s, 39)} < ${p(s, 20)}", p(s, 39) < p(s, 20) * 0.6f)
        assertTrue("middle untouched: ${p(s, 20)}", p(s, 20) > 0.45f)
    }

    @Test
    fun aQuickStrokeIsThinner() {
        val slow = line(1f, 0f, step = 0.5f, ms = 8f)
        val fast = line(1f, 0f, step = 10f, ms = 8f)
        val o = Fountain.Options(0.65f, 1f, "round", false)
        Fountain.apply(slow, 40, o); Fountain.apply(fast, 40, o)
        assertTrue("fast ${p(fast, 30)} < slow ${p(slow, 30)}", p(fast, 30) < p(slow, 30) * 0.75f)
    }

    @Test
    fun aBroadNibFollowsTheDirection() {
        val o = Fountain.Options(0.65f, 0f, "flat", false)
        // The broad nib sits at -30°: a stroke along it is thin, across it wide.
        val along = line(kotlin.math.cos(Math.toRadians(-30.0)).toFloat(), kotlin.math.sin(Math.toRadians(-30.0)).toFloat())
        val across = line(kotlin.math.cos(Math.toRadians(60.0)).toFloat(), kotlin.math.sin(Math.toRadians(60.0)).toFloat())
        Fountain.apply(along, 40, o); Fountain.apply(across, 40, o)
        assertTrue("along ${p(along, 30)} thin, across ${p(across, 30)} wide", p(along, 30) < p(across, 30) * 0.4f)
        assertTrue("a shaped nib uses the full pressure range", Fountain.thinning(o) >= 0.9f)
    }
}
