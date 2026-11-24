package dev.n0bridge.n0te

import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.Paint
import uniffi.n0te_ffi.FfiBackground
import uniffi.n0te_ffi.FfiSheetSpec
import uniffi.n0te_ffi.NotebookHandle
import java.io.File

/**
 * Pictures of pages without opening the notebook (the start page's "Zuletzt bearbeitet"):
 * white paper with its ruling, graphs, typed text, ink. PDF backgrounds are left out.
 * Kept as files, drawn anew when the page changed.
 */
object PagePictures {
    private val ink = Paint(Paint.ANTI_ALIAS_FLAG).apply { style = Paint.Style.FILL }
    private val rule = Paint(Paint.ANTI_ALIAS_FLAG).apply { color = 0xFFD5D8DE.toInt(); strokeWidth = 0.6f }

    /** The picture of page [sheetId] (changed at [changed]) [w] pixels wide, from the file kept or drawn now (background thread). */
    fun of(dir: File, nb: NotebookHandle, sheetId: String, spec: FfiSheetSpec, changed: Long, w: Int): File? {
        if (spec.width <= 0f) return null
        dir.mkdirs()
        val name = "${nb.id()}_${sheetId}_$changed.png"
        val f = File(dir, name)
        if (f.isFile) return f
        dir.listFiles()?.forEach { if (it.name.startsWith("${nb.id()}_${sheetId}_")) it.delete() }
        // An empty page isn't worth showing (remembered, so it isn't opened again and again).
        val empty = File(dir, "$name.empty")
        if (empty.isFile) return null
        val bmp = render(nb, sheetId, spec, w) ?: run { runCatching { empty.createNewFile() }; return null }
        runCatching { f.outputStream().use { bmp.compress(Bitmap.CompressFormat.PNG, 90, it) } }
        bmp.recycle()
        return f.takeIf { it.isFile }
    }

    /** Old pictures of pages no longer among the recent ones go. */
    fun keepOnly(dir: File, names: Set<String>) {
        dir.listFiles()?.forEach { if (it.name !in names && !it.name.endsWith(".empty")) it.delete() }
    }

    private fun render(nb: NotebookHandle, sheetId: String, spec: FfiSheetSpec, w: Int): Bitmap? {
        val h = runCatching { nb.openSheet(sheetId) }.getOrNull() ?: return null
        val strokes = runCatching { h.allStrokes() }.getOrDefault(emptyList())
        val texts = runCatching { h.textBoxesWithBody(spec.width) }.getOrDefault(emptyList())
        val images = runCatching { h.images() }.getOrDefault(emptyList())
        if (strokes.isEmpty() && texts.isEmpty() && images.isEmpty()) return null
        val k = w / spec.width
        val bmp = Bitmap.createBitmap(w, (spec.height * k).toInt().coerceIn(1, w * 4), Bitmap.Config.ARGB_8888)
        val c = Canvas(bmp)
        c.drawColor(Color.WHITE)
        c.scale(k, k)
        ruling(c, spec)
        images.forEach { im -> im.plot?.let { p -> runCatching { uniffi.n0te_ffi.plotStrokes(p) }.getOrDefault(emptyList()).forEach { draw(c, InkStroke.of(it)) } } }
        runCatching { TextBoxes.draw(c, texts) }
        strokes.forEach { draw(c, InkStroke.of(it)) }
        return bmp
    }

    private fun draw(c: Canvas, s: InkStroke) {
        ink.color = s.color
        c.drawPath(s.path, ink)
    }

    /** The paper's lines, squares or dots, roughly as on the page. */
    private fun ruling(c: Canvas, spec: FfiSheetSpec) {
        val step = spec.spacing.coerceAtLeast(6f)
        when (spec.background) {
            FfiBackground.LINES, FfiBackground.CORNELL -> {
                var y = step * 3
                while (y < spec.height) { c.drawLine(0f, y, spec.width, y, rule); y += step }
                if (spec.background == FfiBackground.CORNELL) c.drawLine(spec.width * 0.28f, 0f, spec.width * 0.28f, spec.height * 0.8f, rule)
            }
            FfiBackground.GRID -> {
                var x = 0f
                while (x < spec.width) { c.drawLine(x, 0f, x, spec.height, rule); x += step }
                var y = 0f
                while (y < spec.height) { c.drawLine(0f, y, spec.width, y, rule); y += step }
            }
            FfiBackground.DOTS -> {
                var y = step
                while (y < spec.height) { var x = step; while (x < spec.width) { c.drawCircle(x, y, 0.8f, rule); x += step }; y += step }
            }
            else -> {}
        }
    }
}
