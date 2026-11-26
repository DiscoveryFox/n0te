package dev.n0bridge.n0te

import androidx.compose.foundation.Canvas
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.withFrameNanos
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.drawscope.rotate
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.delay
import kotlin.math.sin

/**
 * A design's decorations (snow, fairy lights, stars, a grid, petals, leaves, scanlines) behind
 * the library. Only the canvas redraws while they move (the time is read in the draw phase,
 * nothing recomposes), at about 30 frames a second, and only while the screen is visible.
 * [still]: hold still (the miniatures on design cards). The same pictures as the laptop's
 * (crates/desktop/src/decor.rs).
 */
@Composable
fun DecorLayer(theme: Theme, modifier: Modifier = Modifier, still: Boolean = false) {
    if (theme.decor.isEmpty()) return
    val ctx = LocalContext.current
    val moving = !still && Settings.decorMotion && theme.decor.any { it.animated } && remember { Decorations.animationsOn(ctx) }
    val time = remember { mutableFloatStateOf(0f) }
    if (moving) LaunchedEffect(theme.id) {
        var start = -1L
        while (true) {
            withFrameNanos { now -> if (start < 0) start = now; time.floatValue = (now - start) / 1e9f }
            delay(26)
        }
    }
    val path = remember { Path() }
    Canvas(modifier) { drawDecor(theme, if (moving) time.floatValue else Decorations.STILL_T, path) }
}

object Decorations {
    /** The moment a still decoration shows (snowflakes mid-air, stars half lit). */
    const val STILL_T = 7.3f

    /** The system's "remove animations" switch. */
    fun animationsOn(ctx: android.content.Context) =
        runCatching { android.provider.Settings.Global.getFloat(ctx.contentResolver, android.provider.Settings.Global.ANIMATOR_DURATION_SCALE, 1f) > 0f }.getOrDefault(true)

    /** A stable pseudo-random number in 0 … 1 for item [i] (each [salt] another property). */
    fun h(i: Int, salt: Int): Float {
        var x = i * 374761393 + salt * 668265263
        x = (x xor (x ushr 13)) * 1274126177
        x = x xor (x ushr 16)
        return (x and 0xFFFFFF) / 16777215f
    }

    /** How many items for an area: [base] on a 1200 × 800 dp screen, fewer on small ones. */
    fun count(base: Float, wDp: Float, hDp: Float) = (base * ((wDp * hDp) / (1200f * 800f)).coerceIn(0.12f, 1.6f)).toInt().coerceAtLeast(3)
}

fun DrawScope.drawDecor(theme: Theme, t: Float, path: Path) {
    if (size.width <= 0f || size.height <= 0f) return
    for (d in theme.decor) when (d.kind) {
        "snow" -> snow(d, t)
        "petals" -> falling(d, t, leaf = false, path)
        "leaves" -> falling(d, t, leaf = true, path)
        "stars" -> stars(d, t)
        "lights" -> lights(d, t, theme, path)
        "grid" -> grid(d, theme)
        "scanlines" -> scanlines(d, theme)
        // A kind from a newer n0te: nothing.
    }
}

private fun colors(d: Decor, vararg defaults: Long): List<Long> = d.colors.ifEmpty { defaults.toList() }

private fun DrawScope.snow(d: Decor, t: Float) {
    val w = size.width; val h = size.height
    val n = Decorations.count(24f + 96f * d.amount, w / density, h / density)
    val cols = colors(d, 0xFFFFFFFF)
    val pad = 12.dp.toPx()
    for (i in 0 until n) {
        val r = (1.1f + 2.8f * Decorations.h(i, 3)).dp.toPx()
        // Bigger (nearer) flakes fall faster.
        val speed = (12f + 24f * Decorations.h(i, 2)).dp.toPx() + r * 4f
        val y = (Decorations.h(i, 4) * (h + 2 * pad) + t * speed) % (h + 2 * pad) - pad
        val x = Decorations.h(i, 1) * w + sin(t * (0.4f + 0.5f * Decorations.h(i, 8)) + Decorations.h(i, 5) * 6.283f) * 14.dp.toPx() * Decorations.h(i, 6)
        drawCircle(Color(cols[i % cols.size]).copy(alpha = 0.35f + 0.55f * Decorations.h(i, 7)), r, Offset(x, y))
    }
}

private fun DrawScope.falling(d: Decor, t: Float, leaf: Boolean, path: Path) {
    val w = size.width; val h = size.height
    val n = Decorations.count(if (leaf) 10f + 24f * d.amount else 14f + 34f * d.amount, w / density, h / density)
    val cols = if (leaf) colors(d, 0xFFC2410C, 0xFFD97706, 0xFFA16207) else colors(d, 0xFFF9A8C2, 0xFFFBCFE0)
    for (i in 0 until n) {
        val s = (if (leaf) 11f + 7f * Decorations.h(i, 3) else 9f + 5f * Decorations.h(i, 3)).dp.toPx()
        val pad = 2 * s
        val speed = (10f + 18f * Decorations.h(i, 2)).dp.toPx()
        val y = (Decorations.h(i, 4) * (h + 2 * pad) + t * speed) % (h + 2 * pad) - pad
        val x = Decorations.h(i, 1) * w + sin(t * (0.3f + 0.4f * Decorations.h(i, 8)) + Decorations.h(i, 5) * 6.283f) * 30.dp.toPx() * Decorations.h(i, 6)
        val spin = (20f + 60f * Decorations.h(i, 10)) * (if (Decorations.h(i, 11) > 0.5f) 1f else -1f)
        val c = Color(cols[i % cols.size]).copy(alpha = 0.55f + 0.35f * Decorations.h(i, 7))
        rotate(Decorations.h(i, 9) * 360f + t * spin, Offset(x, y)) {
            if (leaf) {
                path.reset()
                path.moveTo(x - s / 2, y)
                path.quadraticTo(x, y - s * 0.45f, x + s / 2, y)
                path.quadraticTo(x, y + s * 0.45f, x - s / 2, y)
                path.close()
                drawPath(path, c)
            } else {
                drawOval(c, Offset(x - s / 2, y - s * 0.3f), Size(s, s * 0.6f))
            }
        }
    }
}

private fun DrawScope.stars(d: Decor, t: Float) {
    val w = size.width; val h = size.height
    val n = Decorations.count(36f + 110f * d.amount, w / density, h / density)
    val cols = colors(d, 0xFFFFFFFF)
    for (i in 0 until n) {
        val r = (0.5f + 1.3f * Decorations.h(i, 3) * Decorations.h(i, 3)).dp.toPx()
        val twinkle = 0.5f + 0.5f * sin(t * (0.6f + 1.8f * Decorations.h(i, 4)) + Decorations.h(i, 5) * 6.283f)
        val a = (0.2f + 0.8f * twinkle * (0.6f + 0.4f * Decorations.h(i, 6))).coerceIn(0f, 1f)
        val o = Offset(Decorations.h(i, 1) * w, Decorations.h(i, 2) * h)
        val c = Color(cols[i % cols.size])
        drawCircle(c.copy(alpha = a), r, o)
        if (Decorations.h(i, 7) > 0.9f) {
            val l = r * 3.5f
            val sw = 0.8.dp.toPx()
            drawLine(c.copy(alpha = a * 0.7f), Offset(o.x - l, o.y), Offset(o.x + l, o.y), sw)
            drawLine(c.copy(alpha = a * 0.7f), Offset(o.x, o.y - l), Offset(o.x, o.y + l), sw)
        }
    }
}

private fun DrawScope.lights(d: Decor, t: Float, theme: Theme, path: Path) {
    val w = size.width
    val cols = colors(d, 0xFFF5C542, 0xFFD7263D, 0xFF3FA34D, 0xFF4A90E2)
    val span = 160.dp.toPx()
    val sag = 13.dp.toPx() * (0.6f + 0.6f * d.amount)
    val top = 2.dp.toPx()
    val segments = (w / span).toInt() + 1
    path.reset()
    path.moveTo(0f, top)
    for (s in 0 until segments) path.quadraticTo(s * span + span / 2, top + 2 * sag, (s + 1) * span, top)
    drawPath(path, Color(theme.muted).copy(alpha = 0.7f), style = Stroke(1.2.dp.toPx()))
    val drop = 5.dp.toPx()
    var i = 0
    for (s in 0 until segments) for (u in floatArrayOf(0.25f, 0.5f, 0.75f)) {
        val x0 = s * span
        val x = (1 - u) * (1 - u) * x0 + 2 * u * (1 - u) * (x0 + span / 2) + u * u * (x0 + span)
        val y = top + 2 * u * (1 - u) * 2 * sag + drop
        val glow = 0.55f + 0.45f * sin(t * 1.3f + i * 1.7f)
        val c = Color(cols[i % cols.size])
        drawCircle(c.copy(alpha = 0.2f * glow), 9.dp.toPx(), Offset(x, y))
        drawCircle(c.copy(alpha = 0.6f + 0.4f * glow), 4.dp.toPx(), Offset(x, y))
        drawRect(Color(theme.muted), Offset(x - 1.5.dp.toPx(), y - drop + 1.dp.toPx()), Size(3.dp.toPx(), 3.dp.toPx()))
        i++
    }
}

private fun DrawScope.grid(d: Decor, theme: Theme) {
    val c = Color(d.colors.firstOrNull() ?: theme.fg)
    val step = 24.dp.toPx()
    val a = 0.05f + 0.06f * d.amount
    val px = 1f
    var k = 0
    var x = 0f
    while (x <= size.width) { drawLine(c.copy(alpha = if (k % 5 == 0) a * 2.2f else a), Offset(x, 0f), Offset(x, size.height), px); x += step; k++ }
    k = 0
    var y = 0f
    while (y <= size.height) { drawLine(c.copy(alpha = if (k % 5 == 0) a * 2.2f else a), Offset(0f, y), Offset(size.width, y), px); y += step; k++ }
}

private fun DrawScope.scanlines(d: Decor, theme: Theme) {
    val c = Color(d.colors.firstOrNull() ?: theme.accent).copy(alpha = 0.04f + 0.08f * d.amount)
    val step = 3.dp.toPx()
    val sw = 1.dp.toPx()
    var y = 0f
    while (y <= size.height) { drawLine(c, Offset(0f, y), Offset(size.width, y), sw); y += step }
}
