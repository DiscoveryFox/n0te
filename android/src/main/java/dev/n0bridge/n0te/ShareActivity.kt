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
 * a notebook picked here, placed where that notebook was last open.
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
        }
        if (out.isEmpty()) intent?.clipData?.let { c -> for (i in 0 until c.itemCount) c.getItemAt(i).uri?.let { out += it } }
        return out
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        Themes.applyWindow(this)
        if (shared().isEmpty()) { finish(); return }
        N0te.background({
            N0te.library.notebookIds().mapNotNull { id ->
                runCatching { N0te.library.loadNotebook(id) }.getOrNull()?.let { Nb(id, it.title(), it.folder(), java.io.File(N0te.root, "notebooks/$id").lastModified()) }
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
                val bytes = contentResolver.openInputStream(uri)?.use { it.readBytes() } ?: return@mapNotNull null
                val type = contentResolver.getType(uri) ?: ""
                if (type == "application/pdf" || (bytes.size > 4 && String(bytes, 0, 4, Charsets.ISO_8859_1) == "%PDF")) {
                    val f = java.io.File(cacheDir, "shared-${System.nanoTime()}.pdf").apply { writeBytes(bytes) }
                    "pdf|${f.path}"
                } else Images.prepare(bytes)?.let { (jpeg, w, h) -> "img|${N0te.library.putBlob(jpeg)}|$w|$h" }
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

    @Composable
    private fun Screen() = N0teType {
        val n = shared().size
        Column(Modifier.fillMaxSize().background(Ui.Black).statusBarsPadding().padding(horizontal = 28.dp, vertical = 20.dp), verticalArrangement = Arrangement.spacedBy(14.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Heading(if (n == 1) "In welches Notizbuch?" else "$n Dateien: in welches Notizbuch?", 26.sp)
                Spacer(Modifier.weight(1f))
                Btn("Abbrechen", BtnKind.QUIET) { finish() }
            }
            Text(if (busy) "Wird übernommen …" else "Es kommt dorthin, wo das Notizbuch zuletzt offen war.", color = if (busy) Ui.Red else Ui.Muted, fontSize = 14.sp)
            val list = notebooks
            if (list == null) Text("Lade …", color = Ui.Muted) else LazyColumn(Modifier.widthIn(max = 760.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
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
