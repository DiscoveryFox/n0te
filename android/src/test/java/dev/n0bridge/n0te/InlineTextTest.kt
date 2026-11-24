package dev.n0bridge.n0te

import org.junit.Assert.assertEquals
import org.junit.Test

class InlineTextTest {
    @Test
    fun pairsInlineMarks() {
        assertEquals(listOf(Triple(2, 10, "**"), Triple(11, 16, "*")), InlineTextEditor.inlineRanges("a **bold** *ita* x"))
        val r = InlineTextEditor.inlineRanges("x `code` and \$m\$ and *open")
        assertEquals(listOf("`", "$"), r.map { it.third })
        assertEquals(2, r[0].first); assertEquals(8, r[0].second)
    }

    @Test
    fun knowsParagraphKinds() {
        val s = "# A\n## B\n### C\n- item\n- [ ] todo\n1. first\n> quote\nplain"
        val kinds = s.split('\n').scan(0) { at, line -> at + line.length + 1 }.dropLast(1).map { InlineTextEditor.kindOf(s, it) }
        assertEquals(listOf("h1", "h2", "h3", "ul", "todo", "ol", "quote", "p"), kinds)
    }

    @Test
    fun findsLines() {
        val s = "ab\ncd\n\nef"
        assertEquals(3, InlineTextEditor.lineStart(s, 4))
        assertEquals(5, InlineTextEditor.lineEnd(s, 3))
        assertEquals(6, InlineTextEditor.lineStart(s, 6))
        assertEquals(6, InlineTextEditor.lineEnd(s, 6))
    }

    @Test
    fun stylesEveryKindOfParagraphWithoutCrashing() {
        val md = "# Title\n## Sub **b**\n### Small\n- item *i*\n- [ ] todo\n- [x] done\n1. first\n> quote\n```\ncode\n```\n\$x^2\$ and \$\$\\int f\$\$\nplain `c`"
        for (cursor in -1..13) {
            val runs = InlineTextEditor.styleRuns(md, cursor)
            runs.forEach { r -> assert(r.start in 0..md.length && r.end in r.start..md.length) { "run out of range: $r" } }
        }
        val runs = InlineTextEditor.styleRuns(md, -1)
        // Headings: big and bold, their "# " hidden off the cursor's line.
        assert(runs.any { it.start == 0 && it.style == InlineTextEditor.Style.Size(1.6f) })
        assert(runs.any { it.start == 0 && it.end == 2 && it.style == InlineTextEditor.Style.Hide })
        // Formulas are drawn off the cursor's line.
        val math = runs.mapNotNull { it.style as? InlineTextEditor.Style.Math }
        assertEquals(listOf("x^2" to false, "\\int f" to true), math.map { it.latex to it.display })
    }

    @Test
    fun theCursorLineShowsSourceAndFaintMarks() {
        val md = "# Title\nsum \$a+b\$"
        val onHeading = InlineTextEditor.styleRuns(md, 0)
        assert(onHeading.none { it.end <= 2 && it.style == InlineTextEditor.Style.Hide }) { "marks visible on the cursor's line" }
        val onFormula = InlineTextEditor.styleRuns(md, 1)
        assert(onFormula.none { it.style is InlineTextEditor.Style.Math }) { "source shown while editing the formula" }
    }

    @Test
    fun headingLevels() {
        assertEquals(listOf(1, 2, 3, 0, 0), listOf("# a", "## b", "### c", "#### d", "#no").map { InlineTextEditor.headingLevel(it) })
    }

    @Test
    fun aLineWithOnlyADisplayFormulaIsCentred() {
        val md = "Text \$x\$\n\$\$\\int_0^1 x\\,dx\$\$"
        val on = InlineTextEditor.styleRuns(md, -1, centerDisplay = true)
        val centred = on.filter { it.style == InlineTextEditor.Style.Center }
        assertEquals(1, centred.size)
        assertEquals(md.indexOf('\n') + 1, centred[0].start)
        assert(InlineTextEditor.styleRuns(md, -1, centerDisplay = false).none { it.style == InlineTextEditor.Style.Center })
    }
}
