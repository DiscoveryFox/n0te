package dev.n0bridge.n0te

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.n0te_ffi.FfiBackground
import uniffi.n0te_ffi.plotAdd
import uniffi.n0te_ffi.plotInfo
import uniffi.n0te_ffi.plotStrokes
import uniffi.n0te_ffi.plotToPage

/** New coordinate systems: where they go, their numbers, and the core reading functions. */
class PlotsTest {
    private val sq = 14.173f // 5 mm squares

    @Test fun onSquaredPaperItSitsOnTheSquares() {
        val p = Plots.fresh(300f, 400f, 595.28f, 841.89f, FfiBackground.GRID, sq)
        assertEquals(12 * sq, p.w, 1e-3f)
        assertEquals(8 * sq, p.h, 1e-3f)
        // Corner on a line, so the axes (through the middle) are on lines too: a square is a unit.
        assertEquals(0f, p.x / sq - Math.round(p.x / sq), 1e-3f)
        assertEquals(0f, p.y / sq - Math.round(p.y / sq), 1e-3f)
        assertEquals(-6.0, p.xMin, 1e-9); assertEquals(6.0, p.xMax, 1e-9)
        assertEquals(-4.0, p.yMin, 1e-9); assertEquals(4.0, p.yMax, 1e-9)
        assertTrue("no grid of its own on squared paper", !p.grid)
        // Pushed to the edge it keeps a square's room for the arrow heads and numbers.
        val edge = Plots.fresh(590f, 830f, 595.28f, 841.89f, FfiBackground.GRID, sq)
        assertTrue(edge.x + edge.w <= 595.28f - sq + 1e-3f && edge.y + edge.h <= 841.89f - sq + 1e-3f)
        // Plain paper: 20 pt to the unit, a grid of its own; a narrow page gets fewer units.
        val plain = Plots.fresh(150f, 150f, 300f, 400f, FfiBackground.BLANK, 0f)
        assertTrue(plain.grid)
        assertTrue(plain.w <= 300f * 0.85f)
        assertEquals((plain.xMax - plain.xMin) * 20.0, plain.w.toDouble(), 1e-3)
    }

    @Test fun aGraphMovedOrZoomedByHandSnapsToTheSquares() {
        val p = Plots.fresh(300f, 400f, 595.28f, 841.89f, FfiBackground.GRID, sq)
        fun perSquare(q: uniffi.n0te_ffi.FfiPlot) = (q.xMax - q.xMin) / q.w * sq
        fun onLine(v: Double) = assertEquals(0.0, v / sq - Math.round(v / sq), 1e-6)
        // Already on the squares: stays as it is.
        assertEquals(p, GraphSession.aligned(p, sq, offsets = true))
        // Pushed a bit and zoomed a little: back to a square a unit, 0 on the page's lines.
        val f = 1.25
        val moved = p.copy(xMin = p.xMin * f + 0.37, xMax = p.xMax * f + 0.37, yMin = p.yMin * f - 0.2, yMax = p.yMax * f - 0.2)
        val a = GraphSession.aligned(moved, sq, offsets = true)
        assertEquals(1.0, perSquare(a), 1e-9)
        assertEquals(1.0, (a.yMax - a.yMin) / a.h * sq, 1e-9)
        onLine(a.x + (0 - a.xMin) / ((a.xMax - a.xMin) / a.w))
        onLine(a.y + a.yMax / ((a.yMax - a.yMin) / a.h))
        // Zoomed out a lot: two units to a square (1, 2, 5 …), still on the lines.
        val out = GraphSession.aligned(p.copy(xMin = p.xMin * 2.3, xMax = p.xMax * 2.3, yMin = p.yMin * 2.3, yMax = p.yMax * 2.3), sq, offsets = true)
        assertEquals(2.0, perSquare(out), 1e-9)
        onLine(out.x + (0 - out.xMin) / ((out.xMax - out.xMin) / out.w))
        // Zoomed in: half a unit.
        assertEquals(0.5, perSquare(GraphSession.aligned(p.copy(xMin = p.xMin * 0.45, xMax = p.xMax * 0.45, yMin = p.yMin * 0.45, yMax = p.yMax * 0.45), sq, offsets = false)), 1e-9)
    }

    @Test fun rangeNumbersAreGerman() {
        assertEquals("-3", Plots.format(-3.0))
        assertEquals("2,5", Plots.format(2.5))
        assertEquals(-2.5, Plots.number(" −2,5 ")!!, 1e-12)
        assertNull(Plots.number("abc"))
    }

    @Test fun theCoreBuildsConstructionsLikeGeoGebra() {
        var p = Plots.fresh(300f, 400f, 595.28f, 841.89f, FfiBackground.GRID, sq).copy(id = "t")
        for (i in listOf("A = (1, 2)", "(4, 6)", "f(x) = x^2", "Gerade(A, B)", "Schneide(f, g)")) p = plotAdd(p, i)
        val info = plotInfo(p)
        assertEquals("two intersections, two points", listOf("A", "B", "f", "g", "C", "D"), info.map { it.name })
        assertEquals("Punkt", info[0].kind)
        assertTrue(info[0].draggable)
        assertEquals("f(x) = x²", info[2].shown)
        assertTrue(info.all { it.error == null })
        // New letters become sliders, like in GeoGebra; the typed object is the one selected.
        val withSliders = GraphSession("sheet", Plots.fresh(300f, 400f, 595.28f, 841.89f, FfiBackground.GRID, sq).copy(id = "t"), true) {}
        assertNull(withSliders.input("b = 4 + 3a"))
        assertEquals(listOf("a", "b"), plotInfo(withSliders.draft).map { it.name })
        assertEquals("b", withSliders.nameOf(withSliders.selected!!))
        // A wrong input says why.
        val err = runCatching { plotAdd(p, "Gerade(A)") }.exceptionOrNull()
        assertTrue(err?.message?.isNotBlank() == true)
        // Strokes for the page.
        assertTrue(plotStrokes(p).size > 20)
        // The tools: a line through two taps (new points on the grid).
        var changed = 0
        val s = GraphSession("sheet", Plots.fresh(300f, 400f, 595.28f, 841.89f, FfiBackground.GRID, sq).copy(id = "t"), true) { changed++ }
        s.choose(GTool.LINE)
        val a = plotToPage(s.draft, 1.0, 1.0); val b = plotToPage(s.draft, 3.0, 2.0)
        s.tap(a[0], a[1], null)
        s.tap(b[0], b[1], null)
        val names = s.infos.map { it.name to it.kind }
        assertEquals(listOf("A" to "Punkt", "B" to "Punkt", "f" to "Gerade"), names)
        s.undo()
        assertEquals(2, s.infos.size)
        assertTrue(changed > 0)
    }

    @Test fun theEditorGuidesStepByStepAndOffersToTakeBack() {
        val s = GraphSession("sheet", Plots.fresh(300f, 400f, 595.28f, 841.89f, FfiBackground.GRID, sq).copy(id = "t"), true) {}
        // Steps: what to tap next, step i of n.
        s.choose(GTool.CIRCLE)
        assertEquals("Mittelpunkt antippen oder setzen", s.step)
        assertEquals(1 to 2, s.stepOf)
        val a = plotToPage(s.draft, 0.0, 0.0)
        s.tap(a[0], a[1], null)
        assertEquals("Punkt auf dem Kreis antippen oder setzen", s.step)
        assertEquals(2 to 2, s.stepOf)
        s.tap(plotToPage(s.draft, 2.0, 0.0)[0], plotToPage(s.draft, 2.0, 0.0)[1], null)
        assertEquals(listOf("A", "B", "c"), s.infos.map { it.name })
        assertTrue("new tools go onto the bar", GTool.CIRCLE in GraphPrefs.recent)
        // Made from others: the list shows how; free ones don't repeat themselves.
        assertEquals("Kreis(A, B)", s.infos[2].definition)
        assertEquals("", s.infos[0].definition)
        // Deleting says what went with it and can be taken back.
        s.choose(GTool.MOVE)
        s.remove(s.infos[0].id)
        assertEquals("A gelöscht, mit c", s.notice)
        assertEquals(listOf("B"), s.infos.map { it.name })
        s.undo()
        assertNull(s.notice)
        assertEquals(3, s.infos.size)
        // Nullstellen: one point per zero.
        s.input("f(x) = x^2 - 4")
        s.choose(GTool.ROOTS)
        val f = s.infos.first { it.name == "f" }
        s.tap(0f, 0f, f.id)
        assertEquals(2, s.infos.count { it.definition.startsWith("Nullstelle(f") })
        assertNull(s.error)
        // A tool that needs an object says so when there is none.
        s.tap(0f, 0f, null)
        assertTrue(s.error!!.isNotBlank())
        // The view goes back to how it started.
        val x0 = s.draft.xMin
        s.view { it.copy(xMin = it.xMin - 3, xMax = it.xMax - 3) }
        s.resetView()
        assertEquals(x0, s.draft.xMin, 1e-9)
    }
}
