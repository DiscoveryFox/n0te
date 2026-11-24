package dev.n0bridge.n0te

import kotlin.math.abs
import kotlin.math.atan2
import kotlin.math.hypot
import kotlin.math.sin

/**
 * The fountain pen's character, baked into the samples' pressure as they are written (the
 * core turns pressure into width), so the page, the laptop and PDF exports all draw the same
 * stroke without a new stroke format:
 *  - speed: a quick stroke lays down less ink (thinner),
 *  - nib: round, oblique (45°) or broad/italic (30°) — the width follows the direction,
 *  - taper: strokes start and lift off thin,
 *  - sensitivity: how much pressure changes the width (the brush's thinning).
 * Pure functions on the sample array (x, y, pressure, t per sample, page points): JVM-testable.
 */
object Fountain {
    /** Options, kept in [Tools]. */
    data class Options(val sensitivity: Float, val speed: Float, val nib: String, val taper: Boolean)

    fun options() = Options(Tools.fountainSensitivity, Tools.fountainSpeed, Tools.fountainNib, Tools.fountainTaper)

    /** The brush's thinning: a shaped nib needs the full range of pressure to show its angle. */
    fun thinning(o: Options = options()) = if (o.nib == "round") o.sensitivity else maxOf(o.sensitivity, 0.9f)

    /** Running state of one stroke. */
    class State {
        var n = 0
        var lastX = 0f; var lastY = 0f; var lastT = 0f
        var length = 0f
        var speed = 0f
        var dirX = 1f; var dirY = 0f
    }

    private const val TAPER_START = 5f
    private const val TAPER_END = 7f

    private fun smooth(e0: Float, e1: Float, x: Float): Float {
        val t = ((x - e0) / (e1 - e0)).coerceIn(0f, 1f)
        return t * t * (3 - 2 * t)
    }

    /** The pressure to store for a new sample (start taper, speed, nib). */
    fun pressure(st: State, o: Options, x: Float, y: Float, p: Float, t: Float): Float {
        var f = 1f
        if (st.n > 0) {
            val dx = x - st.lastX; val dy = y - st.lastY
            val d = hypot(dx, dy)
            st.length += d
            val dt = (t - st.lastT).coerceAtLeast(1f)
            st.speed = st.speed * 0.7f + (d / dt) * 0.3f
            if (d > 0.05f) { st.dirX = st.dirX * 0.6f + dx / d * 0.4f; st.dirY = st.dirY * 0.6f + dy / d * 0.4f }
        }
        st.n++
        st.lastX = x; st.lastY = y; st.lastT = t
        // Speed (pt/ms): from a calm 0.15 to a quick 1.2 the line thins by up to 45 %.
        f *= 1f - o.speed * 0.45f * ((st.speed - 0.15f) / 1.05f).coerceIn(0f, 1f)
        // Nib: widest across the nib's edge, thinnest along it.
        if (o.nib != "round") {
            val (angle, thin) = if (o.nib == "flat") Math.toRadians(-30.0) to 0.22f else Math.toRadians(-45.0) to 0.45f
            val dir = atan2(st.dirY.toDouble(), st.dirX.toDouble())
            f *= thin + (1f - thin) * abs(sin(dir - angle)).toFloat()
        }
        if (o.taper) f *= 0.45f + 0.55f * smooth(0f, TAPER_START, st.length)
        return (p * f).coerceIn(0.03f, 1f)
    }

    /** At pen-up: the stroke lifts off thin over its last few points. */
    fun taperEnd(samples: FloatArray, n: Int, o: Options) {
        if (!o.taper || n < 3) return
        var len = 0f
        for (i in n - 1 downTo 1) {
            val k = 0.4f + 0.6f * smooth(0f, TAPER_END, len)
            if (k >= 0.999f) break
            samples[i * 4 + 2] = (samples[i * 4 + 2] * k).coerceAtLeast(0.03f)
            len += hypot(samples[i * 4] - samples[(i - 1) * 4], samples[i * 4 + 1] - samples[(i - 1) * 4 + 1])
        }
    }

    /** A whole stroke at once (the pen menu's preview). */
    fun apply(samples: FloatArray, n: Int, o: Options = options()) {
        val st = State()
        for (i in 0 until n) samples[i * 4 + 2] = pressure(st, o, samples[i * 4], samples[i * 4 + 1], samples[i * 4 + 2], samples[i * 4 + 3])
        taperEnd(samples, n, o)
    }
}
