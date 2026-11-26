package dev.n0bridge.n0te

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.StrokeJoin
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp

/** Line icons drawn on a 24-unit grid (thin, even strokes: the Nothing look). */
enum class Glyph { BUCKET, LOUPE, ARROW_LEFT, ARROW_RIGHT, RETURN, CLOSE, BACK, PEN, MARKER, ERASER, LASSO, UNDO, REDO, HAND, PLUS, MENU, SEND, TRASH, CHECK, PDF, RULER, BOARD, DOWN, LEFT, RIGHT, GROUP, SHARE, LAPTOP, PAGE, UP_ARROW, DOWN_ARROW, SETTINGS, PIPETTE, LOCK, UNLOCK, COPY, CUT, PASTE, LASER, TEXT, SPACE, HEADING, SEARCH, PLOT, FOLDER, TAG, PIN, HOME, MARK, LIST, GRID, SPLIT, STAR, MAXIMIZE, RESTORE }

@Composable
fun GlyphIcon(g: Glyph, tint: Color, size: Dp = 22.dp) {
    Canvas(Modifier.size(size)) {
        val k = this.size.minDimension / 24f
        val st = Stroke(width = 1.7f * k, cap = StrokeCap.Round, join = StrokeJoin.Round)
        fun p(vararg xy: Float): Path = Path().apply {
            moveTo(xy[0] * k, xy[1] * k)
            var i = 2
            while (i + 1 < xy.size) { lineTo(xy[i] * k, xy[i + 1] * k); i += 2 }
        }
        fun line(vararg xy: Float) = drawPath(p(*xy), tint, style = st)
        when (g) {
            Glyph.BACK -> line(15f, 5f, 8f, 12f, 15f, 19f)
            Glyph.PEN -> { line(5f, 19f, 6f, 15f, 16f, 5f, 19f, 8f, 9f, 18f, 5f, 19f); line(14f, 7f, 17f, 10f) }
            Glyph.MARKER -> { line(7f, 17f, 7f, 11f, 13f, 5f, 19f, 11f, 13f, 17f, 7f, 17f); line(5f, 20f, 19f, 20f) }
            Glyph.ERASER -> { line(9f, 19f, 4f, 14f, 13f, 5f, 20f, 12f, 13f, 19f, 9f, 19f); line(8.5f, 9.5f, 15.5f, 16.5f); line(13f, 19f, 20f, 19f) }
            Glyph.LASSO -> {
                drawOval(tint, topLeft = Offset(4f * k, 4f * k), size = Size(16f * k, 11f * k), style = Stroke(width = 1.7f * k, cap = StrokeCap.Round, pathEffect = androidx.compose.ui.graphics.PathEffect.dashPathEffect(floatArrayOf(3f * k, 2.6f * k))))
                line(9f, 15f, 8f, 20f)
            }
            Glyph.UNDO -> { line(9f, 6f, 5f, 10f, 9f, 14f); arcBack(k, tint, st, flip = false) }
            Glyph.REDO -> { line(15f, 6f, 19f, 10f, 15f, 14f); arcBack(k, tint, st, flip = true) }
            Glyph.HAND -> { line(8f, 13f, 8f, 6f); line(11f, 12f, 11f, 4f); line(14f, 12f, 14f, 5f); line(17f, 13f, 17f, 8f); line(17f, 13f, 17f, 15f, 14f, 20f, 9f, 20f, 5f, 15f, 6f, 13f, 8f, 15f) }
            Glyph.PLUS -> { line(12f, 5f, 12f, 19f); line(5f, 12f, 19f, 12f) }
            Glyph.MENU -> { line(5f, 7f, 19f, 7f); line(5f, 12f, 19f, 12f); line(5f, 17f, 13f, 17f) }
            Glyph.SEND -> { line(4f, 12f, 20f, 4f, 15f, 20f, 11f, 13f, 4f, 12f); line(11f, 13f, 20f, 4f) }
            Glyph.TRASH -> { line(5f, 7f, 19f, 7f); line(7f, 7f, 8f, 20f, 16f, 20f, 17f, 7f); line(10f, 7f, 10f, 4f, 14f, 4f, 14f, 7f) }
            Glyph.CHECK -> line(5f, 12f, 10f, 17f, 19f, 7f)
            // The writing pad: the pen (the same as the pen tool's) writing into a box.
            Glyph.LOUPE -> {
                line(2.5f, 12f, 13f, 12f)
                line(21.5f, 16f, 21.5f, 21f, 2.5f, 21f, 2.5f, 12f)
                line(5.5f, 17.5f, 7.5f, 15.5f, 9.5f, 17.5f, 11f, 16.5f)
                line(12f, 17.2f, 12.72f, 14.32f, 19.92f, 7.12f, 22.08f, 9.28f, 14.88f, 16.48f, 12f, 17.2f)
                line(18.48f, 8.56f, 20.64f, 10.72f)
            }
            Glyph.ARROW_LEFT -> { line(19f, 12f, 5f, 12f); line(11f, 6f, 5f, 12f, 11f, 18f) }
            Glyph.ARROW_RIGHT -> { line(5f, 12f, 19f, 12f); line(13f, 6f, 19f, 12f, 13f, 18f) }
            Glyph.RETURN -> { line(19f, 5f, 19f, 14f, 6f, 14f); line(10f, 10f, 6f, 14f, 10f, 18f) }
            Glyph.CLOSE -> { line(6f, 6f, 18f, 18f); line(18f, 6f, 6f, 18f) }
            Glyph.RULER -> { line(3f, 15f, 15f, 3f, 21f, 9f, 9f, 21f, 3f, 15f); line(7f, 11f, 9f, 13f); line(10f, 8f, 12f, 10f); line(13f, 5f, 15f, 7f) }
            Glyph.BOARD -> { line(4f, 5f, 20f, 5f, 20f, 19f, 4f, 19f, 4f, 5f); line(8f, 15f, 11f, 10f, 14f, 13f, 17f, 9f) }
            Glyph.DOWN -> { line(5f, 4f, 19f, 4f, 19f, 12f, 5f, 12f, 5f, 4f); line(12f, 14f, 12f, 21f); line(9f, 18f, 12f, 21f, 15f, 18f) }
            Glyph.LEFT -> { line(10f, 4f, 20f, 4f, 20f, 20f, 10f, 20f, 10f, 4f); line(8f, 12f, 2f, 12f); line(5f, 9f, 2f, 12f, 5f, 15f) }
            Glyph.RIGHT -> { line(4f, 4f, 14f, 4f, 14f, 20f, 4f, 20f, 4f, 4f); line(16f, 12f, 22f, 12f); line(19f, 9f, 22f, 12f, 19f, 15f) }
            Glyph.GROUP -> { line(3f, 7f, 9f, 7f, 11f, 9f, 21f, 9f, 21f, 19f, 3f, 19f, 3f, 7f); line(3f, 12f, 21f, 12f) }
            Glyph.SHARE -> { line(12f, 3f, 12f, 15f); line(8f, 7f, 12f, 3f, 16f, 7f); line(6f, 11f, 4f, 11f, 4f, 21f, 20f, 21f, 20f, 11f, 18f, 11f) }
            Glyph.LAPTOP -> { line(5f, 6f, 19f, 6f, 19f, 15f, 5f, 15f, 5f, 6f); line(2f, 18f, 22f, 18f) }
            Glyph.PAGE -> { line(6f, 3f, 18f, 3f, 18f, 21f, 6f, 21f, 6f, 3f); line(9f, 8f, 15f, 8f); line(9f, 12f, 15f, 12f); line(9f, 16f, 13f, 16f) }
            Glyph.SETTINGS -> {
                // Gear: ring with six teeth.
                drawCircle(tint, 3.2f * k, Offset(12f * k, 12f * k), style = st)
                drawCircle(tint, 6.6f * k, Offset(12f * k, 12f * k), style = st)
                for (i in 0 until 6) {
                    val a = Math.toRadians(i * 60.0)
                    val c = kotlin.math.cos(a).toFloat(); val sn = kotlin.math.sin(a).toFloat()
                    line(12f + 6.6f * c, 12f + 6.6f * sn, 12f + 9f * c, 12f + 9f * sn)
                }
            }
            Glyph.PIPETTE -> { line(5f, 19f, 6f, 16f, 13f, 9f, 15f, 11f, 8f, 18f, 5f, 19f); line(12f, 8f, 16f, 12f); line(14f, 10f, 17f, 7f, 18.5f, 5.5f) ; drawCircle(tint, 2.2f * k, Offset(17.6f * k, 6.4f * k), style = st) }
            Glyph.LOCK -> { line(6f, 11f, 18f, 11f, 18f, 20f, 6f, 20f, 6f, 11f); line(8.5f, 11f, 8.5f, 8f); line(15.5f, 11f, 15.5f, 8f); drawArc(tint, 180f, 180f, false, Offset(8.5f * k, 4.5f * k), Size(7f * k, 7f * k), style = st) }
            Glyph.UNLOCK -> { line(6f, 11f, 18f, 11f, 18f, 20f, 6f, 20f, 6f, 11f); line(8.5f, 11f, 8.5f, 8f); drawArc(tint, 180f, 160f, false, Offset(8.5f * k, 4.5f * k), Size(7f * k, 7f * k), style = st) }
            Glyph.COPY -> { line(9f, 8f, 19f, 8f, 19f, 20f, 9f, 20f, 9f, 8f); line(5f, 16f, 5f, 4f, 15f, 4f) }
            Glyph.CUT -> {
                drawCircle(tint, 2.8f * k, Offset(7f * k, 17f * k), style = st); drawCircle(tint, 2.8f * k, Offset(17f * k, 17f * k), style = st)
                line(9f, 15f, 17f, 4f); line(15f, 15f, 7f, 4f)
            }
            Glyph.PASTE -> { line(7f, 5f, 5f, 5f, 5f, 21f, 19f, 21f, 19f, 5f, 17f, 5f); line(9f, 3f, 15f, 3f, 15f, 7f, 9f, 7f, 9f, 3f); line(9f, 13f, 15f, 13f); line(9f, 17f, 13f, 17f) }
            // The paint bucket: a tipped pail pouring a drop.
            Glyph.BUCKET -> { line(5f, 11f, 11f, 5f, 18f, 12f, 12f, 18f, 5f, 11f); line(5f, 11f, 18f, 12f); line(19f, 14f, 20f, 17f, 19f, 19f, 18f, 17f, 19f, 14f) }
            Glyph.LASER -> { drawCircle(tint, 3f * k, Offset(16f * k, 8f * k)); line(4f, 20f, 13f, 11f); line(16f, 2f, 16f, 3.5f); line(22f, 8f, 20.5f, 8f); line(20.2f, 3.8f, 19.2f, 4.8f) }
            Glyph.TEXT -> { line(5f, 7f, 5f, 4.5f, 19f, 4.5f, 19f, 7f); line(12f, 4.5f, 12f, 19.5f); line(9f, 19.5f, 15f, 19.5f) }
            Glyph.UP_ARROW -> { line(12f, 20f, 12f, 4f); line(6f, 10f, 12f, 4f, 18f, 10f) }
            Glyph.DOWN_ARROW -> { line(12f, 4f, 12f, 20f); line(6f, 14f, 12f, 20f, 18f, 14f) }
            // A coordinate system with a curve.
            Glyph.PLOT -> { line(4f, 3.5f, 4f, 20f, 20.5f, 20f); line(6.5f, 5f, 9f, 12.5f, 12f, 16f, 15f, 12.5f, 17.5f, 5f) }
            // A heading: an H over a line.
            Glyph.HEADING -> { line(6f, 4f, 6f, 16f); line(15f, 4f, 15f, 16f); line(6f, 10f, 15f, 10f); line(4f, 20.5f, 20f, 20.5f) }
            // A folder: tab on the left, the body below it.
            Glyph.FOLDER -> line(3f, 18f, 3f, 6f, 9f, 6f, 11f, 8.5f, 21f, 8.5f, 21f, 18f, 3f, 18f)
            // A tag: a label with a hole, its point to the left.
            Glyph.TAG -> { line(11f, 4f, 20f, 4f, 20f, 13f, 11.5f, 21.5f, 2.5f, 12.5f, 11f, 4f); drawCircle(tint, 1.4f * k, Offset(16f * k, 8f * k), style = st) }
            // A house: roof and walls with a door.
            Glyph.HOME -> { line(3.5f, 11f, 12f, 4f, 20.5f, 11f); line(6f, 9.5f, 6f, 20f, 18f, 20f, 18f, 9.5f); line(10f, 20f, 10f, 14.5f, 14f, 14.5f, 14f, 20f) }
            // A bookmark ribbon.
            Glyph.MARK -> line(7f, 3.5f, 17f, 3.5f, 17f, 20.5f, 12f, 16f, 7f, 20.5f, 7f, 3.5f)
            // Rows: the list view.
            Glyph.LIST -> { line(9f, 6f, 20f, 6f); line(9f, 12f, 20f, 12f); line(9f, 18f, 20f, 18f); line(4f, 6f, 5f, 6f); line(4f, 12f, 5f, 12f); line(4f, 18f, 5f, 18f) }
            // Tiles: the grid view.
            Glyph.GRID -> { line(4f, 4f, 10.5f, 4f, 10.5f, 10.5f, 4f, 10.5f, 4f, 4f); line(13.5f, 4f, 20f, 4f, 20f, 10.5f, 13.5f, 10.5f, 13.5f, 4f); line(4f, 13.5f, 10.5f, 13.5f, 10.5f, 20f, 4f, 20f, 4f, 13.5f); line(13.5f, 13.5f, 20f, 13.5f, 20f, 20f, 13.5f, 20f, 13.5f, 13.5f) }
            // A pin: head, body, point.
            Glyph.PIN -> { line(9f, 3.5f, 15f, 3.5f); line(10f, 3.5f, 10f, 9f, 7f, 13f, 17f, 13f, 14f, 9f, 14f, 3.5f); line(12f, 13f, 12f, 20.5f) }
            Glyph.SEARCH -> { drawCircle(tint, 6.5f * k, Offset(10.5f * k, 10.5f * k), style = st); line(15.3f, 15.3f, 20.5f, 20.5f) }
            // Insert space: lines of writing pushed apart, arrows up and down from the gap.
            Glyph.SPACE -> { line(4f, 3.5f, 20f, 3.5f); line(4f, 20.5f, 20f, 20.5f); line(12f, 11f, 12f, 7f); line(9.5f, 9f, 12f, 6.5f, 14.5f, 9f); line(12f, 13f, 12f, 17f); line(9.5f, 15f, 12f, 17.5f, 14.5f, 15f) }
            // Two pages side by side: open beside.
            Glyph.SPLIT -> { line(3f, 5f, 10.5f, 5f, 10.5f, 19f, 3f, 19f, 3f, 5f); line(13.5f, 5f, 21f, 5f, 21f, 19f, 13.5f, 19f, 13.5f, 5f) }
            // The main pane.
            Glyph.STAR -> line(12f, 3.5f, 14.35f, 9.26f, 20.56f, 9.72f, 15.8f, 13.74f, 17.29f, 19.78f, 12f, 16.5f, 6.71f, 19.78f, 8.2f, 13.74f, 3.44f, 9.72f, 9.65f, 9.26f, 12f, 3.5f)
            // Corners out: alone on the screen; corners in: back beside the other.
            Glyph.MAXIMIZE -> { line(4f, 9f, 4f, 4f, 9f, 4f); line(15f, 4f, 20f, 4f, 20f, 9f); line(20f, 15f, 20f, 20f, 15f, 20f); line(9f, 20f, 4f, 20f, 4f, 15f) }
            Glyph.RESTORE -> { line(9f, 4f, 9f, 9f, 4f, 9f); line(15f, 4f, 15f, 9f, 20f, 9f); line(20f, 15f, 15f, 15f, 15f, 20f); line(4f, 15f, 9f, 15f, 9f, 20f) }
            Glyph.PDF -> { line(6f, 3f, 14f, 3f, 19f, 8f, 19f, 21f, 6f, 21f, 6f, 3f); line(14f, 3f, 14f, 8f, 19f, 8f); line(9f, 14f, 16f, 14f); line(9f, 17f, 14f, 17f) }
        }
    }
}

private fun DrawScope.arcBack(k: Float, tint: Color, st: Stroke, flip: Boolean) {
    val path = Path().apply {
        if (!flip) { moveTo(5f * k, 10f * k); cubicTo(11f * k, 10f * k, 19f * k, 10f * k, 19f * k, 15f * k); cubicTo(19f * k, 18f * k, 16f * k, 19f * k, 13f * k, 19f * k) }
        else { moveTo(19f * k, 10f * k); cubicTo(13f * k, 10f * k, 5f * k, 10f * k, 5f * k, 15f * k); cubicTo(5f * k, 18f * k, 8f * k, 19f * k, 11f * k, 19f * k) }
    }
    drawPath(path, tint, style = st)
}
