package dev.n0bridge.n0te

import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.n0te_ffi.FfiBackground
import uniffi.n0te_ffi.FfiBrush
import uniffi.n0te_ffi.FfiOutlineRow
import uniffi.n0te_ffi.FfiSheetSpec
import uniffi.n0te_ffi.NoteLibrary
import uniffi.n0te_ffi.NotebookHandle
import java.nio.file.Files

/**
 * What the app (NotebookModel) takes for granted about the core, checked against a laptop build
 * of it: its undo steps are a sheet's own and local, a refresh merges another writer's changes
 * without losing unsaved ones, a stroke only moved needs no reload, the body box it shows while
 * typing is the core's, sample data has the layout it reads.
 */
class CoreContractTest {
    private val dir = Files.createTempDirectory("n0te-contract").toFile()
    private val a4 = FfiSheetSpec(595f, 842f, FfiBackground.GRID, 20f, null, 0u)
    private val pen = FfiBrush(2f, 0.6f, 0xFF111111u, 0u)

    @After fun cleanUp() { dir.deleteRecursively() }

    private fun line(x0: Float, y0: Float, x1: Float, y1: Float): ByteArray {
        val n = 6
        return bytesOf(FloatArray(4 * n) { i ->
            val k = (i / 4) / (n - 1f)
            when (i % 4) { 0 -> x0 + (x1 - x0) * k; 1 -> y0 + (y1 - y0) * k; 2 -> 0.5f; else -> (i / 4) * 8f }
        })
    }

    private fun notebook(): Pair<NotebookHandle, String> {
        val nb = NoteLibrary.open(dir.absolutePath).createNotebook("Vertrag")
        val ids = nb.addSheet(null, 0u, a4)
        nb.save()
        return nb to ids[1]
    }

    @Test fun undoAndRedoAreTheSheetsOwnSteps() {
        val (nb, sid) = notebook()
        val other = nb.openSheet(nb.addSheet(null, UInt.MAX_VALUE, a4)[1])
        val h = nb.openSheet(sid)
        assertFalse("nothing to undo yet", h.undo())
        val s = h.addStroke(pen, line(100f, 100f, 200f, 120f))
        other.addStroke(pen, line(10f, 10f, 50f, 10f))
        // Undo on one sheet takes back that sheet's edit, never the other's (the app keeps one
        // list of steps naming the sheets).
        assertTrue(h.undo())
        assertEquals(0, h.allStrokes().size)
        assertEquals(1, other.allStrokes().size)
        assertTrue(h.redo())
        assertEquals(listOf(s.id), h.allStrokes().map { it.id })
        assertFalse("redone: nothing more to redo", h.redo())
    }

    @Test fun aRefreshKeepsUnsavedStrokesAndUndoStaysInOrder() {
        val (nb, sid) = notebook()
        val mine = nb.openSheet(sid)
        val saved = mine.addStroke(pen, line(100f, 100f, 200f, 100f))
        mine.save()
        val local = mine.addStroke(pen, line(100f, 300f, 200f, 300f)) // not saved yet
        assertFalse("nothing new on disk", mine.refresh())
        assertEquals(setOf(saved.id, local.id), mine.allStrokes().map { it.id }.toSet())
        // The app's undo list has only its own edits, in order: the unsaved stroke first.
        assertTrue(mine.undo())
        assertEquals(listOf(saved.id), mine.allStrokes().map { it.id })
    }

    @Test fun twoHandlesOfOnePageBothKeepTheirStrokes() {
        // Two handles of one page (the page view and the handwriting reader) both writing: the
        // core gives the second its own peer id, so neither save collides and nothing is lost
        // (before 0.35 they collided and the page's later saves failed). NotebookModel still
        // keeps one handle per page, which saves the second load.
        val (nb, sid) = notebook()
        val reader = nb.openSheet(sid)
        val view = nb.openSheet(sid)
        reader.addStroke(pen, line(10f, 10f, 90f, 10f)); reader.saveQuiet()
        view.addStroke(pen, line(10f, 50f, 90f, 50f))
        view.save()
        reader.addStroke(pen, line(10f, 90f, 90f, 90f)); reader.save()
        assertEquals(3, nb.openSheet(sid).allStrokes().size)
        // One handle for both: everything is kept too.
        val (nb2, sid2) = notebook()
        val one = nb2.openSheet(sid2)
        one.addStroke(pen, line(10f, 10f, 90f, 10f)); one.saveQuiet()
        one.addStroke(pen, line(10f, 50f, 90f, 50f)); one.save()
        assertEquals(2, nb2.openSheet(sid2).allStrokes().size)
    }

    @Test fun movingWithoutScalingMovesTheOutlineAsTheAppDoes() {
        // NotebookModel.transform keeps its own strokes moved by (dx, dy) instead of reloading them.
        val (nb, sid) = notebook()
        val h = nb.openSheet(sid)
        val s = h.addStroke(pen, line(100f, 100f, 220f, 160f))
        val before = floatsOf(s.outline)
        val dx = 37.25f; val dy = -12.5f
        h.transform(listOf(s.id), dx, dy, 1f, 0f, 0f)
        val after = h.strokesById(listOf(s.id)).single()
        val moved = floatsOf(after.outline)
        assertEquals(before.size, moved.size)
        for (i in before.indices) assertEquals("outline value $i", before[i] + if (i % 2 == 0) dx else dy, moved[i], 0.05f)
        for (i in 0 until 4) assertEquals("bounds $i", s.bounds[i] + if (i % 2 == 0) dx else dy, after.bounds[i], 0.05f)
        if (s.ends.size >= 9) {
            assertEquals(s.ends[0] + dx, after.ends[0], 0.05f)
            assertEquals(s.ends[5] + dy, after.ends[5], 0.05f)
            assertEquals("direction unchanged", s.ends[2], after.ends[2], 1e-3f)
        }
    }

    @Test fun theBodyBoxShownWhileTypingIsTheCores() {
        val nb = NoteLibrary.open(dir.absolutePath).createNotebook("Text")
        for (w in listOf(595f, 842f, 150f)) {
            val ids = nb.addSheet(null, UInt.MAX_VALUE, FfiSheetSpec(w, 842f, FfiBackground.TEXT, 20f, null, 0u))
            val h = nb.openSheet(ids[1])
            h.setPageText("Hallo **Welt**")
            val core = h.textBoxesWithBody(w).single { it.id == BODY_ID }
            assertEquals("page $w wide", core, bodyBox(w, core.text))
        }
    }

    @Test fun strokeDataHasSixteenBytesPerSample() {
        // NotebookModel.strokeEnds reads the first and last sample at 16-byte steps.
        val (nb, sid) = notebook()
        val h = nb.openSheet(sid)
        val s = h.addStroke(pen, line(40f, 50f, 300f, 400f))
        val data = h.strokeData(listOf(s.id)).single()
        assertEquals(0, data.samples.size % 16)
        val f = floatsOf(data.samples)
        assertEquals(40f, f[0], 0.05f); assertEquals(50f, f[1], 0.05f)
        assertEquals(300f, f[f.size - 4], 0.05f); assertEquals(400f, f[f.size - 3], 0.05f)
        // Pasting the data back (another sheet, moved) gives the same stroke there.
        val pasted = nb.openSheet(nb.addSheet(null, UInt.MAX_VALUE, a4)[1]).addStrokes(listOf(data), 10f, 20f, 1f, 0f, 0f).single()
        assertEquals(s.bounds[0] + 10f, pasted.bounds[0], 0.05f)
        assertEquals(s.bounds[1] + 20f, pasted.bounds[1], 0.05f)
    }

    @Test fun theAreaEraserCountsAndUndoBringsItBack() {
        val (nb, sid) = notebook()
        val h = nb.openSheet(sid)
        h.addStroke(pen, line(100f, 100f, 300f, 100f))
        h.addStroke(pen, line(100f, 500f, 300f, 500f))
        assertEquals(0u, h.eraseArea(bytesOf(floatArrayOf(50f, 700f, 10f, 60f, 700f, 10f))))
        assertTrue(h.eraseArea(bytesOf(floatArrayOf(200f, 90f, 15f, 200f, 110f, 15f))) > 0u)
        val left = h.allStrokes()
        assertTrue("the far stroke stays", left.any { it.bounds[1] > 400f })
        assertTrue(h.undo())
        assertEquals(2, h.allStrokes().size)
    }

    @Test fun aDeletedPageComesBackWithItsSidePagesAndInk() {
        val (nb, sid) = notebook()
        val row = nb.outline(true).single() as FfiOutlineRow.Sheet
        nb.addSideSheet(row.node, true, UInt.MAX_VALUE, a4)
        nb.openSheet(sid).apply { addStroke(pen, line(10f, 10f, 90f, 90f)); save() }
        nb.deleteNode(row.node)
        assertTrue(nb.outline(true).isEmpty())
        assertTrue(nb.undoDelete())
        val back = nb.outline(true).single() as FfiOutlineRow.Sheet
        assertEquals(sid, back.sheetId)
        assertEquals(1, back.right.size)
        assertEquals(1, nb.openSheet(sid).allStrokes().size)
        assertFalse("nothing more to bring back", nb.undoDelete())
    }

    @Test fun spaceInsertedAndTheHeightGoBackSeparately() {
        // NotebookModel.insertSpace: shift_below is the sheet's undo step, the height is put back by reshape.
        val (nb, sid) = notebook()
        val h = nb.openSheet(sid)
        val s = h.addStroke(pen, line(100f, 800f, 200f, 800f))
        val plan = uniffi.n0te_ffi.planSpace(a4, 700f, 120f, h.below(700f))
        assertEquals(120f, plan.dy)
        assertTrue("the ink below pushed past the edge: longer page", plan.height > 842f)
        assertEquals(1u, h.shiftBelow(700f, plan.dy))
        val node = (nb.outline(true).single() as FfiOutlineRow.Sheet).node
        nb.reshape(node, plan.height, plan.gaps)
        assertEquals(s.bounds[1] + 120f, h.strokesById(listOf(s.id)).single().bounds[1], 0.05f)
        assertTrue(h.undo())
        assertEquals(s.bounds[1], h.strokesById(listOf(s.id)).single().bounds[1], 0.05f)
        nb.reshape(node, 842f, emptyList())
        assertEquals(842f, (nb.outline(true).single() as FfiOutlineRow.Sheet).spec.height)
    }

    @Test fun headingsAreUndoSteps() {
        val (nb, sid) = notebook()
        val h = nb.openSheet(sid)
        val s = h.addStroke(pen, line(60f, 60f, 260f, 60f))
        val id = h.addHeading("Kapitel 1", listOf(s.id))
        assertNotNull(h.headings().singleOrNull { it.id == id })
        nb.save(); h.save()
        assertEquals("Kapitel 1", nb.headings().single().text)
        assertTrue(h.undo())
        assertTrue(h.headings().isEmpty())
        assertEquals("the stroke stays", 1, h.allStrokes().size)
    }

}
