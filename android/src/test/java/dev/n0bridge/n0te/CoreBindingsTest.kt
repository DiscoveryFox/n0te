package dev.n0bridge.n0te

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.n0te_ffi.FfiBackground
import uniffi.n0te_ffi.FfiBrush
import uniffi.n0te_ffi.FfiOutlineRow
import uniffi.n0te_ffi.FfiSheetSpec
import uniffi.n0te_ffi.NoteLibrary
import uniffi.n0te_ffi.strokeOutline
import java.nio.ByteBuffer
import java.nio.ByteOrder
import java.nio.file.Files

/** The generated Kotlin bindings against a laptop build of the Rust core (same code as on the tablet). */
class CoreBindingsTest {
    private fun pack(vararg f: Float): ByteArray =
        ByteBuffer.allocate(f.size * 4).order(ByteOrder.LITTLE_ENDIAN).apply { asFloatBuffer().put(f) }.array()

    @Test
    fun notebookSheetsStrokesRoundTrip() {
        val dir = Files.createTempDirectory("n0te-bindings").toFile()
        try {
            val lib = NoteLibrary.open(dir.absolutePath)
            val nb = lib.createNotebook("Mathe")
            val a4 = FfiSheetSpec(595f, 842f, FfiBackground.GRID, 20f, null, 0u)
            val group = nb.addGroup(null, 0u, "Woche 01")
            val ids = nb.addSheet(group, 0u, a4)
            nb.addSideSheet(ids[0], true, 0u, a4)
            nb.save()

            val rows = nb.outline(true)
            assertEquals(2, rows.size)
            val sheetRow = rows[1] as FfiOutlineRow.Sheet
            assertEquals(1, sheetRow.right.size)
            assertEquals(FfiBackground.GRID, sheetRow.spec.background)

            val sheet = nb.openSheet(ids[1])
            val brush = FfiBrush(2f, 0.6f, 0xFFD71921u, 0u)
            val stroke = sheet.addStroke(brush, pack(10f, 10f, 0.5f, 0f, 20f, 12f, 0.6f, 8f, 30f, 15f, 0.7f, 16f))
            assertTrue("outline has vertices", stroke.outline.size >= 8 * 8)
            assertEquals(0xFFD71921u, stroke.brush.color)
            sheet.save()

            // Reopen from disk through a fresh library handle.
            val again = NoteLibrary.open(dir.absolutePath).loadNotebook(nb.id())
            assertEquals("Mathe", again.title())
            assertEquals(1, again.openSheet(ids[1]).allStrokes().size)
            assertEquals(0, again.openSheet(ids[1]).strokesIn(500f, 500f, 600f, 600f).size)

            val live = strokeOutline(brush, pack(0f, 0f, 0.5f, 0f))
            assertEquals(16 * 8, live.size) // a dot: 16 vertices
        } finally {
            dir.deleteRecursively()
        }
    }

    /** Space sideways: what lies right of the line moves, the page gets wider; with nothing below, down makes the page longer. */
    @Test
    fun spaceGoesInSidewaysAndBelowEmptyPaper() {
        val dir = Files.createTempDirectory("n0te-space").toFile()
        try {
            val nb = NoteLibrary.open(dir.absolutePath).createNotebook("Platz")
            val a4 = FfiSheetSpec(595f, 842f, FfiBackground.GRID, 20f, null, 0u)
            val ids = nb.addSheet(null, 0u, a4)
            val sheet = nb.openSheet(ids[1])
            val brush = FfiBrush(2f, 0f, 0xFF000000u, 0u)
            sheet.addStroke(brush, pack(500f, 100f, 0.5f, 0f, 560f, 120f, 0.5f, 8f))
            val right = sheet.rightOf(200f)
            assertTrue(right.bottom!! > 559f)
            val plan = uniffi.n0te_ffi.planSpaceX(a4, 200f, 80f, right)
            assertEquals(80f, plan.dx)
            assertTrue("pushed past the edge: wider", plan.width > 595f)
            assertEquals(1u, sheet.shiftRightOf(200f, plan.dx))
            nb.setSize(ids[0], plan.width, 842f)
            assertEquals(plan.width, (nb.outline(true)[0] as FfiOutlineRow.Sheet).spec.width)
            // Down where nothing is below: the page is longer by the space.
            val down = uniffi.n0te_ffi.planSpace(a4, 800f, 60f, sheet.below(800f))
            assertEquals(902f, down.height)
        } finally {
            dir.deleteRecursively()
        }
    }

    /** Formats come from the core (the same list as on the laptop); a page turns and takes any size. */
    @Test
    fun pageFormatsAndTurningAPage() {
        val formats = uniffi.n0te_ffi.pageFormats()
        assertEquals("a4", formats.first().id)
        assertTrue("upright", formats.all { it.width <= it.height })
        assertEquals("A4 quer", uniffi.n0te_ffi.describePageSize(842f, 595f))
        assertEquals("a5", uniffi.n0te_ffi.pageFormatId(595f, 420f))
        assertEquals("", uniffi.n0te_ffi.pageFormatId(Formats.pt(200f), Formats.pt(100f)))
        assertEquals(842f to 595f, Formats.oriented(595f, 842f, true))
        val dir = Files.createTempDirectory("n0te-format").toFile()
        try {
            val nb = NoteLibrary.open(dir.absolutePath).createNotebook("Formate")
            val ids = nb.addSheet(null, 0u, FfiSheetSpec(595f, 842f, FfiBackground.LINES, 20f, null, 0u))
            nb.setSize(ids[0], 842f, 595f)
            val spec = (nb.outline(true)[0] as FfiOutlineRow.Sheet).spec
            assertEquals(842f to 595f, spec.width to spec.height)
            assertEquals(FfiBackground.LINES, spec.background)
            assertTrue("too small is refused", runCatching { nb.setSize(ids[0], 10f, 595f) }.isFailure)
        } finally {
            dir.deleteRecursively()
        }
    }
}
