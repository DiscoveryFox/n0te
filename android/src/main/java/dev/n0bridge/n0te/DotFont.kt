package dev.n0bridge.n0te

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp

/**
 * A 5×7 dot-matrix alphabet in the style of Nothing's dot type (our own glyphs): capitals,
 * digits and the lowercase letters of the wordmark. Unknown characters are left blank.
 */
object DotFont {
    private val glyphs: Map<Char, Array<String>> = mapOf(
        'A' to arrayOf(".XXX.", "X...X", "X...X", "XXXXX", "X...X", "X...X", "X...X"),
        'B' to arrayOf("XXXX.", "X...X", "X...X", "XXXX.", "X...X", "X...X", "XXXX."),
        'C' to arrayOf(".XXX.", "X...X", "X....", "X....", "X....", "X...X", ".XXX."),
        'D' to arrayOf("XXXX.", "X...X", "X...X", "X...X", "X...X", "X...X", "XXXX."),
        'E' to arrayOf("XXXXX", "X....", "X....", "XXXX.", "X....", "X....", "XXXXX"),
        'F' to arrayOf("XXXXX", "X....", "X....", "XXXX.", "X....", "X....", "X...."),
        'G' to arrayOf(".XXX.", "X...X", "X....", "X.XXX", "X...X", "X...X", ".XXXX"),
        'H' to arrayOf("X...X", "X...X", "X...X", "XXXXX", "X...X", "X...X", "X...X"),
        'I' to arrayOf(".XXX.", "..X..", "..X..", "..X..", "..X..", "..X..", ".XXX."),
        'J' to arrayOf("..XXX", "...X.", "...X.", "...X.", "...X.", "X..X.", ".XX.."),
        'K' to arrayOf("X...X", "X..X.", "X.X..", "XX...", "X.X..", "X..X.", "X...X"),
        'L' to arrayOf("X....", "X....", "X....", "X....", "X....", "X....", "XXXXX"),
        'M' to arrayOf("X...X", "XX.XX", "X.X.X", "X.X.X", "X...X", "X...X", "X...X"),
        'N' to arrayOf("X...X", "X...X", "XX..X", "X.X.X", "X..XX", "X...X", "X...X"),
        'O' to arrayOf(".XXX.", "X...X", "X...X", "X...X", "X...X", "X...X", ".XXX."),
        'P' to arrayOf("XXXX.", "X...X", "X...X", "XXXX.", "X....", "X....", "X...."),
        'Q' to arrayOf(".XXX.", "X...X", "X...X", "X...X", "X.X.X", "X..X.", ".XX.X"),
        'R' to arrayOf("XXXX.", "X...X", "X...X", "XXXX.", "X.X..", "X..X.", "X...X"),
        'S' to arrayOf(".XXXX", "X....", "X....", ".XXX.", "....X", "....X", "XXXX."),
        'T' to arrayOf("XXXXX", "..X..", "..X..", "..X..", "..X..", "..X..", "..X.."),
        'U' to arrayOf("X...X", "X...X", "X...X", "X...X", "X...X", "X...X", ".XXX."),
        'V' to arrayOf("X...X", "X...X", "X...X", "X...X", "X...X", ".X.X.", "..X.."),
        'W' to arrayOf("X...X", "X...X", "X...X", "X.X.X", "X.X.X", "X.X.X", ".X.X."),
        'X' to arrayOf("X...X", "X...X", ".X.X.", "..X..", ".X.X.", "X...X", "X...X"),
        'Y' to arrayOf("X...X", "X...X", ".X.X.", "..X..", "..X..", "..X..", "..X.."),
        'Z' to arrayOf("XXXXX", "....X", "...X.", "..X..", ".X...", "X....", "XXXXX"),
        '0' to arrayOf(".XXX.", "X...X", "X..XX", "X.X.X", "XX..X", "X...X", ".XXX."),
        '1' to arrayOf("..X..", ".XX..", "..X..", "..X..", "..X..", "..X..", ".XXX."),
        '2' to arrayOf(".XXX.", "X...X", "....X", "...X.", "..X..", ".X...", "XXXXX"),
        '3' to arrayOf("XXXXX", "...X.", "..X..", "...X.", "....X", "X...X", ".XXX."),
        '4' to arrayOf("...X.", "..XX.", ".X.X.", "X..X.", "XXXXX", "...X.", "...X."),
        '5' to arrayOf("XXXXX", "X....", "XXXX.", "....X", "....X", "X...X", ".XXX."),
        '6' to arrayOf("..XX.", ".X...", "X....", "XXXX.", "X...X", "X...X", ".XXX."),
        '7' to arrayOf("XXXXX", "....X", "...X.", "..X..", ".X...", ".X...", ".X..."),
        '8' to arrayOf(".XXX.", "X...X", "X...X", ".XXX.", "X...X", "X...X", ".XXX."),
        '9' to arrayOf(".XXX.", "X...X", "X...X", ".XXXX", "....X", "...X.", ".XX.."),
        'n' to arrayOf(".....", ".....", "X.XX.", "XX..X", "X...X", "X...X", "X...X"),
        't' to arrayOf(".X...", ".X...", "XXX..", ".X...", ".X...", ".X..X", "..XX."),
        'e' to arrayOf(".....", ".....", ".XXX.", "X...X", "XXXXX", "X....", ".XXX."),
    )

    private fun glyph(c: Char): Array<String>? = glyphs[c] ?: glyphs[c.uppercaseChar()] ?: when (c.uppercaseChar()) {
        'Ä' -> glyphs['A']; 'Ö' -> glyphs['O']; 'Ü' -> glyphs['U']; 'ß' -> glyphs['S']; else -> null
    }

    /** Width in cells (6 per character: 5 dots and a gap). */
    fun cells(text: String) = (text.length * 6 - 1).coerceAtLeast(0)

    /** Draws [text] with its top-left at [origin], one dot per cell of [cell] px. */
    fun DrawScope.drawDots(text: String, origin: Offset, cell: Float, color: Color) {
        text.forEachIndexed { i, ch ->
            val g = glyph(ch) ?: return@forEachIndexed
            for (row in 0 until 7) for (col in 0 until 5) {
                if (g[row][col] == 'X') drawCircle(color, cell * 0.38f, Offset(origin.x + (i * 6 + col + 0.5f) * cell, origin.y + (row + 0.5f) * cell))
            }
        }
    }
}

/** Dot-matrix text, [cell] per dot. */
@Composable
fun DotText(text: String, cell: Dp, color: Color, modifier: Modifier = Modifier) {
    Canvas(modifier.size(cell * DotFont.cells(text), cell * 7)) {
        with(DotFont) { drawDots(text, Offset.Zero, cell.toPx(), color) }
    }
}

/** The wordmark: "n0te" in dots with the red dot. */
@Composable
fun Wordmark(cell: Dp = 5.dp, modifier: Modifier = Modifier) {
    Canvas(modifier.size(cell * (DotFont.cells("n0te") + 3), cell * 7)) {
        val c = cell.toPx()
        with(DotFont) { drawDots("n0te", Offset.Zero, c, Ui.Chalk) }
        drawCircle(Ui.Red, c * 0.62f, Offset((DotFont.cells("n0te") + 1.6f) * c, c * 1.6f))
    }
}
