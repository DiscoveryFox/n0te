package dev.n0bridge.n0te

import android.content.Intent
import android.net.Uri
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp

/**
 * "Teilen → n0te" (Samsung Gallery, the screenshot toolbar, any app): pictures or a PDF go into
 * a notebook picked here, placed where that notebook was last open. "Öffnen mit → n0te" (a PDF
 * in Files, a download, a mail): the PDF becomes a new notebook, or goes into one picked here.
 */
class ShareActivity : ComponentActivity() {
    private class Nb(val id: String, val title: String, val folder: String, val modified: Long)

    private var notebooks by mutableStateOf<List<Nb>?>(null)

    private fun shared(): ArrayList<Uri> {
        val out = ArrayList<Uri>()
        @Suppress("DEPRECATION")
        when (intent?.action) {
            Intent.ACTION_SEND -> (intent.getParcelableExtra<Uri>(Intent.EXTRA_STREAM))?.let { out += it }
            Intent.ACTION_SEND_MULTIPLE -> intent.getParcelableArrayListExtra<Uri>(Intent.EXTRA_STREAM)?.let { out += it }
            Intent.ACTION_VIEW -> intent.data?.let { out += it }
        }
        if (out.isEmpty()) intent?.clipData?.let { c -> for (i in 0 until c.itemCount) c.getItemAt(i).uri?.let { out += it } }
        return out
    }

    /** The file's name as the other app shows it ("Skript.pdf"), else the last part of its path if that is a file name. */
    private fun displayName(uri: Uri): String? =
        runCatching {
            contentResolver.query(uri, arrayOf(android.provider.OpenableColumns.DISPLAY_NAME), null, null, null)?.use { c -> if (c.moveToFirst()) c.getString(0) else null }
        }.getOrNull()?.takeIf { it.isNotBlank() } ?: uri.lastPathSegment?.substringAfterLast('/')?.takeIf { '.' in it }

    /** One PDF (opened with n0te, or shared alone): it can also become a notebook of its own. */
    private val pdf: Uri? by lazy {
        shared().singleOrNull()?.takeIf { uri ->
            val type = intent?.type ?: runCatching { contentResolver.getType(uri) }.getOrNull()
            type == "application/pdf" || displayName(uri)?.endsWith(".pdf", ignoreCase = true) == true
        }
    }

    /** The new notebook's name: the file's, without ".pdf". */
    private val pdfTitle: String by lazy {
        pdf?.let { displayName(it) }?.let { if (it.endsWith(".pdf", ignoreCase = true)) it.dropLast(4) else it }?.trim()?.ifBlank { null } ?: "PDF"
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        Themes.applyWindow(this)
        if (shared().isEmpty()) { finish(); return }
        N0te.background({
            N0te.library.notebookIds().mapNotNull { id ->
                runCatching { N0te.library.loadNotebook(id) }.getOrNull()?.use { Nb(id, it.title(), it.folder(), java.io.File(N0te.root, "notebooks/$id").lastModified()) }
            }.sortedByDescending { it.modified }
        }) { r -> notebooks = r.getOrDefault(emptyList()) }
        setContent { Screen() }
    }

    private var busy by mutableStateOf(false)

    /**
     * Reads the shared items here (this window holds the permission to read them), stores
     * pictures as n0te images and PDFs in the cache, then opens the notebook with what to place:
     * "img|blob|w|h" or "pdf|path".
     */
    private fun into(nb: Nb) {
        if (busy) return
        busy = true
        val uris = shared()
        N0te.background({
            uris.mapNotNull { uri ->
                val type = contentResolver.getType(uri) ?: ""
                contentResolver.openInputStream(uri)?.buffered()?.use { input ->
                    input.mark(8)
                    val head = ByteArray(4).also { input.read(it) }
                    input.reset()
                    if (type == "application/pdf" || String(head, Charsets.ISO_8859_1) == "%PDF") {
                        // Straight into the file: a big PDF isn't held in memory on the way.
                        val f = java.io.File(cacheDir, "shared-${System.nanoTime()}.pdf")
                        f.outputStream().use { input.copyTo(it) }
                        "pdf|${f.path}"
                    } else Images.prepare(input.readBytes())?.let { (jpeg, w, h) -> "img|${N0te.library.putBlob(jpeg)}|$w|$h" }
                }
            }
        }) { r ->
            busy = false
            r.onSuccess { items ->
                if (items.isEmpty()) { android.widget.Toast.makeText(this, "Nichts Einfügbares dabei", android.widget.Toast.LENGTH_LONG).show(); return@onSuccess }
                startActivity(Intent(this, NotebookActivity::class.java).putExtra(NotebookActivity.EXTRA_ID, nb.id).putStringArrayListExtra(NotebookActivity.EXTRA_SHARE, ArrayList(items)))
                finish()
            }.onFailure { android.widget.Toast.makeText(this, "Teilen fehlgeschlagen: ${it.message}", android.widget.Toast.LENGTH_LONG).show() }
        }
    }

    /** The PDF as a new notebook named after the file (one page per PDF page), then opened. */
    private fun asNotebook(uri: Uri) {
        if (busy) return
        val svc = N0te.pdf ?: return android.widget.Toast.makeText(this, "PDF-Unterstützung nicht geladen", android.widget.Toast.LENGTH_LONG).show()
        busy = true
        val title = pdfTitle
        N0te.background({
            // Read here: the permission to read it belongs to this window.
            val bytes = contentResolver.openInputStream(uri)?.use { it.readBytes() } ?: error("nicht lesbar")
            if (bytes.size < 5 || String(bytes, 0, 5, Charsets.ISO_8859_1) != "%PDF-") error("keine PDF-Datei")
            val nb = N0te.library.createNotebook(title)
            // A PDF pdfium can't read leaves no empty notebook behind.
            runCatching { nb.importPdf(svc, bytes, null, 0u, Settings.pdfChapterDepth) }.onFailure { runCatching { N0te.library.trashNotebook(nb.id()) }; throw it }
            nb.save()
            nb.id()
        }) { r ->
            busy = false
            r.onSuccess { id ->
                Settings.set("lib_opened", LibraryOrg.openedNow(Settings.str("lib_opened", ""), id, System.currentTimeMillis()))
                startActivity(Intent(this, NotebookActivity::class.java).putExtra(NotebookActivity.EXTRA_ID, id))
                finish()
            }.onFailure { android.widget.Toast.makeText(this, "PDF öffnen fehlgeschlagen: ${it.message}", android.widget.Toast.LENGTH_LONG).show() }
        }
    }

    @Composable
    private fun Screen() = N0teType {
        val n = shared().size
        val pdf = pdf
        Column(Modifier.fillMaxSize().background(Ui.Black).statusBarsPadding().padding(horizontal = 28.dp, vertical = 20.dp), verticalArrangement = Arrangement.spacedBy(14.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Heading(
                    when {
                        pdf != null -> "„$pdfTitle“ öffnen"
                        n == 1 -> "In welches Notizbuch?"
                        else -> "$n Dateien: in welches Notizbuch?"
                    },
                    26.sp, Modifier.weight(1f),
                )
                Btn("Abbrechen", BtnKind.QUIET) { finish() }
            }
            if (busy) Text("Wird übernommen …", color = Ui.Red, fontSize = 14.sp)
            if (pdf != null) {
                Row(Modifier.widthIn(max = 760.dp).fillMaxWidth().cardBox().clickable { asNotebook(pdf) }.padding(horizontal = 18.dp, vertical = 14.dp), verticalAlignment = Alignment.CenterVertically) {
                    GlyphIcon(Glyph.PDF, Ui.Red, 22.dp)
                    Column(Modifier.weight(1f).padding(start = 14.dp)) {
                        Text("Neues Notizbuch", color = Ui.Chalk, fontSize = 16.sp, fontFamily = Voice.titleFont, fontWeight = FontWeight.Medium)
                        Text("„$pdfTitle“, eine Seite pro PDF-Seite", color = Ui.Muted, fontSize = 12.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
                    }
                    GlyphIcon(Glyph.RIGHT, Ui.Muted, 18.dp)
                }
                Spacer(Modifier.height(4.dp))
                Caption("In Notizbuch einfügen")
            }
            if (!busy) Text(
                if (pdf != null) "Die Seiten kommen nach der Seite, an der das Notizbuch zuletzt offen war." else "Es kommt dorthin, wo das Notizbuch zuletzt offen war.",
                color = Ui.Muted, fontSize = 14.sp,
            )
            val list = notebooks
            if (list == null) Text("Lade …", color = Ui.Muted)
            else if (list.isEmpty()) Text(
                if (pdf != null) "Noch keine Notizbücher: „Neues Notizbuch“ oben macht aus der PDF eins." else "Noch keine Notizbücher. Leg in n0te eins an und teile dann nochmal.",
                color = Ui.Muted, fontSize = 14.sp,
            )
            else LazyColumn(Modifier.widthIn(max = 760.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                items(list, key = { it.id }) { nb ->
                    Row(Modifier.fillMaxWidth().cardBox().clickable { into(nb) }.padding(horizontal = 18.dp, vertical = 14.dp), verticalAlignment = Alignment.CenterVertically) {
                        Column(Modifier.weight(1f)) {
                            Text(nb.title, color = Ui.Chalk, fontSize = 16.sp, fontFamily = Voice.titleFont, fontWeight = FontWeight.Medium, maxLines = 1, overflow = TextOverflow.Ellipsis)
                            if (nb.folder.isNotBlank()) Text(nb.folder, color = Ui.Muted, fontSize = 12.sp)
                        }
                        GlyphIcon(Glyph.RIGHT, Ui.Muted, 18.dp)
                    }
                }
            }
        }
    }
}
