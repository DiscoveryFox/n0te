package dev.n0bridge.n0te

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/** Lines of handwriting found from the strokes' boxes alone (what the recogniser then reads). */
class InkLinesTest {
    private var n = 0
    private fun box(x0: Float, y0: Float, x1: Float, y1: Float) = InkLines.Box("s${n++}", x0, y0, x1, y1)

    /** A word of [letters] letters from [x] on a line at [y] (letters 7 wide, 12 tall), and an i-dot. */
    private fun word(x: Float, y: Float, letters: Int, dot: Boolean = false): List<InkLines.Box> =
        (0 until letters).map { box(x + it * 8f, y + (if (it % 3 == 0) -4f else 0f), x + it * 8f + 7f, y + 12f) } +
            (if (dot) listOf(box(x + 2f, y - 6f, x + 3.5f, y - 4.5f)) else emptyList())

    @Test fun twoLinesTwoColumnsAndNoDrawings() {
        val line1 = word(60f, 100f, 6, dot = true) + word(120f, 101f, 4)
        val line2 = word(60f, 130f, 5) + word(110f, 129f, 7, dot = true)
        // Far to the right on the first line's height: another column.
        val column = word(400f, 100f, 5)
        // A long rule and a big drawing are no writing.
        val rule = box(60f, 160f, 500f, 162f)
        val drawing = box(300f, 200f, 420f, 320f)
        val lines = InkLines.group(line1 + line2 + column + rule + drawing)
        assertEquals("two lines and a column: ${lines.map { it.size }}", 3, lines.size)
        val ids = lines.map { l -> l.map { it.id }.toSet() }
        assertTrue("line 1 with its i-dot", ids.any { it == line1.map { b -> b.id }.toSet() })
        assertTrue("line 2 with its i-dot", ids.any { it == line2.map { b -> b.id }.toSet() })
        assertTrue(ids.any { it == column.map { b -> b.id }.toSet() })
        assertTrue(ids.none { rule.id in it || drawing.id in it })
        // The same strokes: the same name (read once).
        assertEquals(InkLines.key(line1), InkLines.key(line1.reversed()))
    }

    @Test fun aHeldStrokeTakesItsWord() {
        val first = word(60f, 100f, 6, dot = true)
        val second = word(120f, 101f, 4)
        val all = first + second + word(60f, 130f, 5)
        // A letter of the first word: the whole first word (with its i-dot), not the next one.
        assertEquals(first.map { it.id }.toSet(), InkLines.word(all, first[2].id).toSet())
        assertEquals(second.map { it.id }.toSet(), InkLines.word(all, second[1].id).toSet())
        // A drawing is a word of its own.
        val drawing = box(300f, 200f, 420f, 320f)
        assertEquals(listOf(drawing.id), InkLines.word(all + drawing, drawing.id))
    }
}
