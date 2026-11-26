package dev.n0bridge.n0te

import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.n0te_ffi.FfiBrush
import uniffi.n0te_ffi.FfiOutlineRow
import uniffi.n0te_ffi.FfiTile
import uniffi.n0te_ffi.FfiTileRequest
import uniffi.n0te_ffi.NoteLibrary
import uniffi.n0te_ffi.NotebookHandle
import uniffi.n0te_ffi.PdfService
import uniffi.n0te_ffi.TileSink
import uniffi.n0te_ffi.pdfPagePixels
import java.io.ByteArrayOutputStream
import java.io.File
import java.nio.ByteBuffer
import java.nio.ByteOrder
import java.nio.file.Files
import java.util.concurrent.LinkedBlockingQueue
import java.util.concurrent.TimeUnit

/**
 * PDF import, tiles, search and export through the generated bindings, against a laptop build of
 * the Rust core and the linux-x64 libpdfium from n0te/third_party (scripts/fetch-pdfium.sh).
 */
class PdfBindingsTest {
    private fun pdfium(): String =
        System.getProperty("n0te.pdfium") ?: File("../../n0te/third_party/pdfium/linux-x64/lib/libpdfium.so").absolutePath

    private fun pack(vararg f: Float): ByteArray =
        ByteBuffer.allocate(f.size * 4).order(ByteOrder.LITTLE_ENDIAN).apply { asFloatBuffer().put(f) }.array()

    /** Two 400 x 300 pt pages: a blue box and "Hallo n0te"; the second page is turned by /Rotate 90. */
    private fun tinyPdf(): ByteArray {
        fun stream(s: String) = "<</Length ${s.length}>>stream\n$s\nendstream"
        val objects = listOf(
            "<</Type/Catalog/Pages 2 0 R>>",
            "<</Type/Pages/Kids[3 0 R 5 0 R]/Count 2>>",
            "<</Type/Page/Parent 2 0 R/MediaBox[0 0 400 300]/Contents 4 0 R/Resources<</Font<</F1 7 0 R>>>>>>",
            stream("0 0.4 0.8 rg 40 40 80 60 re f BT /F1 24 Tf 40 200 Td (Hallo n0te) Tj ET"),
            "<</Type/Page/Parent 2 0 R/MediaBox[0 0 400 300]/Rotate 90/Contents 6 0 R/Resources<<>>>>",
            stream("1 0 0 rg 10 10 50 50 re f"),
            "<</Type/Font/Subtype/Type1/BaseFont/Helvetica>>",
        )
        val out = ByteArrayOutputStream()
        fun put(s: String) = out.write(s.toByteArray(Charsets.ISO_8859_1))
        put("%PDF-1.7\n")
        val offsets = objects.mapIndexed { i, o ->
            out.size().also { put("${i + 1} 0 obj\n$o\nendobj\n") }
        }
        val xref = out.size()
        put("xref\n0 ${objects.size + 1}\n0000000000 65535 f\r\n")
        offsets.forEach { put("%010d 00000 n\r\n".format(it)) }
        put("trailer\n<</Size ${objects.size + 1}/Root 1 0 R>>\nstartxref\n$xref\n%%EOF\n")
        return out.toByteArray()
    }

    /** One A4 page as PDFs really are: 595.276 × 841.89 pt, text up to the right and bottom edge. */
    private fun a4Pdf(): ByteArray {
        fun stream(s: String) = "<</Length ${s.length}>>stream\n$s\nendstream"
        val objects = listOf(
            "<</Type/Catalog/Pages 2 0 R>>",
            "<</Type/Pages/Kids[3 0 R]/Count 1>>",
            "<</Type/Page/Parent 2 0 R/MediaBox[0 0 595.276 841.89]/Contents 4 0 R/Resources<</Font<</F1 5 0 R>>>>>>",
            stream("0 0 0 rg 560 0 35.276 841.89 re f BT /F1 18 Tf 40 800 Td (Hoehere Mathematik) Tj ET"),
            "<</Type/Font/Subtype/Type1/BaseFont/Helvetica>>",
        )
        val out = ByteArrayOutputStream()
        fun put(s: String) = out.write(s.toByteArray(Charsets.ISO_8859_1))
        put("%PDF-1.7\n")
        val offsets = objects.mapIndexed { i, o -> out.size().also { put("${i + 1} 0 obj\n$o\nendobj\n") } }
        val xref = out.size()
        put("xref\n0 ${objects.size + 1}\n0000000000 65535 f\r\n")
        offsets.forEach { put("%010d 00000 n\r\n".format(it)) }
        put("trailer\n<</Size ${objects.size + 1}/Root 1 0 R>>\nstartxref\n$xref\n%%EOF\n")
        return out.toByteArray()
    }

    /** A huge PDF page's whole-page picture stays at most 2048 px a side (memory not growing with the page). */
    @Test
    fun thumbnailsOfHugePagesStaySmall() {
        assertEquals(PDF_THUMB, pdfThumbScale(595.28f, 841.89f), 0f)
        assertEquals("2000 mm: 1417 px as before", PDF_THUMB, pdfThumbScale(5669.3f, 5669.3f), 0f)
        for (side in listOf(8192f, 14400f, 200_000f)) {
            val t = pdfThumbScale(side, side / 3f)
            assertTrue("$side pt at $t", side * t <= 2048f && side * t > 1024f)
        }
    }

    /** The tablet's page size in pixels matches the core's for odd sizes at every zoom bucket. */
    @Test
    fun pagePixelsMatchTheCore() {
        val sizes = listOf(595.276f to 841.89f, 400f to 300f, 842f to 474f, 612f to 792f, 419.53f to 595.28f, 1000.5f to 0.7f)
        var b = 0.125f
        while (b <= 8f) {
            for ((w, h) in sizes) {
                val core = pdfPagePixels(w, h, b)
                assertEquals("$w x $h at $b", core[0].toInt() to core[1].toInt(), pagePixels(w, h, b))
            }
            b *= 2f
        }
    }

    /**
     * The regression from the tablet: at every zoom bucket the last tile column and row of an A4
     * PDF render (the worker refused them when the tablet rounded the page size up).
     */
    @Test
    fun lastTilesOfAnA4PdfRenderAtEveryZoom() {
        val dir = Files.createTempDirectory("n0te-pdf-a4").toFile()
        val tiles = LinkedBlockingQueue<FfiTile>()
        val lib = NoteLibrary.open(dir.absolutePath)
        val pdf = PdfService.start(lib, pdfium(), emptyList(), object : TileSink {
            override fun onTile(tile: FfiTile) { tiles.put(tile) }
        })
        try {
            val nb = lib.createNotebook("A4")
            nb.importPdf(pdf, a4Pdf(), null, 0u, 0u)
            val spec = (nb.outline(true).first { it is FfiOutlineRow.Sheet } as FfiOutlineRow.Sheet).spec
            val blob = spec.pdfBlob!!
            var b = 0.25f
            var gen = 1uL
            while (b <= 8f) {
                val (pw, ph) = pagePixels(spec.width, spec.height, b)
                val x = (pw - 1) / 512 * 512
                val y = (ph - 1) / 512 * 512
                pdf.requestTiles(listOf(FfiTileRequest(blob, 0u, b, x.toUInt(), y.toUInt(), (pw - x).toUInt(), (ph - y).toUInt(), 0u, 0u, gen++)))
                val t = tiles.poll(20, TimeUnit.SECONDS)
                assertNotNull("tile at $b arrives", t)
                assertNull("bottom-right tile at $b (page ${pw}x$ph px)", t!!.error)
                // The black bar along the right edge is in it.
                val tw = t.request.width.toInt()
                assertTrue("right edge drawn at $b", pixel(t, tw - 1, 0)[0] < 60)
                b *= 2f
            }
        } finally {
            pdf.close()
            dir.deleteRecursively()
        }
    }

    private fun firstBlob(nb: NotebookHandle): String =
        (nb.outline(true).first { it is FfiOutlineRow.Sheet } as FfiOutlineRow.Sheet).spec.pdfBlob!!

    private fun pixel(t: FfiTile, x: Int, y: Int): IntArray {
        val i = (y * t.request.width.toInt() + x) * 4
        return IntArray(4) { t.rgba[i + it].toInt() and 0xFF }
    }

    @Test
    fun importTilesSearchExport() {
        val dir = Files.createTempDirectory("n0te-pdf-bindings").toFile()
        val tiles = LinkedBlockingQueue<FfiTile>()
        val lib = NoteLibrary.open(dir.absolutePath)
        val pdf = PdfService.start(lib, pdfium(), emptyList(), object : TileSink {
            override fun onTile(tile: FfiTile) {
                tiles.put(tile)
            }
        })
        try {
            val nb = lib.createNotebook("Physik")
            val sheets = nb.importPdf(pdf, tinyPdf(), null, 0u, 2u)
            assertEquals(2, sheets.size)
            assertEquals(400f to 300f, sheets[0].width to sheets[0].height)
            assertEquals(300f to 400f, sheets[1].width to sheets[1].height) // displayed upright
            nb.save()
            val blob = firstBlob(nb)

            // A whole-page tile through the callback: the blue box (user 40..120 x 40..100) sits at
            // display y 200..260.
            assertEquals(listOf(400u, 300u), pdfPagePixels(400f, 300f, 1f))
            pdf.requestTiles(listOf(FfiTileRequest(blob, 0u, 1f, 0u, 0u, 400u, 300u, 0u, 0u, 1uL)))
            val tile = tiles.poll(10, TimeUnit.SECONDS)
            assertNotNull("tile arrives", tile)
            assertNull(tile!!.error)
            assertEquals(400 * 300 * 4, tile.rgba.size)
            assertArrayEquals(intArrayOf(0, 102, 204, 255), pixel(tile, 80, 230))
            assertArrayEquals(intArrayOf(255, 255, 255, 255), pixel(tile, 300, 50))

            val hits = pdf.findText(blob, 0u, "N0TE")
            assertEquals(1, hits.size)
            assertTrue(hits[0].boxes[0].y0 < 100f)
            assertEquals("Hallo n0te", pdf.pageText(blob, 0u))

            // Ink on page 1 (not saved: export sees open sheets), then export everything.
            val sheet = nb.openSheet(sheets[0].sheetId)
            val line = (0..40).flatMap { listOf(150f + it * 5f, 150f, 0.5f, it * 8f) }.toFloatArray()
            sheet.addStroke(FfiBrush(6f, 0f, 0xFFD71921u, 0u), pack(*line))
            val exported = nb.exportPdf(pdf, null, false)
            assertTrue(String(exported, 0, 5, Charsets.ISO_8859_1) == "%PDF-")

            // Import the export again: same pages, ink where it was drawn.
            val nb2 = lib.createNotebook("Export")
            val again = nb2.importPdf(pdf, exported, null, 0u, 2u)
            assertEquals(2, again.size)
            assertEquals(300f to 400f, again[1].width to again[1].height)
            pdf.requestTiles(listOf(FfiTileRequest(firstBlob(nb2), 0u, 1f, 0u, 0u, 400u, 300u, 0u, 0u, 1uL)))
            val inked = tiles.poll(10, TimeUnit.SECONDS)!!
            assertArrayEquals(intArrayOf(0xD7, 0x19, 0x21, 255), pixel(inked, 250, 150))
            assertArrayEquals(intArrayOf(0, 102, 204, 255), pixel(inked, 80, 230))
        } finally {
            pdf.close()
            dir.deleteRecursively()
        }
    }
}
