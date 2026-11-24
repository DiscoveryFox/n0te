package dev.n0bridge.n0te

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.PathEffect
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.StrokeJoin
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp

/**
 * The graph tools' icons, drawn like GeoGebra's (the same pictures, so they are found where
 * they are expected): lines in the ink, points as dots in the accent, on a 24 × 24 grid.
 */
@Composable
fun ToolIcon(t: GTool, ink: Color, accent: Color, size: Dp = 26.dp) {
    Canvas(Modifier.size(size)) {
        val k = this.size.minDimension / 24f
        val st = Stroke(width = 1.6f * k, cap = StrokeCap.Round, join = StrokeJoin.Round)
        val thin = Stroke(width = 1.1f * k, cap = StrokeCap.Round, join = StrokeJoin.Round)
        fun o(x: Float, y: Float) = Offset(x * k, y * k)
        fun line(vararg xy: Float, color: Color = ink, s: Stroke = st) {
            val p = Path()
            p.moveTo(xy[0] * k, xy[1] * k)
            var i = 2
            while (i + 1 < xy.size) { p.lineTo(xy[i] * k, xy[i + 1] * k); i += 2 }
            drawPath(p, color, style = s)
        }
        fun dot(x: Float, y: Float, r: Float = 2.3f) = drawCircle(accent, r * k, o(x, y))
        fun ring(x: Float, y: Float, r: Float, color: Color = ink, s: Stroke = st) = drawCircle(color, r * k, o(x, y), style = s)
        fun curve(x0: Float, y0: Float, cx: Float, cy: Float, x1: Float, y1: Float, color: Color = ink) {
            val p = Path(); p.moveTo(x0 * k, y0 * k); p.quadraticTo(cx * k, cy * k, x1 * k, y1 * k); drawPath(p, color, style = st)
        }
        when (t) {
            GTool.MOVE -> {
                // An arrow cursor.
                val p = Path(); p.moveTo(6f * k, 3f * k); p.lineTo(6f * k, 19f * k); p.lineTo(10f * k, 15f * k); p.lineTo(13f * k, 21f * k); p.lineTo(15.5f * k, 20f * k); p.lineTo(12.5f * k, 14f * k); p.lineTo(18f * k, 14f * k); p.close()
                drawPath(p, ink, style = st)
            }
            GTool.VIEW -> {
                line(12f, 3f, 12f, 21f); line(3f, 12f, 21f, 12f)
                line(9f, 6f, 12f, 3f, 15f, 6f); line(9f, 18f, 12f, 21f, 15f, 18f); line(6f, 9f, 3f, 12f, 6f, 15f); line(18f, 9f, 21f, 12f, 18f, 15f)
            }
            GTool.POINT -> { dot(12f, 12f, 3.2f); line(15f, 6f, 18f, 3.5f, s = thin); line(16.5f, 3.5f, 18f, 3.5f, 18f, 5f, s = thin) }
            GTool.INTERSECT -> { line(3f, 18f, 21f, 6f); curve(3f, 6f, 12f, 24f, 21f, 14f); dot(9.6f, 13.6f); dot(16.4f, 9.1f) }
            GTool.ROOTS -> {
                line(2f, 13f, 22f, 13f, s = thin)
                val p = Path(); p.moveTo(2f * k, 20f * k); p.cubicTo(8f * k, 2f * k, 14f * k, 26f * k, 22f * k, 5f * k); drawPath(p, ink, style = st)
                dot(5.6f, 13f); dot(12f, 13f); dot(18.6f, 13f)
            }
            GTool.EXTREMUM -> {
                line(2f, 20f, 22f, 20f, s = thin)
                curve(3f, 19f, 12f, -3f, 21f, 19f)
                dot(12f, 8f, 2.6f)
            }
            GTool.MIDPOINT -> { line(3f, 16f, 21f, 8f); dot(3f, 16f, 1.8f); dot(21f, 8f, 1.8f); drawCircle(accent, 3f * k, o(12f, 12f)) }
            GTool.LINE -> { line(2f, 19f, 22f, 5f); dot(7f, 15.5f); dot(17f, 8.5f) }
            GTool.SEGMENT -> { line(5f, 17f, 19f, 7f); dot(5f, 17f); dot(19f, 7f) }
            GTool.RAY -> { line(5f, 17f, 22f, 5f); dot(5f, 17f); dot(14f, 10.6f) }
            GTool.VECTOR -> { line(4f, 18f, 19f, 7f); line(13.5f, 7.5f, 19f, 7f, 17.5f, 12.3f); dot(4f, 18f) }
            GTool.PERPENDICULAR -> { line(2f, 18f, 22f, 18f); line(12f, 18f, 12f, 3f); line(12f, 14f, 16f, 14f, 16f, 18f, s = thin); dot(12f, 8f) }
            GTool.PARALLEL -> { line(2f, 16f, 18f, 4f); line(6f, 21f, 22f, 9f); dot(14f, 12f) }
            GTool.BISECTOR -> { line(4f, 17f, 20f, 17f); dot(4f, 17f); dot(20f, 17f); line(12f, 21f, 12f, 3f, color = accent, s = thin) }
            GTool.ANGLE_BISECTOR -> { line(20f, 4f, 4f, 20f, 21f, 20f); line(4f, 20f, 21f, 11f, color = accent, s = thin) }
            GTool.TANGENT -> { curve(3f, 6f, 12f, 30f, 21f, 6f); line(3f, 18f, 21f, 18f, color = accent); dot(12f, 18f) }
            GTool.POLYGON -> {
                val p = Path(); p.moveTo(4f * k, 19f * k); p.lineTo(12f * k, 4f * k); p.lineTo(20f * k, 17f * k); p.close()
                drawPath(p, accent.copy(alpha = 0.18f)); drawPath(p, ink, style = st)
                dot(4f, 19f, 1.8f); dot(12f, 4f, 1.8f); dot(20f, 17f, 1.8f)
            }
            GTool.CIRCLE -> { ring(12f, 12f, 8f); dot(12f, 12f); dot(17.7f, 6.3f) }
            GTool.CIRCLE_RADIUS -> { ring(12f, 12f, 8f); line(12f, 12f, 20f, 12f, s = thin); dot(12f, 12f) }
            GTool.CIRCLE3 -> { ring(12f, 12f, 8f); dot(4f, 12f); dot(17.7f, 6.3f); dot(15f, 19.4f) }
            GTool.ANGLE -> { line(20f, 6f, 4f, 19f, 21f, 19f); drawArc(accent, -38f, 38f, false, o(-1f, 11f), Size(10f * k, 16f * k), style = st) }
            GTool.DISTANCE -> { line(3f, 12f, 21f, 12f); line(3f, 9f, 3f, 15f); line(21f, 9f, 21f, 15f); line(7f, 12f, 7f, 10f, s = thin); line(11f, 12f, 11f, 10f, s = thin); line(15f, 12f, 15f, 10f, s = thin); line(19f, 12f, 19f, 10f, s = thin) }
            GTool.AREA -> {
                val p = Path(); p.moveTo(3f * k, 19f * k); p.lineTo(8f * k, 5f * k); p.lineTo(20f * k, 8f * k); p.lineTo(18f * k, 19f * k); p.close()
                drawPath(p, accent.copy(alpha = 0.35f)); drawPath(p, ink, style = st)
            }
            GTool.SLOPE -> { line(3f, 20f, 21f, 5f); line(9f, 15f, 15f, 15f, 15f, 10f, color = accent) }
            GTool.REFLECT_LINE -> {
                line(12f, 2f, 12f, 22f, color = accent, s = Stroke(width = 1.4f * k, pathEffect = PathEffect.dashPathEffect(floatArrayOf(2.5f * k, 2f * k))))
                val p = Path(); p.moveTo(3f * k, 17f * k); p.lineTo(8f * k, 7f * k); p.lineTo(9f * k, 17f * k); p.close(); drawPath(p, ink, style = st)
                val q = Path(); q.moveTo(21f * k, 17f * k); q.lineTo(16f * k, 7f * k); q.lineTo(15f * k, 17f * k); q.close(); drawPath(q, ink.copy(alpha = 0.5f), style = st)
            }
            GTool.REFLECT_POINT -> {
                dot(12f, 12f)
                val p = Path(); p.moveTo(3f * k, 5f * k); p.lineTo(9f * k, 5f * k); p.lineTo(6f * k, 10f * k); p.close(); drawPath(p, ink, style = st)
                val q = Path(); q.moveTo(21f * k, 19f * k); q.lineTo(15f * k, 19f * k); q.lineTo(18f * k, 14f * k); q.close(); drawPath(q, ink.copy(alpha = 0.5f), style = st)
            }
            GTool.ROTATE -> { drawArc(ink, 200f, 250f, false, o(4f, 4f), Size(16f * k, 16f * k), style = st); line(17.5f, 5f, 18.8f, 8.6f, 15f, 8.8f); dot(12f, 12f) }
            GTool.TRANSLATE -> {
                val p = Path(); p.moveTo(2f * k, 20f * k); p.lineTo(6f * k, 12f * k); p.lineTo(9f * k, 20f * k); p.close(); drawPath(p, ink, style = st)
                val q = Path(); q.moveTo(14f * k, 12f * k); q.lineTo(18f * k, 4f * k); q.lineTo(21f * k, 12f * k); q.close(); drawPath(q, ink.copy(alpha = 0.5f), style = st)
                line(8f, 14f, 16f, 8f, color = accent, s = thin)
            }
            GTool.SLIDER -> { line(3f, 13f, 21f, 13f); line(3f, 10f, 3f, 16f, s = thin); line(21f, 10f, 21f, 16f, s = thin); drawCircle(accent, 3.2f * k, o(14f, 13f)); line(4f, 6f, 7f, 6f, s = thin) }
            GTool.TEXT -> { line(4f, 19f, 9f, 5f, 14f, 19f); line(6f, 14f, 12f, 14f); line(16f, 9f, 16f, 19f, color = accent); line(16f, 9f, 19f, 9f, color = accent, s = thin) }
        }
    }
}
