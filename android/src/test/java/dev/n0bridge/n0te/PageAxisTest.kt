package dev.n0bridge.n0te

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.n0te_ffi.FfiBackground
import uniffi.n0te_ffi.FfiSheetSpec
import uniffi.n0te_ffi.plotAxisAt
import uniffi.n0te_ffi.plotAxisDragged
import uniffi.n0te_ffi.plotToPage

/** An interactive graph's axis stretched right on the page. */
class PageAxisTest {
    private val sq = 14.173f

    @Test fun thePenGrabsAnAxisAFingerOnlyWhereFingersWrite() {
        val pen = NotebookView.Tool.PEN
        fun ok(stylus: Boolean = true, erasing: Boolean = false, tool: NotebookView.Tool = pen, penOnly: Boolean = true, nav: Boolean = false, palm: Boolean = false, near: Boolean = false) =
            PageAxis.allowed(stylus, erasing, tool, penOnly, nav, palm, near)
        assertTrue(ok())
        assertTrue(ok(tool = NotebookView.Tool.MARKER))
        assertFalse("erasing erases", ok(erasing = true))
        assertFalse("the lasso selects", ok(tool = NotebookView.Tool.LASSO))
        assertFalse(ok(tool = NotebookView.Tool.ERASER))
        assertFalse("only looking", ok(nav = true))
        // Fingers: only where they write, never a palm or with the pen near.
        assertFalse("pen-only: fingers move the page", ok(stylus = false))
        assertTrue(ok(stylus = false, penOnly = false))
        assertFalse(ok(stylus = false, penOnly = false, palm = true))
        assertFalse(ok(stylus = false, penOnly = false, near = true))
        assertTrue("the pen isn't a palm", ok(palm = true, near = true))
        // The pen's point needs less room than a finger.
        assertTrue(PageAxis.tolerance(true, 2f, 1f) < PageAxis.tolerance(false, 2f, 1f))
        assertEquals(7f, PageAxis.tolerance(true, 2f, 2f), 1e-6f)
    }

    @Test fun aStretchedAxisSnapsToTheSquaresWhenPutDown() {
        val spec = FfiSheetSpec(595.28f, 841.89f, FfiBackground.GRID, sq, null, 0u, emptyList())
        assertEquals(sq, PageAxis.squares(spec), 0f)
        assertEquals(0f, PageAxis.squares(spec.copy(background = FfiBackground.LINES)), 0f)
        assertEquals("a board", 0f, PageAxis.squares(spec.copy(width = 0f, height = 0f)), 0f)
        val p = Plots.fresh(300f, 400f, 595.28f, 841.89f, FfiBackground.GRID, sq)
        // Grab the x axis at 3 and pull it out a bit past where 6 was: x stretched (about 0.48
        // units to a square), y as it was.
        val at3 = plotToPage(p, 3.0, 0.0); val at6 = plotToPage(p, 6.0, 0.0)
        assertEquals(1, plotAxisAt(p, at3[0], at3[1] + 1f, 4f).toInt())
        val s = plotAxisDragged(p, 1.toUByte(), at3[0], at3[0] + (at6[0] - at3[0]) * 1.1f)
        assertEquals(3.0 / 6.3, (s.xMax - s.xMin) / (p.xMax - p.xMin), 1e-3)
        assertEquals(p.yMin, s.yMin, 1e-9); assertEquals(p.yMax, s.yMax, 1e-9)
        // Put down: half a unit to a square across (the nearest of … ½, 1, 2 …), 0 on a line, y untouched.
        val put = PageAxis.settled(s, sq)
        assertEquals(0.5, (put.xMax - put.xMin) / put.w * sq, 1e-9)
        val zeroX = put.x + (0 - put.xMin) / ((put.xMax - put.xMin) / put.w)
        assertEquals(0.0, zeroX / sq - Math.round(zeroX / sq), 1e-6)
        assertEquals(p.yMin, put.yMin, 1e-9); assertEquals(p.yMax, put.yMax, 1e-9)
        // Not on squared paper (or not kept to it): as stretched.
        assertEquals(s, PageAxis.settled(s, 0f))
        assertEquals(s, PageAxis.settled(s.copy(align = false), sq).copy(align = true))
    }
}
