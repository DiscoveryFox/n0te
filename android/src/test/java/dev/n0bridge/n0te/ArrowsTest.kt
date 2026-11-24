package dev.n0bridge.n0te

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import kotlin.math.abs
import kotlin.math.atan2
import kotlin.math.hypot

/** Arrows drawn by hand: a line and a head at its end, or one stroke (held, or with both barbs as it lifts). */
class ArrowsTest {
    private val rnd = java.util.Random(3)
    private fun jitter(a: Float) = (rnd.nextFloat() - 0.5f) * 2 * a

    /** A path through [pts], a sample every ~2 pt with a little wobble, the pressure swelling in the middle. */
    private fun drawn(vararg pts: Pair<Float, Float>, wobble: Float = 0.6f): FloatArray {
        val out = ArrayList<Float>()
        for (i in 0 until pts.size - 1) {
            val (ax, ay) = pts[i]; val (bx, by) = pts[i + 1]
            val steps = maxOf(1, (hypot(bx - ax, by - ay) / 2f).toInt())
            for (k in 0 until steps) { out += ax + (bx - ax) * k / steps + jitter(wobble); out += ay + (by - ay) * k / steps + jitter(wobble); out += 0f; out += 0f }
        }
        out += pts.last().first; out += pts.last().second; out += 0f; out += 0f
        val n = out.size / 4
        return FloatArray(out.size) { out[it] }.also { a -> for (i in 0 until n) { a[i * 4 + 2] = 0.3f + 0.5f * kotlin.math.sin(Math.PI * i / n).toFloat(); a[i * 4 + 3] = i * 6f } }
    }

    private fun kind(s: FloatArray) = Shapes.detect(s, s.size / 4)?.kind
    private fun pts(o: FloatArray) = (0 until o.size / 4).map { o[it * 4] to o[it * 4 + 1] }

    /**
     * A stroke's end and the way back into it, as the core hands it out (FfiStroke.ends): a
     * straight stroke runs its chord's way, a curved one the main axis of its last fifth
     * (8–30 pt in); a flick at the very end (its last 4 pt off by more than 60° from the 4–12 pt
     * before) is left out. x, y, dx, dy, length.
     */
    private fun end(s: FloatArray, last: Boolean = true): FloatArray {
        val n = s.size / 4
        val order = if (last) (n - 1 downTo 0).toList() else (0 until n).toList()
        val len = (1 until n).sumOf { hypot(s[it * 4] - s[(it - 1) * 4], s[it * 4 + 1] - s[(it - 1) * 4 + 1]).toDouble() }.toFloat()
        fun along(d: Float): FloatArray {
            var acc = 0f
            for (k in 0 until order.size - 1) {
                val i = order[k]; val j = order[k + 1]
                val sl = hypot(s[j * 4] - s[i * 4], s[j * 4 + 1] - s[i * 4 + 1])
                if (acc + sl >= d && sl > 0f) { val f = (d - acc) / sl; return floatArrayOf(s[i * 4] + (s[j * 4] - s[i * 4]) * f, s[i * 4 + 1] + (s[j * 4 + 1] - s[i * 4 + 1]) * f) }
                acc += sl
            }
            val i = order.last(); return floatArrayOf(s[i * 4], s[i * 4 + 1])
        }
        fun unit(x: Float, y: Float): FloatArray { val l = hypot(x, y).coerceAtLeast(1e-6f); return floatArrayOf(x / l, y / l) }
        val ax = s[0]; val ay = s[1]; val bx = s[(n - 1) * 4]; val by = s[(n - 1) * 4 + 1]
        val chord = hypot(bx - ax, by - ay)
        val cu = unit(bx - ax, by - ay)
        val straight = chord > len * 0.9f && (0 until n).all { abs((s[it * 4] - ax) * cu[1] - (s[it * 4 + 1] - ay) * cu[0]) <= maxOf(len * 0.04f, 1.5f) }
        val near = minOf(1.5f, len / 8f); val far = minOf((len * 0.2f).coerceIn(8f, 30f), len / 2f)
        fun fitted(): FloatArray {
            val q = ArrayList<FloatArray>(); q += along(near); q += along(far)
            var acc = 0f
            for (k in 0 until order.size - 1) {
                val i = order[k]; val j = order[k + 1]
                acc += hypot(s[j * 4] - s[i * 4], s[j * 4 + 1] - s[i * 4 + 1])
                if (acc > far) break
                if (acc > near) q += floatArrayOf(s[j * 4], s[j * 4 + 1])
            }
            val mx = q.map { it[0] }.average().toFloat(); val my = q.map { it[1] }.average().toFloat()
            var cxx = 0f; var cxy = 0f; var cyy = 0f
            for (p in q) { cxx += (p[0] - mx) * (p[0] - mx); cxy += (p[0] - mx) * (p[1] - my); cyy += (p[1] - my) * (p[1] - my) }
            val t = 0.5f * atan2(2f * cxy, cxx - cyy)
            val dx = kotlin.math.cos(t); val dy = kotlin.math.sin(t)
            val fx = q[1][0] - q[0][0]; val fy = q[1][1] - q[0][1]
            return if (dx * fx + dy * fy < 0f) floatArrayOf(-dx, -dy) else floatArrayOf(dx, dy)
        }
        val tip = order.first()
        val p = along(minOf(4f, len / 6f)); val q = along(minOf(12f, len / 2f))
        val fl = unit(p[0] - s[tip * 4], p[1] - s[tip * 4 + 1]); val g = unit(q[0] - p[0], q[1] - p[1])
        val flick = len > 20f && fl[0] * g[0] + fl[1] * g[1] < 0.5f
        val d = if (straight && !flick) (if (last) unit(ax - bx, ay - by) else unit(bx - ax, by - ay)) else fitted()
        return if (flick) floatArrayOf(p[0], p[1], d[0], d[1], len) else floatArrayOf(s[tip * 4], s[tip * 4 + 1], d[0], d[1], len)
    }

    /** A closed triangle drawn turned by [deg] from pointing along +x, its middle at (cx, cy), [r] to its corners. */
    private fun triangle(cx: Float, cy: Float, r: Float, deg: Double): FloatArray {
        val corners = (0..3).map { k -> val t = Math.toRadians(deg + 120.0 * (k % 3)); (cx + r * kotlin.math.cos(t).toFloat()) to (cy + r * kotlin.math.sin(t).toFloat()) }
        return drawn(*corners.toTypedArray(), wobble = 0.3f)
    }

    @Test fun aTriangleAtALineEndPointsAlongTheLineHoweverItWasTurned() {
        val line = drawn(100f to 200f, 300f to 202f, wobble = 0.3f)
        val e = end(line)
        for (deg in listOf(0.0, 35.0, -50.0, 90.0, 180.0)) {
            val tri = triangle(e[0] - 4f, e[1] + 1f, 9f, deg)
            val head = Arrows.closedHead(tri, 0, tri.size / 4 - 1, e[0], e[1], e[2], e[3], e[4])
            assertNotNull("turned $deg°", head)
            val p = pts(head!!)
            // The point: foremost along the line, at its end; the two other corners mirror each other about the line.
            val tip = p.maxBy { it.first * -e[2] + it.second * -e[3] }
            assertEquals(e[0], tip.first, 0.6f); assertEquals(e[1], tip.second, 0.6f)
            val back = p.filter { (it.first - tip.first) * e[2] + (it.second - tip.second) * e[3] > 5f }
            val side = back.map { (it.first - tip.first) * -e[3] + (it.second - tip.second) * e[2] }
            assertEquals("symmetric about the line ($deg°)", 0f, side.max() + side.min(), 1.2f)
        }
        // A circle at the end stays a circle; a triangle far back along the line is no head.
        val circle = (0..30).map { val t = 2 * Math.PI * it / 30; (e[0] - 6f + 7f * kotlin.math.cos(t).toFloat()) to (e[1] + 7f * kotlin.math.sin(t).toFloat()) }
        val c = drawn(*circle.toTypedArray(), wobble = 0.2f)
        assertNull("o", Arrows.closedHead(c, 0, c.size / 4 - 1, e[0], e[1], e[2], e[3], e[4]))
        val far = triangle(e[0] - 40f, e[1], 9f, 30.0)
        assertNull("behind", Arrows.closedHead(far, 0, far.size / 4 - 1, e[0], e[1], e[2], e[3], e[4]))
    }

    @Test fun aWobbleAsThePenRestsDoesNotTurnTheHead() {
        // A straight line whose last few points dip off 30° (the pen landing): the head still lies along the line.
        val pts = (0..95).map { (100f + it * 2f) to 200f } + (1..4).map { (290f + it * 1.7f) to (200f + it * 1f) }
        val line = drawn(*pts.toTypedArray(), wobble = 0.1f)
        val e = end(line)
        assertEquals("along the line, not the dip", 0f, e[3], 0.06f)
    }

    /** Why the last [headOn] said no (from Arrows.trace). */
    private var why = ""
    private fun headOn(line: FloatArray, h: FloatArray, last: Boolean = true): FloatArray? {
        val e = end(line, last)
        val log = StringBuilder()
        Arrows.trace = { log.append(it).append("; ") }
        return Arrows.head(h, 0, h.size / 4 - 1, e[0], e[1], e[2], e[3], e[4]).also { Arrows.trace = null; why = log.toString() }
    }

    /** Strokes from the tablet (x, y, pressure), as a user drew them. */
    private val user = mapOf(
        "1224" to "266.0,772.0,0.09 266.1,771.9,0.23 266.3,771.7,0.23 266.7,771.5,0.23 267.3,771.3,0.23 267.9,771.0,0.23 268.7,770.6,0.24 269.6,770.1,0.26 270.6,769.6,0.28 271.8,768.9,0.31 273.1,768.3,0.34 274.4,767.5,0.38 275.8,766.7,0.42 277.2,765.9,0.45 278.7,765.0,0.47 280.3,764.1,0.49 282.0,763.1,0.51 283.6,762.1,0.53 285.4,761.1,0.55 287.2,760.0,0.56 289.1,758.8,0.56 291.1,757.5,0.56 293.2,756.0,0.56 295.3,754.5,0.56 297.6,752.8,0.56 299.9,750.9,0.56 302.1,749.0,0.57 304.2,746.9,0.57 306.3,744.7,0.57 308.4,742.6,0.58 310.2,740.5,0.58 311.9,738.4,0.60 313.4,736.4,0.61 314.8,734.5,0.62 316.0,732.7,0.64 316.9,730.8,0.64 317.6,729.0,0.65 318.0,727.3,0.66 318.2,725.6,0.67 318.2,724.1,0.68 318.0,722.6,0.69 317.6,721.1,0.69 316.9,719.6,0.70 316.1,718.2,0.70 315.2,716.7,0.70 314.0,715.2,0.69 312.7,713.8,0.70 311.3,712.3,0.70 309.7,710.9,0.70 308.1,709.6,0.71 306.3,708.3,0.71 304.4,707.1,0.71 302.5,705.9,0.72 300.6,704.6,0.72 298.7,703.3,0.72 296.8,702.0,0.73 295.1,700.8,0.75 293.6,699.6,0.76 292.4,698.4,0.76 291.3,697.2,0.77 290.6,696.0,0.78 290.1,694.8,0.79 289.8,693.6,0.80 289.7,692.4,0.80 289.7,691.2,0.80 289.7,690.1,0.80 289.9,689.1,0.80 290.1,688.1,0.78 290.1,687.2,0.73 290.2,686.2,0.58 290.1,685.4,0.28",
        "1225" to "284.3,692.8,0.08 284.2,692.8,0.11 284.1,693.0,0.13 284.0,693.0,0.16 283.9,693.1,0.18 283.8,693.2,0.20 283.8,693.2,0.22 283.7,693.3,0.23 283.6,693.3,0.25 283.6,693.4,0.29 283.5,693.5,0.34 283.4,693.4,0.46 283.4,693.3,0.48 283.5,693.1,0.51 283.6,692.9,0.53 283.6,692.8,0.54 283.8,692.6,0.55 283.9,692.3,0.56 284.0,692.1,0.57 284.2,691.8,0.58 284.3,691.5,0.59 284.5,691.2,0.60 284.7,690.8,0.60 284.9,690.4,0.61 285.2,690.0,0.61 285.4,689.6,0.62 285.7,689.2,0.62 285.9,688.8,0.62 286.2,688.4,0.62 286.4,687.9,0.63 286.6,687.4,0.64 286.9,686.9,0.64 287.1,686.5,0.64 287.3,686.0,0.65 287.6,685.6,0.65 287.8,685.2,0.65 288.0,684.8,0.66 288.2,684.4,0.66 288.4,684.0,0.66 288.5,683.6,0.67 288.7,683.3,0.67 288.9,683.0,0.67 289.1,682.7,0.67 289.2,682.4,0.67 289.3,682.2,0.67 289.4,681.9,0.68 289.6,681.7,0.68 289.7,681.5,0.68 289.7,681.4,0.68 289.8,681.2,0.69 289.9,681.1,0.69 290.0,681.0,0.70 290.1,681.0,0.70 290.2,680.9,0.71 290.2,680.9,0.71 290.4,680.8,0.72 290.5,680.8,0.73 290.7,680.8,0.74 290.8,680.9,0.75 291.0,681.0,0.76 291.3,681.2,0.77 291.5,681.4,0.78 291.8,681.6,0.79 292.1,682.0,0.80 292.4,682.4,0.81 292.8,682.8,0.81 293.1,683.3,0.82 293.5,683.8,0.83 293.9,684.3,0.84 294.3,684.8,0.85 294.7,685.4,0.85 295.1,685.9,0.86 295.4,686.4,0.86 295.8,687.0,0.87 296.2,687.6,0.87 296.7,688.1,0.88 297.1,688.6,0.88 297.5,689.1,0.88 297.9,689.5,0.88 298.3,689.9,0.89 298.6,690.2,0.89 299.0,690.5,0.90 299.3,690.8,0.90 299.6,691.0,0.90 299.9,691.2,0.90 300.2,691.4,0.89 300.4,691.5,0.87 300.6,691.5,0.82 300.7,691.6,0.74 300.8,691.6,0.59 301.0,691.7,0.38",
        "1226" to "286.3,696.3,0.05 286.2,696.4,0.06 286.0,696.6,0.09 285.8,696.7,0.11 285.7,696.8,0.14 285.6,696.9,0.19 285.5,696.9,0.44 285.6,696.7,0.51 285.7,696.5,0.56 285.8,696.3,0.60 285.9,695.9,0.63 286.1,695.7,0.65 286.3,695.3,0.66 286.5,694.8,0.68 286.8,694.2,0.68 287.1,693.6,0.69 287.4,692.9,0.69 287.8,692.2,0.70 288.2,691.4,0.70 288.6,690.5,0.70 289.1,689.5,0.69 289.5,688.6,0.69 289.9,687.7,0.69 290.3,686.8,0.69 290.7,686.1,0.69 290.9,685.4,0.70 291.1,684.9,0.70 291.2,684.5,0.70 291.2,684.2,0.71 291.2,684.2,0.72 291.1,684.3,0.72 290.9,684.4,0.72 290.8,684.6,0.72 290.8,684.9,0.72 290.7,685.1,0.72 290.7,685.5,0.72 290.7,685.8,0.72 290.8,686.0,0.72 290.9,686.2,0.72 291.0,686.5,0.72 291.2,686.8,0.72 291.4,687.0,0.72 291.7,687.2,0.72 292.0,687.5,0.72 292.3,687.8,0.72 292.7,688.1,0.72 293.1,688.3,0.72 293.5,688.6,0.73 293.9,688.8,0.73 294.3,689.1,0.73 294.7,689.4,0.72 295.1,689.7,0.72 295.6,690.0,0.72 296.0,690.3,0.73 296.4,690.6,0.73 296.8,690.9,0.73 297.2,691.2,0.73 297.6,691.5,0.73 297.9,691.8,0.74 298.2,692.1,0.74 298.5,692.3,0.74 298.7,692.5,0.74 298.9,692.8,0.74 299.1,692.9,0.74 299.2,693.1,0.75 299.2,693.3,0.78 299.0,693.4,0.80 298.8,693.5,0.82 298.6,693.6,0.83 298.4,693.6,0.83 298.2,693.6,0.82 297.9,693.6,0.71 297.7,693.6,0.31",
        "1227" to "289.1,686.9,0.05 289.1,686.7,0.05 289.1,686.4,0.07 289.1,686.0,0.12 289.2,685.9,0.19 289.3,685.8,0.29 289.4,685.7,0.42 289.4,685.9,0.56 289.3,686.2,0.60 289.0,686.6,0.63 288.7,687.1,0.65 288.4,687.8,0.67 287.9,688.7,0.68 287.4,689.6,0.69 286.9,690.5,0.69 286.4,691.5,0.70 285.9,692.4,0.71 285.4,693.3,0.71 285.0,694.1,0.71 284.7,694.9,0.71 284.4,695.6,0.72 284.2,696.1,0.72 284.2,696.6,0.72 284.2,697.0,0.72 284.2,697.4,0.72 284.4,697.7,0.72 284.7,698.0,0.71 285.1,698.2,0.71 285.6,698.3,0.71 286.2,698.4,0.71 286.8,698.4,0.71 287.5,698.5,0.71 288.2,698.5,0.71 288.9,698.4,0.71 289.6,698.3,0.70 290.4,698.2,0.70 291.1,698.1,0.70 291.8,698.0,0.70 292.6,697.8,0.70 293.3,697.6,0.70 294.0,697.3,0.70 294.7,697.1,0.70 295.2,696.8,0.70 295.8,696.7,0.70 296.2,696.5,0.70 296.7,696.3,0.71 297.0,696.3,0.71 297.3,696.2,0.71 297.5,696.1,0.71 297.6,696.0,0.71 297.7,695.9,0.71 297.7,695.7,0.71 297.6,695.4,0.72 297.4,695.2,0.72 297.3,695.0,0.72 297.0,694.6,0.72 296.7,694.3,0.73 296.4,693.9,0.73 295.9,693.5,0.73 295.5,693.0,0.74 295.0,692.5,0.74 294.5,692.0,0.74 294.0,691.5,0.75 293.5,690.9,0.75 292.9,690.3,0.76 292.4,689.6,0.76 291.9,689.0,0.76 291.5,688.4,0.76 291.1,687.8,0.76 290.8,687.2,0.76 290.4,686.8,0.77 290.3,686.4,0.77 290.1,686.1,0.76 289.9,686.1,0.76 289.8,686.0,0.74 289.6,686.1,0.61 289.5,686.4,0.42 289.4,686.7,0.20",
        "1228" to "282.7,767.5,0.07 282.6,767.5,0.12 282.5,767.6,0.16 282.4,767.6,0.18 282.4,767.7,0.21 282.3,767.7,0.23 282.3,767.8,0.25 282.2,767.8,0.26 282.2,767.9,0.28 282.2,767.9,0.29 282.1,768.0,0.32 282.1,768.1,0.36 282.2,768.0,0.42 282.3,767.9,0.43 282.4,767.8,0.44 282.6,767.6,0.45 282.7,767.5,0.46 282.9,767.2,0.47 283.2,767.0,0.49 283.4,766.8,0.50 283.7,766.5,0.51 284.0,766.1,0.53 284.3,765.8,0.54 284.7,765.4,0.55 285.0,764.9,0.56 285.4,764.5,0.57 285.8,764.0,0.58 286.3,763.5,0.59 286.8,763.0,0.60 287.3,762.4,0.60 287.9,761.7,0.61 288.5,761.0,0.62 289.1,760.3,0.63 289.8,759.5,0.60 290.5,758.7,0.62 291.3,757.9,0.63 292.1,757.0,0.63 293.0,756.0,0.60 294.0,755.1,0.61 295.0,754.1,0.61 296.1,753.0,0.61 297.2,752.0,0.61 298.4,750.9,0.59 299.7,749.7,0.59 301.0,748.6,0.59 302.3,747.4,0.58 303.7,746.2,0.58 305.1,745.0,0.57 306.5,743.7,0.56 307.9,742.3,0.56 309.2,741.0,0.56 310.6,739.6,0.56 312.1,738.1,0.55 313.6,736.7,0.56 315.1,735.2,0.56 316.6,733.7,0.55 318.2,732.2,0.56 319.8,730.8,0.57 321.3,729.4,0.58 322.9,728.0,0.59 324.4,726.8,0.58 326.0,725.5,0.60 327.4,724.2,0.61 328.9,723.1,0.62 330.4,721.9,0.62 331.7,720.9,0.63 333.1,719.9,0.64 334.3,718.9,0.65 335.6,718.1,0.66 336.7,717.2,0.67 337.8,716.5,0.66 338.9,715.8,0.68 339.8,715.2,0.69 340.7,714.6,0.69 341.6,714.1,0.70 342.3,713.6,0.70 343.0,713.1,0.71 343.6,712.7,0.71 344.2,712.3,0.70 344.7,712.0,0.70 345.1,711.7,0.68 345.5,711.4,0.64 345.8,711.1,0.59 346.1,710.9,0.52 346.3,710.6,0.41 346.5,710.4,0.29 346.6,710.1,0.20",
        "b182" to "374.4,222.4,0.11 374.2,222.2,0.12 373.9,222.0,0.16 373.6,221.9,0.18 373.5,221.6,0.20 373.3,221.6,0.22 372.8,221.4,0.28 372.7,221.4,0.30 372.4,221.3,0.33 371.9,221.3,0.37 371.5,221.3,0.42 371.0,221.3,0.46 370.9,221.4,0.48 370.6,221.4,0.50 370.5,221.6,0.52 370.2,221.8,0.54 370.1,221.9,0.55 369.8,222.3,0.57 369.7,222.5,0.59 369.6,222.9,0.60 369.3,223.3,0.62 369.3,223.8,0.63 369.2,224.3,0.64 369.1,224.9,0.66 369.0,225.7,0.64 368.9,226.4,0.66 368.8,227.3,0.67 368.8,228.2,0.68 368.8,229.2,0.68 368.8,230.3,0.68 368.8,231.4,0.69 368.8,232.6,0.69 368.9,233.8,0.69 369.0,235.0,0.69 369.3,236.3,0.69 369.4,237.7,0.69 369.8,239.1,0.70 370.1,240.6,0.68 370.4,242.1,0.68 370.7,243.6,0.68 371.1,245.1,0.68 371.6,246.6,0.67 372.0,248.1,0.68 372.6,249.6,0.68 373.2,251.0,0.67 373.9,252.4,0.67 374.7,253.6,0.66 375.6,254.8,0.67 376.6,255.8,0.67 377.7,256.6,0.68 378.8,257.3,0.68 380.0,257.9,0.67 381.4,258.4,0.67 382.9,258.7,0.68 384.5,258.8,0.66 386.2,258.8,0.67 387.9,258.8,0.66 389.7,258.5,0.66 391.6,258.2,0.66 393.4,257.7,0.67 395.3,257.2,0.67 397.2,256.5,0.67 399.0,255.8,0.65 400.9,255.0,0.66 402.7,254.2,0.66 404.5,253.2,0.65 406.2,252.2,0.66 407.9,251.2,0.66 409.6,250.1,0.65 411.2,248.9,0.66 412.7,247.6,0.67 414.2,246.3,0.65 415.7,244.9,0.66 417.1,243.4,0.64 418.6,241.9,0.65 419.9,240.2,0.65 421.2,238.6,0.66 422.5,236.8,0.66 423.6,234.9,0.64 424.8,233.0,0.65 425.9,231.1,0.65 426.9,228.9,0.65 427.9,226.8,0.64 428.8,224.6,0.64 429.6,222.3,0.66 430.4,220.0,0.64 431.1,217.5,0.62 431.8,215.1,0.64 432.4,212.5,0.64 433.0,210.0,0.62 433.5,207.4,0.61 434.0,204.8,0.63 434.5,202.2,0.62 434.9,199.6,0.63 435.3,196.9,0.64 435.6,194.5,0.62 435.8,192.1,0.63 435.9,189.9,0.64 436.3,187.8,0.66 436.4,185.9,0.66 436.4,184.3,0.66 436.5,182.8,0.68 436.5,181.6,0.69 436.5,180.6,0.70 436.4,179.7,0.71 436.1,179.0,0.71 435.8,178.4,0.72 435.4,178.0,0.73 434.9,177.7,0.73 434.5,177.6,0.72 433.9,177.3,0.71 433.3,177.3,0.69 432.7,177.3,0.66 432.0,177.3,0.60 431.3,177.6,0.53 430.6,177.7,0.43 429.9,178.1,0.31 429.3,178.3,0.20",
        "b183" to "426.8,186.3,0.05 426.6,186.4,0.07 426.4,186.4,0.09 426.3,186.4,0.12 426.1,186.5,0.17 425.9,186.5,0.28 426.1,186.3,0.44 426.3,186.0,0.46 426.8,185.6,0.51 427.1,185.3,0.53 427.5,185.0,0.55 428.0,184.5,0.57 428.5,184.1,0.58 429.1,183.5,0.59 429.7,182.9,0.60 430.4,182.2,0.62 431.2,181.4,0.62 432.0,180.6,0.62 432.8,179.8,0.63 433.7,178.9,0.63 434.6,178.1,0.63 435.4,177.2,0.64 436.2,176.2,0.63 437.0,175.2,0.63 437.8,174.3,0.64 438.6,173.4,0.64 439.3,172.6,0.64 440.0,171.8,0.64 440.6,171.1,0.64 441.1,170.4,0.65 441.6,169.7,0.66 442.1,169.1,0.66 442.4,168.6,0.66 442.8,168.2,0.67 442.9,167.9,0.67 443.2,167.5,0.67 443.3,167.2,0.66 442.9,167.2,0.45 442.7,167.3,0.35 442.3,167.7,0.25 441.8,168.0,0.17",
        "b189" to "412.1,252.7,0.16 412.0,252.9,0.17 411.9,253.2,0.19 411.6,253.3,0.21 411.6,253.7,0.23 411.4,254.1,0.24 411.4,254.5,0.25 411.5,255.0,0.25 411.8,255.1,0.25 411.9,255.5,0.25 412.2,255.8,0.25 412.5,255.9,0.25 412.9,256.2,0.25 413.2,256.5,0.26 413.7,256.8,0.26 414.2,257.1,0.27 414.7,257.4,0.29 415.2,257.7,0.30 415.9,258.0,0.32 416.6,258.3,0.34 417.3,258.6,0.35 418.1,258.9,0.37 419.0,259.0,0.39 419.9,259.3,0.40 420.8,259.4,0.42 421.7,259.6,0.43 422.7,259.6,0.44 423.7,259.8,0.46 424.7,259.8,0.46 425.8,259.8,0.47 427.0,259.7,0.47 428.1,259.3,0.48 429.2,258.9,0.49 430.4,258.5,0.49 431.5,257.9,0.50 432.7,257.2,0.51 433.9,256.5,0.50 435.1,255.7,0.51 436.2,254.7,0.51 437.4,253.7,0.47 438.5,252.7,0.49 439.6,251.6,0.50 440.7,250.4,0.50 441.8,249.2,0.51 442.8,247.9,0.51 443.7,246.6,0.51 444.6,245.3,0.51 445.5,244.0,0.51 446.3,242.7,0.51 447.1,241.2,0.52 447.8,239.8,0.52 448.5,238.3,0.53 449.1,236.8,0.52 449.6,235.3,0.52 450.1,233.8,0.53 450.5,232.3,0.53 450.8,230.8,0.53 451.1,229.2,0.52 451.2,227.6,0.53 451.5,226.1,0.53 451.5,224.6,0.54 451.5,223.0,0.53 451.5,221.5,0.54 451.4,220.0,0.54 451.1,218.4,0.54 450.8,216.9,0.54 450.5,215.4,0.54 450.1,213.9,0.55 449.6,212.5,0.55 449.1,211.2,0.55 448.4,209.9,0.54 447.8,208.7,0.55 447.1,207.8,0.55 446.3,206.9,0.56 445.6,206.1,0.56 444.7,205.6,0.56 443.9,205.1,0.57 443.0,204.8,0.58 442.0,204.8,0.58 441.0,204.8,0.57 439.9,204.9,0.58 438.8,205.4,0.57 437.7,205.9,0.57 436.5,206.5,0.57 435.4,207.3,0.57 434.3,208.2,0.55 433.1,209.2,0.55 432.0,210.3,0.54 430.8,211.4,0.54 429.7,212.6,0.54 428.7,213.8,0.54 427.7,215.0,0.54 426.7,216.3,0.54 425.8,217.5,0.53 425.1,218.7,0.53 424.4,219.7,0.53 423.8,220.7,0.53 423.3,221.5,0.51 422.9,222.3,0.50 422.7,222.9,0.47 422.6,223.4,0.42 422.6,223.9,0.35 422.6,224.4,0.26 422.6,224.7,0.18",
        "b190" to "422.2,224.0,0.05 422.1,224.1,0.05 421.9,224.2,0.06 421.8,224.2,0.07 421.6,224.3,0.09 421.6,224.2,0.17 421.6,223.9,0.20 421.6,223.7,0.22 421.6,223.4,0.25 421.6,222.9,0.28 421.6,222.4,0.31 421.6,222.2,0.33 421.6,221.9,0.34 421.6,221.6,0.36 421.6,221.2,0.37 421.6,220.8,0.39 421.6,220.3,0.40 421.6,219.8,0.41 421.6,219.3,0.42 421.7,218.7,0.43 421.7,218.1,0.44 421.8,217.4,0.44 421.8,216.7,0.45 421.9,216.0,0.44 422.0,215.3,0.46 422.1,214.6,0.46 422.1,213.9,0.47 422.2,213.1,0.47 422.3,212.4,0.48 422.3,211.6,0.48 422.4,210.8,0.48 422.5,210.1,0.49 422.6,209.2,0.48 422.6,208.5,0.48 422.7,207.7,0.49 422.7,207.0,0.49 422.8,206.3,0.49 422.8,205.7,0.50 422.9,205.1,0.50 423.0,204.5,0.50 423.1,204.0,0.50 423.1,203.6,0.51 423.2,203.2,0.51 423.2,202.9,0.52 423.2,202.6,0.53 423.3,202.4,0.54 423.3,202.6,0.55 423.3,202.9,0.55 423.3,203.2,0.55 423.3,203.6,0.55 423.3,204.1,0.55 423.3,204.7,0.55 423.3,205.3,0.55 423.2,206.1,0.55 423.2,206.8,0.54 423.1,207.7,0.55 423.1,208.6,0.55 423.1,209.4,0.55 423.0,210.3,0.54 422.9,211.2,0.54 422.8,212.1,0.54 422.7,213.0,0.54 422.6,213.9,0.54 422.6,214.8,0.55 422.4,215.7,0.54 422.4,216.6,0.54 422.3,217.4,0.55 422.2,218.2,0.55 422.2,218.9,0.55 422.1,219.5,0.55 422.1,220.1,0.55 422.1,220.6,0.56 422.1,221.0,0.56 422.1,221.4,0.56 422.1,221.8,0.56 422.1,222.1,0.56 422.1,222.5,0.56 422.2,222.7,0.57 422.2,222.9,0.57 422.3,223.2,0.56 422.4,223.4,0.57 422.5,223.6,0.56 422.6,223.7,0.56 422.8,223.8,0.56 422.9,223.9,0.56 423.1,224.1,0.57 423.2,224.1,0.56 423.5,224.2,0.56 423.9,224.2,0.56 424.1,224.2,0.56 424.4,224.2,0.56 424.8,224.2,0.56 425.2,224.2,0.56 425.7,224.2,0.56 426.3,223.9,0.56 427.0,223.9,0.56 427.8,223.5,0.56 428.5,223.2,0.56 429.3,222.9,0.55 430.1,222.5,0.56 431.0,222.2,0.56 431.8,221.8,0.56 432.6,221.4,0.56 433.4,221.1,0.56 434.3,220.7,0.57 435.1,220.3,0.57 435.9,220.0,0.58 436.7,219.7,0.58 437.4,219.4,0.59 438.1,219.3,0.59 438.8,218.9,0.60 439.4,218.8,0.60 440.1,218.5,0.60 440.6,218.4,0.61 441.1,218.1,0.61 441.5,218.0,0.62 441.8,218.0,0.62 442.1,217.9,0.62 442.5,217.8,0.60 442.4,218.0,0.26",
        "b191" to "390.3,262.8,0.25 389.4,263.6,0.29 388.5,264.2,0.34 388.2,264.6,0.35 388.1,265.3,0.36 388.1,265.9,0.35 388.5,266.7,0.35 389.2,267.6,0.35 389.9,268.7,0.35 390.9,270.0,0.36 392.0,271.3,0.39 393.2,272.5,0.41 394.8,273.6,0.43 396.8,274.6,0.44 399.1,275.2,0.46 401.6,275.6,0.46 404.4,275.6,0.46 407.4,275.4,0.47 410.7,274.6,0.47 413.9,273.1,0.48 417.2,271.2,0.49 420.5,268.8,0.50 423.8,266.0,0.51 427.1,262.8,0.51 430.4,259.4,0.50 433.7,255.8,0.50 436.7,252.0,0.50 439.8,248.1,0.50 442.6,244.1,0.49 445.2,239.9,0.50 447.6,235.5,0.51 449.9,231.0,0.49 452.0,226.3,0.48 454.0,221.6,0.50 455.8,216.7,0.51 457.5,211.6,0.51 459.2,206.7,0.50 460.8,201.9,0.51 462.3,197.4,0.51 463.6,193.1,0.49 464.8,189.0,0.51 465.8,185.3,0.53 466.7,182.1,0.53 467.3,179.2,0.55 467.7,176.9,0.55 467.8,174.8,0.56 467.8,173.1,0.57 467.4,171.7,0.57 466.9,170.5,0.58 466.1,169.7,0.57 465.0,169.1,0.57 463.9,168.7,0.57 462.6,168.7,0.57 461.3,168.7,0.57 459.8,168.9,0.57 458.5,169.4,0.57 457.2,170.2,0.57 455.8,171.1,0.56 454.5,172.2,0.57 453.0,173.8,0.56 451.4,175.5,0.55 449.7,177.4,0.55 447.9,179.3,0.54 446.1,181.4,0.55 444.2,183.5,0.54 442.4,185.6,0.53 440.6,187.6,0.53 438.8,189.5,0.50 437.1,191.3,0.46 435.7,192.9,0.38 434.5,194.4,0.26 433.4,195.9,0.13 432.8,196.6,0.10",
        "c133" to "278.3,200.8,0.10 278.1,201.0,0.11 277.8,201.2,0.12 277.6,201.5,0.12 277.3,201.8,0.14 277.2,202.1,0.14 277.1,202.3,0.15 277.0,202.5,0.16 276.9,202.7,0.17 276.9,202.9,0.18 276.9,203.0,0.20 276.8,203.1,0.22 276.8,203.2,0.27 276.9,203.3,0.33 277.1,203.2,0.35 277.2,203.2,0.37 277.3,203.0,0.38 277.5,202.8,0.40 277.7,202.7,0.41 277.9,202.5,0.42 278.2,202.2,0.44 278.6,201.9,0.46 278.9,201.5,0.47 279.3,201.1,0.48 279.8,200.6,0.49 280.3,200.1,0.49 281.0,199.4,0.50 281.7,198.7,0.51 282.5,198.0,0.51 283.4,197.1,0.51 284.4,196.2,0.52 285.6,195.2,0.51 286.8,194.1,0.52 288.1,193.0,0.53 289.5,191.8,0.53 291.0,190.5,0.53 292.6,189.2,0.51 294.2,187.8,0.52 296.0,186.4,0.52 297.8,185.1,0.51 299.7,183.6,0.51 301.6,182.2,0.51 303.5,180.9,0.51 305.4,179.6,0.51 307.3,178.2,0.51 309.3,176.9,0.50 311.3,175.6,0.51 313.2,174.4,0.51 315.1,173.2,0.51 317.0,172.1,0.51 318.8,170.9,0.52 320.5,169.9,0.51 322.2,169.0,0.51 323.7,168.1,0.51 325.2,167.2,0.52 326.6,166.4,0.52 327.8,165.6,0.53 329.0,165.0,0.53 330.0,164.4,0.54 330.9,163.8,0.53 331.7,163.4,0.53 332.4,163.1,0.50 332.9,162.7,0.46 333.5,162.5,0.41 333.9,162.3,0.34 334.2,162.1,0.26 334.5,162.1,0.17",
        "c134" to "294.7,169.1,0.07 294.9,169.0,0.08 295.1,168.8,0.10 295.4,168.6,0.11 295.7,168.4,0.13 296.0,168.3,0.14 296.4,168.1,0.16 296.9,168.0,0.18 297.4,167.8,0.20 298.1,167.6,0.22 298.7,167.3,0.25 299.4,167.0,0.27 300.2,166.7,0.29 301.0,166.4,0.31 301.9,166.0,0.33 302.8,165.7,0.34 303.8,165.3,0.35 304.8,164.9,0.36 305.9,164.6,0.37 306.9,164.2,0.38 308.0,163.8,0.38 309.1,163.5,0.39 310.3,163.1,0.40 311.5,162.7,0.41 312.7,162.4,0.41 313.9,162.0,0.42 315.2,161.6,0.42 316.5,161.2,0.43 317.8,160.9,0.43 319.1,160.5,0.44 320.4,160.1,0.44 321.8,159.7,0.45 323.2,159.3,0.46 324.5,159.0,0.46 325.9,158.6,0.46 327.2,158.3,0.46 328.5,158.0,0.47 329.8,157.7,0.47 331.0,157.4,0.47 332.2,157.2,0.47 333.4,157.1,0.48 334.5,156.9,0.48 335.6,156.9,0.49 336.7,156.7,0.49 337.6,156.7,0.50 338.6,156.7,0.51 339.4,156.7,0.52 340.3,156.7,0.53 341.0,156.9,0.54 341.7,157.0,0.55 342.3,157.2,0.55 342.9,157.5,0.57 343.3,157.8,0.58 343.7,158.2,0.59 344.0,158.6,0.60 344.2,159.0,0.61 344.4,159.5,0.63 344.4,160.0,0.64 344.4,160.6,0.65 344.4,161.3,0.66 344.1,161.9,0.67 343.8,162.7,0.67 343.5,163.4,0.68 343.0,164.2,0.68 342.5,165.1,0.68 341.9,166.0,0.69 341.2,166.9,0.68 340.5,167.8,0.69 339.8,168.8,0.69 339.0,169.7,0.68 338.2,170.6,0.69 337.4,171.5,0.69 336.7,172.4,0.69 335.9,173.2,0.69 335.2,174.0,0.70 334.5,174.8,0.71 333.8,175.5,0.71 333.2,176.2,0.70 332.6,176.9,0.71 332.0,177.5,0.71 331.6,178.1,0.71 331.1,178.6,0.69 330.7,179.1,0.67 330.3,179.5,0.62 330.0,179.9,0.54 329.7,180.2,0.43 329.4,180.5,0.28",
        "1230" to "340.0,711.6,0.05 339.9,711.6,0.08 339.8,711.6,0.10 339.7,711.5,0.12 339.6,711.5,0.14 339.5,711.4,0.16 339.4,711.3,0.18 339.3,711.3,0.21 339.3,711.2,0.24 339.2,711.2,0.28 339.2,711.1,0.33 339.2,711.0,0.36 339.4,710.9,0.38 339.6,710.9,0.41 339.7,710.8,0.42 339.9,710.8,0.44 340.1,710.6,0.46 340.4,710.6,0.47 340.7,710.4,0.49 341.0,710.3,0.50 341.4,710.2,0.51 341.8,710.1,0.53 342.2,709.9,0.54 342.6,709.8,0.55 343.1,709.6,0.56 343.5,709.5,0.56 344.0,709.3,0.57 344.5,709.2,0.58 345.0,709.1,0.58 345.5,708.9,0.58 345.9,708.8,0.59 346.4,708.6,0.59 346.8,708.5,0.59 347.3,708.4,0.60 347.7,708.2,0.60 348.1,708.2,0.60 348.4,708.0,0.61 348.7,708.0,0.61 349.0,707.9,0.62 349.3,707.9,0.62 349.5,707.9,0.62 349.7,707.8,0.62 349.9,707.8,0.62 350.1,707.8,0.63 350.2,707.8,0.63 350.4,707.9,0.64 350.5,707.9,0.64 350.6,708.0,0.65 350.7,708.1,0.65 350.7,708.3,0.66 350.7,708.4,0.67 350.7,708.6,0.67 350.7,708.8,0.68 350.7,709.0,0.69 350.7,709.3,0.69 350.6,709.6,0.69 350.6,709.9,0.70 350.4,710.3,0.71 350.2,710.7,0.71 350.1,711.1,0.71 349.9,711.6,0.71 349.7,712.1,0.71 349.5,712.6,0.72 349.3,713.2,0.71 349.1,713.8,0.71 348.9,714.4,0.71 348.8,715.0,0.71 348.6,715.6,0.71 348.5,716.2,0.71 348.3,716.7,0.72 348.2,717.2,0.71 348.0,717.6,0.72 347.9,718.1,0.72 347.8,718.5,0.72 347.7,718.8,0.72 347.6,719.2,0.73 347.5,719.5,0.73 347.4,719.8,0.73 347.3,720.0,0.72 347.3,720.2,0.71 347.2,720.3,0.68 347.1,720.4,0.64 347.1,720.4,0.58 346.9,720.5,0.49 346.8,720.5,0.36 346.7,720.5,0.24",
    ).mapValues { (_, v) -> v.split(" ").map { it.split(",").map(String::toFloat) }.let { p -> FloatArray(p.size * 4) { i -> if (i % 4 == 3) (i / 4) * 4f else p[i / 4][i % 4] } } }

    @Test fun headsOnUsersLines() {
        // An S-curve, then a head at its top end: a caret with a hook, another, a small triangle.
        val curve = user.getValue("1224")
        for (h in listOf("1225", "1226", "1227")) { val r = headOn(curve, user.getValue(h)); assertNotNull("head $h on the curve: $why", r) }
        // A straight-ish line and a V with a hook where the pen landed.
        val hooked = headOn(user.getValue("1228"), user.getValue("1230"))
        assertNotNull("V with a hook: $why", hooked)
        // The triangle stays closed (a filled-looking head), its point on the curve's end.
        val tri = pts(headOn(curve, user.getValue("1227"))!!)
        assertEquals(tri.first().first, tri.last().first, 0.5f)
        val e = end(curve)
        assertTrue(tri.any { hypot(it.first - e[0], it.second - e[1]) < 0.5f })
    }

    /** Drawn by the core (which smooths every stroke), the head's point reaches the line's end: the line doesn't poke through it. */
    @Test fun theLineEndsInsideThePoint() {
        val brush = uniffi.n0te_ffi.FfiBrush(1f, 0.3f, 0xFF000000u, 0u)
        val line = drawn(100f to 200f, 300f to 140f, wobble = 0.2f)
        val e = end(line)
        val head = headOn(line, drawn(e[0] - 14f to e[1] - 2f, e[0] + 1f to e[1], e[0] - 6f to e[1] + 13f, wobble = 0.2f))!!
        val o = floatsOf(uniffi.n0te_ffi.strokeOutline(brush, bytesOf(head)))
        // How far the head's outline reaches forward past the line's end (along the line's way on).
        val reach = (0 until o.size / 2).maxOf { -((o[it * 2] - e[0]) * e[2] + (o[it * 2 + 1] - e[1]) * e[3]) }
        assertTrue("the point reaches $reach pt past the end (the line's round end reaches 0.5)", reach >= 0.4f)
    }

    /** A big, rounded head drawn a bit past a curve's end: tidied, it lies about the curve's way and isn't huge. */
    @Test fun aBigLeaningHeadComesOutAlongTheLine() {
        val line = user.getValue("c133"); val e = end(line)
        val h = headOn(line, user.getValue("c134"))
        assertNotNull("head: $why", h)
        val p = pts(h!!)
        val tip = p.minBy { hypot(it.first - e[0], it.second - e[1]) }
        val far = p.maxBy { hypot(it.first - tip.first, it.second - tip.second) }
        // The two barb ends: the farthest point on each side of the line's way.
        val side = { q: Pair<Float, Float> -> e[2] * (q.second - e[1]) - e[3] * (q.first - e[0]) }
        val left = p.filter { side(it) > 0f }.maxBy { hypot(it.first - e[0], it.second - e[1]) }
        val right = p.filter { side(it) < 0f }.maxBy { hypot(it.first - e[0], it.second - e[1]) }
        fun angle(q: Pair<Float, Float>): Double {
            val vx = q.first - e[0]; val vy = q.second - e[1]
            return Math.toDegrees(kotlin.math.acos(((vx * e[2] + vy * e[3]) / hypot(vx, vy)).toDouble()))
        }
        assertEquals("symmetric about the line", angle(left), angle(right), 6.0)
        assertTrue("not huge: ${hypot(far.first - tip.first, far.second - tip.second)} on a line of ${e[4]}", hypot(far.first - tip.first, far.second - tip.second) <= e[4] * 0.31f)
    }

    @Test fun headsOnCurlingEnds() {
        // Curves that curl over at the top and end with a little flick, as drawn on the tablet:
        // their way at the end is the curve's, not the flick's.
        val e = end(user.getValue("b182"))
        assertTrue("182 ends going up: ${e.toList()}", e[3] > 0.6f) // back into it = downwards
        // A V right at the end of a curve that loops over (it was not taken before).
        val why = StringBuilder(); Arrows.trace = { why.append(it).append("; ") }
        val h = headOn(user.getValue("b189"), user.getValue("b190"))
        Arrows.trace = null
        assertNotNull("V on the looping curve: $why end=${end(user.getValue("b189")).toList()}", h)
    }

    @Test fun aHeadDrawnAsTwoBarbs() {
        val line = drawn(100f to 200f, 300f to 200f, wobble = 0.3f)
        val e = end(line)
        // One barb up-left from the tip, one down-left, drawn as separate strokes.
        val first = drawn(e[0] - 1f to e[1] + 1f, e[0] - 18f to e[1] - 12f, wobble = 0.2f)
        val second = drawn(e[0] + 1f to e[1], e[0] - 17f to e[1] + 13f, wobble = 0.2f)
        val b1 = Arrows.barb(first, 0, first.size / 4 - 1, e[0], e[1], e[2], e[3], e[4])
        val b2 = Arrows.barb(second, 0, second.size / 4 - 1, e[0], e[1], e[2], e[3], e[4])
        assertNotNull(b1); assertNotNull(b2)
        val head = Arrows.headFromBarbs(b1!!, b2!!, e[0], e[1], e[2], e[3], e[4], 0f, 100f)
        assertNotNull(head)
        val tip = pts(head!!).maxBy { it.first }
        assertEquals(e[0], tip.first, 0.5f)
        // Both on the same side: no head. A stroke pointing away from the line: no barb.
        assertNull(Arrows.headFromBarbs(b1, b1, e[0], e[1], e[2], e[3], e[4], 0f, 100f))
        val away = drawn(e[0] to e[1], e[0] + 18f to e[1] - 12f, wobble = 0.2f)
        assertNull(Arrows.barb(away, 0, away.size / 4 - 1, e[0], e[1], e[2], e[3], e[4]))
    }

    @Test fun oneStrokeArrows() {
        // Shaft, then out to one barb, back to the tip, out to the other.
        val both = drawn(100f to 200f, 300f to 200f, 276f to 186f, 300f to 200f, 278f to 216f)
        val a = Shapes.detect(both, both.size / 4)!!
        assertEquals("arrow", a.kind)
        val p = pts(a.samples)
        assertEquals(100f, p.first().first, 2f)
        assertTrue(p.takeWhile { it.first < 299f }.all { abs(it.second - p.first().second) < 1.5f })
        val tip = p.maxBy { it.first }
        assertEquals(300f, tip.first, 2f)
        val barbs = p.filter { hypot(it.first - tip.first, it.second - tip.second) > 20f && it.first > 250f }
        val up = barbs.minBy { it.second }; val down = barbs.maxBy { it.second }
        val angUp = Math.toDegrees(atan2((tip.second - up.second).toDouble(), (tip.first - up.first).toDouble()))
        val angDown = Math.toDegrees(atan2((down.second - tip.second).toDouble(), (tip.first - down.first).toDouble()))
        assertEquals(angUp, angDown, 4.0)
        val ps = (0 until a.samples.size / 4).map { a.samples[it * 4 + 2] }
        assertTrue("pressure varies: ${ps.min()}..${ps.max()}", ps.max() - ps.min() > 0.1f)
        // As the pen lifts (no hold) too, with both barbs.
        assertNotNull(Arrows.fromStroke(both, 0, both.size / 4 - 1, strict = true))
        // One barb only: held, the other comes as its mirror image; not as the pen lifts.
        val one = drawn(100f to 300f, 260f to 220f, 262f to 248f)
        assertEquals("arrow", kind(one))
        assertNull(Arrows.fromStroke(one, 0, one.size / 4 - 1, strict = true))
        // Drawn from the tip: barb first, then the shaft; both ends.
        assertEquals("arrow", kind(drawn(130f to 115f, 100f to 100f, 300f to 100f)))
        assertEquals("arrow", kind(drawn(125f to 85f, 100f to 100f, 125f to 115f, 100f to 100f, 300f to 100f, 275f to 85f, 300f to 100f, 275f to 115f)))
    }

    @Test fun aCurvedArrowInOneStrokeKeepsItsCurve() {
        // A quarter arc ending upwards, then a head (barbs back down along it).
        val arc = (0..40).map { val t = Math.PI / 2 * it / 40; (100f + 150f * kotlin.math.sin(t).toFloat()) to (300f - 150f * (1 - kotlin.math.cos(t)).toFloat()) }
        val (tx, ty) = arc.last()
        val s = drawn(*(arc + listOf(tx - 11f to ty + 17f, tx to ty, tx + 10f to ty + 18f)).toTypedArray(), wobble = 0.3f)
        val why = StringBuilder()
        Arrows.trace = { why.append(it).append("; ") }
        val out = Arrows.fromStroke(s, 0, s.size / 4 - 1, strict = true)
        Arrows.trace = null
        assertNotNull("curved arrow: $why", out)
        // The arc's middle is still where it was drawn.
        val (mx, my) = arc[20]
        assertTrue(pts(out!!).any { hypot(it.first - mx, it.second - my) < 2f })
    }

    @Test fun otherStrokesAreNoArrows() {
        assertEquals("an L", null, kind(drawn(100f to 100f, 100f to 300f, 250f to 300f)))
        assertEquals("a tick", null, kind(drawn(100f to 200f, 130f to 240f, 220f to 140f)))
        assertEquals("a zigzag", null, kind(drawn(100f to 100f, 150f to 150f, 200f to 100f, 250f to 150f, 300f to 100f)))
        assertEquals("a straight line stays a line", "line", kind(drawn(100f to 100f, 300f to 140f)))
        // Writing (sharp turns all along) is no shaft, also as the pen lifts.
        val word = drawn(100f to 200f, 110f to 170f, 120f to 200f, 130f to 170f, 140f to 200f, 150f to 170f, 160f to 200f, 200f to 200f, 190f to 192f, 200f to 200f, 190f to 208f)
        assertNull(Arrows.fromStroke(word, 0, word.size / 4 - 1, strict = true))
    }

    @Test fun aVeeAtTheEndOfALineBecomesItsHead() {
        val line = drawn(100f to 200f, 300f to 202f, wobble = 0.3f)
        val e = end(line)
        val vee = drawn(282f to 186f, 302f to 199f, 279f to 217f)
        val head = headOn(line, vee)
        assertNotNull(head)
        val tip = pts(head!!).maxBy { it.first }
        assertEquals(e[0], tip.first, 0.5f)
        assertEquals(e[1], tip.second, 0.5f)
        // A "<" after the line (opening away from it) and a "v" below its end stay as they are.
        assertNull(headOn(line, drawn(322f to 188f, 303f to 200f, 322f to 213f)))
        assertNull(headOn(line, drawn(290f to 210f, 300f to 230f, 310f to 210f)))
        // Drawn from the point: out to one barb, back along it, out to the other.
        assertNotNull("from the point, retraced", headOn(line, drawn(e[0] to e[1], e[0] - 18f to e[1] - 13f, e[0] + 0.5f to e[1], e[0] - 17f to e[1] + 14f)))
        // Number lines keep their ends: a round bracket or a small circle at a line's end is no head.
        val start = end(line, last = false) // the line's left end, the way into it is to the right
        val paren = (0..24).map { val t = Math.PI * (0.5 + it / 24.0); (start[0] + 11f + 12f * kotlin.math.cos(t).toFloat()) to (start[1] + 12f * kotlin.math.sin(t).toFloat()) }
        assertNull("(", Arrows.head(drawn(*paren.toTypedArray(), wobble = 0.2f), 0, drawn(*paren.toTypedArray(), wobble = 0.2f).size / 4 - 1, start[0], start[1], start[2], start[3], start[4]))
        val circle = (0..30).map { val t = 2 * Math.PI * it / 30; (start[0] - 6f + 6f * kotlin.math.cos(t).toFloat()) to (start[1] + 6f * kotlin.math.sin(t).toFloat()) }
        val c = drawn(*circle.toTypedArray(), wobble = 0.2f)
        assertNull("o", Arrows.head(c, 0, c.size / 4 - 1, start[0], start[1], start[2], start[3], start[4]))
        // A head as big as the line is no head.
        assertNull(headOn(drawn(100f to 200f, 130f to 200f), drawn(90f to 160f, 132f to 200f, 90f to 240f)))
    }
}
