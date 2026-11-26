package dev.n0bridge.n0te

import android.content.Intent
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.gestures.awaitLongPressOrCancellation
import androidx.compose.foundation.gestures.drag
import androidx.compose.foundation.layout.offset
import androidx.compose.ui.input.pointer.changedToUp
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.boundsInRoot
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.layout.positionInRoot
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.lazy.grid.GridCells
import androidx.compose.foundation.lazy.grid.GridItemSpan
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.window.Dialog
import androidx.compose.foundation.combinedClickable
import androidx.compose.ui.graphics.asImageBitmap
import uniffi.n0te_ffi.FfiBackground
import uniffi.n0te_ffi.FfiSheetSpec

/** The library: notebooks grouped by folder. Opens a notebook, creates one. */
class LibraryActivity : ComponentActivity() {

    data class Entry(val id: String, val title: String, val folder: String, val sheets: Int, val cover: String, val modified: Long = 0L,
                     val tags: List<String> = emptyList(), val pinned: Boolean = false)

    private var entries by mutableStateOf<List<Entry>?>(null)

    /** A page changed lately (here or on another device): where, what it is called, its picture. */
    data class RecentPage(val notebook: String, val sheet: String, val changed: Long, val notebookTitle: String, val label: String, val aspect: Float, val picture: String)
    private var recentPages by mutableStateOf<List<RecentPage>>(emptyList())
    // --- organising: where the library looks, search, order, picking several, the dialogs ---
    private var place by mutableStateOf<LibraryOrg.Place>(LibraryOrg.Place.Home)
    /** Notebooks as tiles with their cover, or as a compact list. */
    private var asList by mutableStateOf(Settings.str("lib_view", "grid") == "list")
    /** Notebooks being dragged (onto a folder, a tag, "pinned"), where the finger is, what lies under it. */
    private var dragging by mutableStateOf<List<Entry>?>(null)
    private var dragAt by mutableStateOf(androidx.compose.ui.geometry.Offset.Zero)
    private var dropOn by mutableStateOf<String?>(null)
    private val targets = HashMap<String, androidx.compose.ui.geometry.Rect>()
    private var query by mutableStateOf("")
    private var byName by mutableStateOf(Settings.str("lib_sort", "modified") == "name")
    /** Several notebooks picked at once (null: not picking). */
    private var picked by mutableStateOf<Set<String>?>(null)
    /** The notebook whose menu is open. */
    private var menuFor by mutableStateOf<String?>(null)
    private var sureTrash by mutableStateOf(false)
    private var renaming by mutableStateOf<Entry?>(null)
    private var covering by mutableStateOf<Entry?>(null)
    private var moving by mutableStateOf<List<Entry>?>(null)
    private var tagging by mutableStateOf<List<Entry>?>(null)
    /** The folder (path) or tag whose menu is open in the sidebar; where a new folder goes ("" = top). */
    private var folderFor by mutableStateOf<String?>(null)
    private var tagFor by mutableStateOf<String?>(null)
    private var newFolderIn by mutableStateOf<String?>(null)
    /** Folders made but still empty (kept on this tablet until a notebook goes in). */
    private val extraFolders = androidx.compose.runtime.mutableStateListOf<String>().apply { addAll(Settings.str("lib_folders", "").split('\u001f').filter { it.isNotEmpty() }) }

    private fun keepFolders() = Settings.set("lib_folders", extraFolders.joinToString("\u001f"))
    private var collapsed by mutableStateOf(Settings.str("lib_collapsed", "").split('\u001f').filter { it.isNotEmpty() }.toSet())
    private var joining by mutableStateOf<String?>(null)
    /** What's new in this version: shown once after an install or update. */
    private var news by mutableStateOf<Changelog.Entry?>(null)
    private val syncListener: (uniffi.n0te_ffi.FfiSyncEvent) -> Unit = { e ->
        if (e is uniffi.n0te_ffi.FfiSyncEvent.RemoteChanged || e is uniffi.n0te_ffi.FfiSyncEvent.Paired) reload()
        // Joining an invite with approval: the owner lets us in by hand.
        if (e is uniffi.n0te_ffi.FfiSyncEvent.JoinWaiting) busy = "Warte, bis ${e.owner} dich hereinlässt …"
    }
    /** An invite that asks for a PIN (the link; the dialog asks for the PIN, then joins). */
    private var pinFor by mutableStateOf<String?>(null)
    private var pinWrong by mutableStateOf(false)
    private val themeId = Ui.theme.id
    private var busy by mutableStateOf<String?>(null)
    private val version: String by lazy { runCatching { packageManager.getPackageInfo(packageName, 0).versionName }.getOrNull() ?: "?" }

    private val pickFile = registerForActivityResult(androidx.activity.result.contract.ActivityResultContracts.OpenDocument()) { uri ->
        if (uri != null) importFile(uri)
    }

    /** A picked file: a `.n0te` notebook (from the laptop) is merged in, a PDF becomes a notebook. */
    private fun importFile(uri: android.net.Uri) {
        val head = runCatching { contentResolver.openInputStream(uri)?.use { s -> ByteArray(8).also { s.read(it) } } }.getOrNull() ?: ByteArray(0)
        if (String(head, Charsets.ISO_8859_1).startsWith("N0TEBNDL")) importBundle(uri) else importPdf(uri)
    }

    /** Takes over a notebook sent as `.n0te`, merged with the copy here (nothing is lost). */
    private fun importBundle(uri: android.net.Uri) {
        busy = "Übernehme Notizbuch…"
        N0te.background({
            val f = java.io.File(cacheDir, "import.n0te")
            contentResolver.openInputStream(uri)?.use { input -> f.outputStream().use { input.copyTo(it) } } ?: error("nicht lesbar")
            N0te.library.importBundle(f.path).also { f.delete() }
        }) { r ->
            busy = null
            r.onSuccess { reload(); open(it) }
                .onFailure { android.widget.Toast.makeText(this, "Import fehlgeschlagen: ${it.message}", android.widget.Toast.LENGTH_LONG).show() }
        }
    }

    /** Sends the notebook as a `.n0te` file to the laptop (through n0bridge). */
    private fun sendToLaptop(e: Entry) {
        busy = "Packe „${e.title}“…"
        N0te.background({
            val dir = java.io.File(cacheDir, "share").apply { mkdirs() }
            val safe = e.title.ifBlank { "Notizbuch" }.replace(Regex("[^\\p{L}\\p{N} _-]"), "_").trim().replace(' ', '_')
            val f = java.io.File(dir, "$safe.n0te")
            N0te.library.exportBundle(e.id, f.path)
            f
        }) { r ->
            busy = null
            r.onSuccess { f -> shareToN0bridge(this, f, "application/octet-stream") }
                .onFailure { android.widget.Toast.makeText(this, "Export fehlgeschlagen: ${it.message}", android.widget.Toast.LENGTH_LONG).show() }
        }
    }

    /** A PDF (lecture slides) becomes a new notebook, one page per PDF page. */
    private fun importPdf(uri: android.net.Uri) {
        val svc = N0te.pdf ?: return android.widget.Toast.makeText(this, "PDF-Unterstützung nicht geladen", android.widget.Toast.LENGTH_LONG).show()
        val name = contentResolver.query(uri, arrayOf(android.provider.OpenableColumns.DISPLAY_NAME), null, null, null)?.use { c ->
            if (c.moveToFirst()) c.getString(0) else null
        }?.removeSuffix(".pdf")?.removeSuffix(".PDF") ?: "PDF"
        busy = "Importiere $name…"
        N0te.background({
            val bytes = contentResolver.openInputStream(uri)?.use { it.readBytes() } ?: error("nicht lesbar")
            val nb = N0te.library.createNotebook(name)
            nb.importPdf(svc, bytes, null, 0u)
            nb.save()
            nb.id()
        }) { r ->
            busy = null
            r.onSuccess { open(it) }.onFailure { android.widget.Toast.makeText(this, "Import fehlgeschlagen: ${it.message}", android.widget.Toast.LENGTH_LONG).show() }
        }
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        Themes.applyWindow(this)
        news = Changelog.unseen(this)
        inviteFrom(intent)
        setContent { Screen() }
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        inviteFrom(intent)
    }

    /** A tapped n0te://invite/… link: the join dialog with it filled in. */
    private fun inviteFrom(i: Intent?) {
        val link = i?.dataString ?: return
        if (i.action == Intent.ACTION_VIEW && link.startsWith("n0te://invite/")) joining = link
    }

    /** "Einladung": an invite link in the clipboard is filled in already. */
    private fun clipboardInvite(): String = runCatching {
        val cm = getSystemService(android.content.Context.CLIPBOARD_SERVICE) as android.content.ClipboardManager
        cm.primaryClip?.getItemAt(0)?.coerceToText(this)?.toString()?.trim()?.takeIf { it.startsWith("n0te://invite/") }
    }.getOrNull() ?: ""

    override fun onResume() {
        super.onResume()
        if (Ui.theme.id != themeId) { recreate(); return }
        SyncHub.listen(syncListener)
        SyncHub.refreshGuests()
        reload()
        // A new version on GitHub? (Downloaded in the background, installed on a tap.)
        Updater.maybeCheck(this)
    }

    override fun onPause() {
        SyncHub.unlisten(syncListener)
        super.onPause()
    }

    /** Joins a notebook someone shared (invite link): it is loaded straight from them. */
    private fun join(link: String, pin: String? = null) {
        val engine = SyncHub.engine ?: return android.widget.Toast.makeText(this, "Sync startet noch …", android.widget.Toast.LENGTH_SHORT).show()
        busy = "Verbinde mit dem Besitzer …"
        N0te.background({
            val id = engine.joinWith(link.trim(), pin)
            N0te.main.post { busy = "Lade das Notizbuch vom Besitzer …" }
            // It arrives in the background: wait until it is here (opening it right away was the
            // "error on first load"), at most 45 s.
            val until = System.currentTimeMillis() + 45_000
            var ready = false
            while (!ready && System.currentTimeMillis() < until) {
                ready = runCatching { N0te.library.loadNotebook(id).sheetCount() > 0u }.getOrDefault(false)
                if (!ready) Thread.sleep(400)
            }
            id to ready
        }) { r ->
            busy = null
            SyncHub.refreshGuests()
            r.onSuccess { (id, ready) ->
                reload()
                if (ready) open(id) else android.widget.Toast.makeText(this, "Beigetreten — das Notizbuch kommt, sobald der Besitzer erreichbar ist.", android.widget.Toast.LENGTH_LONG).show()
            }.onFailure {
                val msg = it.message.orEmpty()
                // The invite has a PIN: ask for it (again, if it was wrong).
                if (msg.contains("PIN")) { pinWrong = msg.contains("falsch"); pinFor = link }
                else android.widget.Toast.makeText(this, "Beitreten fehlgeschlagen: $msg", android.widget.Toast.LENGTH_LONG).show()
            }
        }
    }

    @Composable
    private fun PinDialog(link: String) {
        var pin by remember { mutableStateOf("") }
        Dialog(onDismissRequest = { pinFor = null }) {
            N0teType {
                Column(Modifier.width(420.dp).panel().padding(24.dp), verticalArrangement = Arrangement.spacedBy(14.dp)) {
                    Heading("PIN eingeben", 22.sp)
                    Text(if (pinWrong) "Die PIN stimmt nicht. Frag nochmal nach." else "Diese Einladung braucht eine PIN. Wer dich eingeladen hat, sagt sie dir.",
                        color = if (pinWrong) Ui.Red else Ui.Muted, fontSize = 13.sp)
                    Field(pin, "z. B. 4821", mono = true, digits = true) { pin = it.filter(Char::isDigit).take(8) }
                    Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
                        Spacer(Modifier.weight(1f))
                        Btn("Abbrechen", BtnKind.QUIET) { pinFor = null }
                        Spacer(Modifier.width(8.dp))
                        Btn("Beitreten", BtnKind.PRIMARY, enabled = pin.isNotBlank()) { pinFor = null; join(link, pin) }
                    }
                }
            }
        }
    }

    private fun thumbFile(id: String) = java.io.File(java.io.File(filesDir, "thumbs"), "$id.png")

    private fun trash(e: Entry) {
        N0te.background({ N0te.library.trashNotebook(e.id); thumbFile(e.id).delete() }) { r ->
            r.onFailure { android.widget.Toast.makeText(this, "Konnte nicht löschen: ${it.message}", android.widget.Toast.LENGTH_LONG).show() }
            reload()
        }
    }

    private fun reload() {
        N0te.background({
            N0te.library.notebookIds().mapNotNull { id ->
                runCatching { N0te.library.loadNotebook(id) }.getOrNull()?.let {
                    Entry(id, it.title(), it.folder(), it.sheetCount().toInt(), it.cover(), java.io.File(N0te.root, "notebooks/$id").lastModified(), it.tags(), it.pinned())
                }
            }.sortedWith(compareBy({ it.folder.lowercase() }, { it.title.lowercase() }))
        }) { r -> entries = r.getOrDefault(emptyList()); loadRecentPages() }
    }

    /** The pages changed last, with their pictures (after the list, so it never waits for them). */
    private fun loadRecentPages() {
        val dir = java.io.File(filesDir, "thumbs/pages")
        N0te.background({
            val out = ArrayList<RecentPage>()
            val books = HashMap<String, Pair<uniffi.n0te_ffi.NotebookHandle, List<uniffi.n0te_ffi.FfiOutlineRow>>?>()
            for (r in runCatching { N0te.library.recentSheets(30u) }.getOrDefault(emptyList())) {
                if (out.size >= 10) break
                val (nb, rows) = books.getOrPut(r.notebook) { runCatching { N0te.library.loadNotebook(r.notebook).let { it to it.outline(true) } }.getOrNull() } ?: continue
                // Where the page is (a main page, or beside one); a page deleted since isn't shown.
                var n = 0
                var hit: Pair<uniffi.n0te_ffi.FfiOutlineRow.Sheet, String>? = null
                for (row in rows) if (row is uniffi.n0te_ffi.FfiOutlineRow.Sheet) {
                    n++
                    when {
                        row.sheetId == r.sheet -> hit = row to row.title.ifBlank { "Seite $n" }
                        row.left.any { it.sheetId == r.sheet } -> hit = row to "Seite $n · links"
                        row.right.any { it.sheetId == r.sheet } -> hit = row to "Seite $n · rechts"
                    }
                    if (hit != null) break
                }
                val (row, label) = hit ?: continue
                val spec = if (row.sheetId == r.sheet) row.spec else (row.left + row.right).first { it.sheetId == r.sheet }.spec
                val pic = runCatching { PagePictures.of(dir, nb, r.sheet, spec, r.changed, 320) }.getOrNull() ?: continue
                out += RecentPage(r.notebook, r.sheet, r.changed, nb.title(), label, (spec.height / spec.width).coerceIn(0.4f, 2f), pic.path)
            }
            PagePictures.keepOnly(dir, out.map { java.io.File(it.picture).name }.toSet())
            out
        }) { r -> r.getOrNull()?.let { recentPages = it } }
    }

    private fun open(id: String, sheet: String? = null) {
        Settings.set("lib_opened", LibraryOrg.openedNow(Settings.str("lib_opened", ""), id, System.currentTimeMillis()))
        startActivity(Intent(this, NotebookActivity::class.java).putExtra(NotebookActivity.EXTRA_ID, id).apply { sheet?.let { putExtra(NotebookActivity.EXTRA_SHEET, it) } })
    }

    private fun create(title: String, folder: String, spec: FfiSheetSpec) {
        N0te.background({
            val nb = N0te.library.createNotebook(title.ifBlank { "Neues Notizbuch" })
            if (folder.isNotBlank()) nb.setFolder(folder)
            nb.addSheet(null, 0u, spec)
            nb.save()
            nb.id()
        }) { r -> r.getOrNull()?.let { open(it) } }
    }

    @Composable
    private fun Screen() = N0teType {
        var creating by remember { mutableStateOf(false) }
        val list = entries
        Box(Modifier.fillMaxSize()) {
        Column(Modifier.fillMaxSize().background(Ui.Black)) {
            Row(Modifier.fillMaxWidth().padding(start = 28.dp, end = 20.dp, top = 16.dp, bottom = 14.dp), verticalAlignment = Alignment.CenterVertically) {
                Wordmark(cell = 4.dp)
                Spacer(Modifier.weight(1f))
                SyncChip()
                Spacer(Modifier.width(14.dp))
                UpdateButton()
                Btn("Einladung") { joining = clipboardInvite() }
                Spacer(Modifier.width(10.dp))
                Btn("Importieren") { pickFile.launch(arrayOf("application/pdf", "application/octet-stream", "*/*")) }
                Spacer(Modifier.width(10.dp))
                Btn("+ Neues Notizbuch", BtnKind.PRIMARY) { creating = true }
                Spacer(Modifier.width(8.dp))
                IconBtn(Glyph.SETTINGS) { startActivity(Intent(this@LibraryActivity, SettingsActivity::class.java)) }
            }
            Box(Modifier.fillMaxWidth().height(1.dp).background(Ui.Border))
            Row(Modifier.fillMaxSize()) {
                if (!list.isNullOrEmpty()) Sidebar(list)
                Box(Modifier.weight(1f).fillMaxSize()) {
                    // The design's decorations (snow, stars, …) behind the notebooks.
                    if (Settings.decorShow) DecorLayer(Ui.theme, Modifier.matchParentSize())
                    Main(list) { creating = true }
                    picked?.let { p -> SelectBar(p, list.orEmpty(), Modifier.align(Alignment.BottomCenter).padding(bottom = 22.dp)) }
                }
            }
        }
        // What is being dragged, under the finger.
        dragging?.let { g ->
            val d = androidx.compose.ui.platform.LocalDensity.current
            Row(Modifier.offset { androidx.compose.ui.unit.IntOffset((dragAt.x - with(d) { 24.dp.toPx() }).toInt(), (dragAt.y - with(d) { 28.dp.toPx() }).toInt()) }
                .bar(14f).padding(horizontal = 14.dp, vertical = 10.dp), verticalAlignment = Alignment.CenterVertically) {
                GlyphIcon(Glyph.PAGE, Ui.Chalk, 18.dp)
                Text(if (g.size == 1) g[0].title else "${g.size} Notizbücher", color = Ui.Chalk, fontSize = 14.sp, maxLines = 1, overflow = TextOverflow.Ellipsis,
                    modifier = Modifier.padding(start = 10.dp).widthIn(max = 220.dp))
            }
        }
        }
        if (creating) CreateDialog(onDismiss = { creating = false }, folder = (place as? LibraryOrg.Place.Folder)?.path ?: "") { t, f, spec -> creating = false; create(t, f, spec) }
        renaming?.let { RenameDialog(it) }
        covering?.let { CoverDialog(it) }
        moving?.let { MoveDialog(it, list.orEmpty()) }
        tagging?.let { TagsDialog(it, list.orEmpty()) }
        folderFor?.let { FolderDialog(it, list.orEmpty()) }
        newFolderIn?.let { NewFolderDialog(it) }
        tagFor?.let { TagDialog(it, list.orEmpty()) }
        news?.let { NewsDialog(it) }
        // A downloaded update: asked once when the app comes up; later the button at the top.
        (Updater.state as? Updater.State.Ready)?.takeIf { news == null && Updater.asked != it.release.version }?.let { UpdateDialog(it.release) }
        pinFor?.let { PinDialog(it) }
        joining?.let { link ->
            Dialog(onDismissRequest = { joining = null }) {
                N0teType {
                    Column(Modifier.width(480.dp).panel().padding(24.dp), verticalArrangement = Arrangement.spacedBy(14.dp)) {
                        Heading("Einladung öffnen", 22.sp)
                        Text("Das Notizbuch kommt direkt vom Gerät, das es geteilt hat, ohne Server.", color = Ui.Muted, fontSize = 13.sp)
                        Field(link, "n0te://invite/…", mono = true) { joining = it }
                        Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
                            Btn("QR scannen") {
                                runCatching {
                                    com.google.mlkit.vision.codescanner.GmsBarcodeScanning.getClient(this@LibraryActivity).startScan()
                                        .addOnSuccessListener { b -> b.rawValue?.trim()?.let { joining = it } }
                                        .addOnFailureListener { e -> android.widget.Toast.makeText(this@LibraryActivity, "QR-Scanner: ${e.message}", android.widget.Toast.LENGTH_LONG).show() }
                                }
                            }
                            Spacer(Modifier.weight(1f))
                            Btn("Abbrechen", BtnKind.QUIET) { joining = null }
                            Spacer(Modifier.width(8.dp))
                            Btn("Beitreten", BtnKind.PRIMARY, enabled = link.isNotBlank()) { joining = null; join(link) }
                        }
                    }
                }
            }
        }
    }

    // --- organising ---------------------------------------------------------------------------

    /** Changes notebooks in the background (each loaded, changed, saved), then shows the library anew. */
    private fun change(ids: Collection<String>, what: (String, uniffi.n0te_ffi.NotebookHandle) -> Unit) {
        if (ids.isEmpty()) return
        N0te.background({
            ids.forEach { id ->
                // Organising isn't editing: the notebook keeps its place under "Zuletzt geändert".
                val dir = java.io.File(N0te.root, "notebooks/$id")
                val was = dir.lastModified()
                val nb = N0te.library.loadNotebook(id); what(id, nb); nb.save()
                if (was > 0) dir.setLastModified(was)
            }
        }) { r ->
            r.onFailure { android.widget.Toast.makeText(this, "Ging nicht: ${it.message}", android.widget.Toast.LENGTH_LONG).show() }
            reload()
        }
    }

    private fun setCollapsed(path: String, on: Boolean) {
        collapsed = if (on) collapsed + path else collapsed - path
        Settings.set("lib_collapsed", collapsed.joinToString("\u001f"))
    }

    /** Tags across the library: each name once (as first written), with how many notebooks carry it. */
    private fun tagsOf(list: List<Entry>): List<Pair<String, Int>> =
        list.flatMap { it.tags }.groupBy { it.lowercase() }.map { (_, v) -> v.first() to list.count { e -> e.tags.any { t -> t.equals(v.first(), ignoreCase = true) } } }
            .sortedBy { it.first.lowercase() }

    /** The library's sidebar: all, pinned, the folder tree, the tags. A hold opens a folder's or tag's menu. */
    @OptIn(androidx.compose.foundation.ExperimentalFoundationApi::class)
    @Composable
    private fun Sidebar(list: List<Entry>) {
        if (dragging == null) targets.clear()
        Column(Modifier.width(240.dp).fillMaxSize().verticalScroll(rememberScrollState()).padding(start = 14.dp, end = 6.dp, top = 18.dp, bottom = 24.dp),
            verticalArrangement = Arrangement.spacedBy(1.dp)) {
            SideRow(Glyph.HOME, "Start", null, place == LibraryOrg.Place.Home && query.isBlank()) { place = LibraryOrg.Place.Home; query = "" }
            SideRow(Glyph.PAGE, "Alle Notizbücher", list.size, place == LibraryOrg.Place.All && query.isBlank()) { place = LibraryOrg.Place.All; query = "" }
            val pins = list.count { it.pinned }
            // While dragging: "pinned" and "in no folder" take what is dropped there.
            if (pins > 0 || dragging != null) SideRow(Glyph.PIN, "Angeheftet", pins, place == LibraryOrg.Place.Pinned && query.isBlank(), key = "pin") { place = LibraryOrg.Place.Pinned; query = "" }
            if (dragging != null && dragging.orEmpty().any { it.folder.isNotEmpty() }) SideRow(Glyph.UP_ARROW, "Ohne Ordner", null, false, key = "top") {}
            Row(Modifier.fillMaxWidth().padding(start = 10.dp, top = 16.dp, bottom = 2.dp), verticalAlignment = Alignment.CenterVertically) {
                Caption("Ordner", Modifier.weight(1f))
                Box(Modifier.size(30.dp).clip(RoundedCornerShape(8.dp)).clickable { newFolderIn = (place as? LibraryOrg.Place.Folder)?.path ?: "" }, contentAlignment = Alignment.Center) {
                    GlyphIcon(Glyph.PLUS, Ui.Muted, 16.dp)
                }
            }
            val tree = LibraryOrg.tree(list.map { it.folder }.filter { it.isNotEmpty() }, extraFolders, collapsed)
            if (tree.isEmpty()) Text("Noch keine. Über + anlegen, dann Notizbücher hineinziehen.", color = Ui.Muted, fontSize = 12.sp, modifier = Modifier.padding(start = 10.dp, end = 8.dp, top = 2.dp))
            tree.forEach { n ->
                val on = place == LibraryOrg.Place.Folder(n.path) && query.isBlank()
                val over = dropOn == "folder:${n.path}"
                Row(Modifier.fillMaxWidth().onGloballyPositioned { targets["folder:${n.path}"] = it.boundsInRoot() }
                    .clip(RoundedCornerShape(10.dp)).background(if (over) Ui.Red.copy(alpha = 0.3f) else if (on) Ui.Elevated else Color.Transparent)
                    .combinedClickable(onLongClick = { folderFor = n.path }) { place = LibraryOrg.Place.Folder(n.path); query = "" }
                    .padding(start = (4 + n.depth * 14).dp, end = 10.dp, top = 7.dp, bottom = 7.dp), verticalAlignment = Alignment.CenterVertically) {
                    Box(Modifier.size(22.dp).clip(RoundedCornerShape(6.dp)).clickable(enabled = n.hasChildren) { setCollapsed(n.path, n.path !in collapsed) }, contentAlignment = Alignment.Center) {
                        if (n.hasChildren) Text(if (n.path in collapsed) "▸" else "▾", color = Ui.Muted, fontSize = 12.sp)
                    }
                    GlyphIcon(Glyph.FOLDER, if (on) Ui.Chalk else Ui.Muted, 18.dp)
                    Text(n.name, color = Ui.Chalk, fontSize = 14.sp, maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f).padding(start = 10.dp))
                    if (n.count > 0) Text("${n.count}", color = Ui.Muted, fontSize = 12.sp, fontFamily = Voice.metaFont)
                }
            }
            val tags = tagsOf(list)
            if (tags.isNotEmpty()) {
                Caption("Tags", Modifier.padding(start = 10.dp, top = 16.dp, bottom = 2.dp))
                tags.forEach { (t, c) ->
                    val on = (place as? LibraryOrg.Place.Tag)?.name?.equals(t, ignoreCase = true) == true && query.isBlank()
                    val over = dropOn == "tag:$t"
                    Row(Modifier.fillMaxWidth().onGloballyPositioned { targets["tag:$t"] = it.boundsInRoot() }
                        .clip(RoundedCornerShape(10.dp)).background(if (over) Ui.Red.copy(alpha = 0.3f) else if (on) Ui.Elevated else Color.Transparent)
                        .combinedClickable(onLongClick = { tagFor = t }) { place = LibraryOrg.Place.Tag(t); query = "" }
                        .padding(start = 14.dp, end = 10.dp, top = 8.dp, bottom = 8.dp), verticalAlignment = Alignment.CenterVertically) {
                        Box(Modifier.size(10.dp).clip(androidx.compose.foundation.shape.CircleShape).background(Color(LibraryOrg.tagColor(t))))
                        Text(t, color = Ui.Chalk, fontSize = 14.sp, maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f).padding(start = 12.dp))
                        Text("$c", color = Ui.Muted, fontSize = 12.sp, fontFamily = Voice.metaFont)
                    }
                }
            }
        }
    }

    @Composable
    private fun SideRow(g: Glyph, label: String, count: Int?, on: Boolean, key: String? = null, onClick: () -> Unit) {
        val over = key != null && dropOn == key
        Row(Modifier.fillMaxWidth().then(if (key != null) Modifier.onGloballyPositioned { targets[key] = it.boundsInRoot() } else Modifier)
            .clip(RoundedCornerShape(10.dp)).background(if (over) Ui.Red.copy(alpha = 0.3f) else if (on) Ui.Elevated else Color.Transparent).clickable(onClick = onClick)
            .padding(horizontal = 10.dp, vertical = 8.dp), verticalAlignment = Alignment.CenterVertically) {
            GlyphIcon(g, if (on) Ui.Chalk else Ui.Muted, 18.dp)
            Text(label, color = Ui.Chalk, fontSize = 14.sp, fontWeight = if (on) FontWeight.Medium else FontWeight.Normal, modifier = Modifier.weight(1f).padding(start = 12.dp))
            count?.let { Text("$it", color = Ui.Muted, fontSize = 12.sp, fontFamily = Voice.metaFont) }
        }
    }

    /** What is in the place chosen (or found by the search): a heading, search, order and view, then the notebooks. */
    @Composable
    private fun Main(list: List<Entry>?, onCreate: () -> Unit) {
        val all = list
        val p = place
        val searching = query.isNotBlank()
        val order = compareByDescending<Entry> { it.pinned }.then(if (byName) compareBy { it.title.lowercase() } else compareByDescending { it.modified })
        val shown = all.orEmpty().filter { e ->
            if (searching) LibraryOrg.matches(e.title, e.folder, e.tags, query)
            else when (p) {
                LibraryOrg.Place.Home, LibraryOrg.Place.All -> true
                LibraryOrg.Place.Pinned -> e.pinned
                is LibraryOrg.Place.Folder -> e.folder == p.path
                is LibraryOrg.Place.Tag -> e.tags.any { it.equals(p.name, ignoreCase = true) }
            }
        }.sortedWith(order)
        val subfolders = if (!searching && p is LibraryOrg.Place.Folder) LibraryOrg.allFolders(all.orEmpty().map { it.folder }, extraFolders).filter { LibraryOrg.parent(it) == p.path } else emptyList()
        val home = p == LibraryOrg.Place.Home && !searching
        Column(Modifier.fillMaxSize()) {
            Row(Modifier.fillMaxWidth().padding(start = 32.dp, end = 24.dp, top = 20.dp, bottom = 6.dp), verticalAlignment = Alignment.CenterVertically) {
                Column(Modifier.weight(1f)) {
                    // Where we are: Alle › Uni › WS26 (each part leads there).
                    if (!searching && p is LibraryOrg.Place.Folder) Row(verticalAlignment = Alignment.CenterVertically) {
                        val parts = p.path.split('/')
                        Text("Start", color = Ui.Muted, fontSize = 12.sp, modifier = Modifier.clip(RoundedCornerShape(6.dp)).clickable { place = LibraryOrg.Place.Home }.padding(horizontal = 2.dp, vertical = 2.dp))
                        parts.dropLast(1).forEachIndexed { i, part ->
                            Text(" › ", color = Ui.Muted, fontSize = 12.sp)
                            Text(part, color = Ui.Muted, fontSize = 12.sp, modifier = Modifier.clip(RoundedCornerShape(6.dp)).clickable { place = LibraryOrg.Place.Folder(parts.take(i + 1).joinToString("/")) }.padding(horizontal = 2.dp, vertical = 2.dp))
                        }
                    }
                    val title = when {
                        searching -> "Suche"
                        p == LibraryOrg.Place.Home -> "Bibliothek"
                        p == LibraryOrg.Place.All -> "Alle Notizbücher"
                        p == LibraryOrg.Place.Pinned -> "Angeheftet"
                        p is LibraryOrg.Place.Folder -> LibraryOrg.name(p.path)
                        p is LibraryOrg.Place.Tag -> p.name
                        else -> "Bibliothek"
                    }
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        if (!searching && p is LibraryOrg.Place.Tag) Box(Modifier.padding(end = 12.dp).size(14.dp).clip(androidx.compose.foundation.shape.CircleShape).background(Color(LibraryOrg.tagColor(p.name))))
                        Heading(title, 30.sp)
                    }
                    all?.let { a ->
                        val folders = LibraryOrg.allFolders(a.map { it.folder }, extraFolders).count { !it.contains('/') }
                        val facts = if (home) "${a.size} ${if (a.size == 1) "Notizbuch" else "Notizbücher"}" + if (folders > 0) " · $folders Ordner" else ""
                            else "${shown.size} ${if (shown.size == 1) "Notizbuch" else "Notizbücher"}" + if (subfolders.isNotEmpty()) " · ${subfolders.size} Ordner" else ""
                        Text(Voice.cased(facts), color = Ui.Muted, fontSize = 13.sp, fontFamily = Voice.metaFont, letterSpacing = Voice.labelSpacing)
                    }
                    busy?.let { Text(it, color = Ui.Red, fontFamily = Voice.metaFont, fontSize = 13.sp, modifier = Modifier.padding(top = 6.dp)) }
                }
                if (!all.isNullOrEmpty()) {
                    SearchBox(Modifier.width(230.dp))
                    Spacer(Modifier.width(10.dp))
                    SmallChoice("Zuletzt", !byName) { byName = false; Settings.set("lib_sort", "modified") }
                    Spacer(Modifier.width(4.dp))
                    SmallChoice("Name", byName) { byName = true; Settings.set("lib_sort", "name") }
                    Spacer(Modifier.width(8.dp))
                    ViewBtn(Glyph.GRID, !asList) { asList = false; Settings.set("lib_view", "grid") }
                    ViewBtn(Glyph.LIST, asList) { asList = true; Settings.set("lib_view", "list") }
                    Spacer(Modifier.width(6.dp))
                    Btn(if (picked != null) "Fertig" else "Auswählen", BtnKind.QUIET) { picked = if (picked != null) null else emptySet(); menuFor = null }
                }
            }
            when {
                all == null -> Text("Lade…", color = Ui.Muted, fontSize = 14.sp, modifier = Modifier.padding(32.dp))
                all.isEmpty() -> Box(Modifier.padding(32.dp)) { EmptyState(onCreate) }
                else -> {
                  val grid = androidx.compose.foundation.lazy.grid.rememberLazyGridState()
                  // A new place starts at its top; the recent pages (loaded after the list) don't push it down.
                  androidx.compose.runtime.LaunchedEffect(p, searching) { grid.scrollToItem(0) }
                  androidx.compose.runtime.LaunchedEffect(recentPages.size) { if (grid.firstVisibleItemIndex <= 3) grid.scrollToItem(0) }
                  LazyVerticalGrid(
                    state = grid,
                    columns = if (asList) GridCells.Fixed(1) else GridCells.Adaptive(190.dp),
                    contentPadding = androidx.compose.foundation.layout.PaddingValues(start = 32.dp, end = 32.dp, top = 14.dp, bottom = 110.dp),
                    horizontalArrangement = Arrangement.spacedBy(20.dp), verticalArrangement = Arrangement.spacedBy(if (asList) 6.dp else 20.dp),
                    modifier = Modifier.fillMaxSize(),
                ) {
                    if (home) homeItems(all, order) else {
                        if (asList) items(subfolders, key = { "folder:$it" }) { f -> FolderTile(f, all) }
                        else items(subfolders, key = { "folder:$it" }) { f -> FolderTile(f, all) }
                        notebooks(shown, showFolder = searching || p !is LibraryOrg.Place.Folder)
                        if (shown.isEmpty() && subfolders.isEmpty()) item(span = { GridItemSpan(maxLineSpan) }) {
                            Text(if (searching) "Nichts gefunden." else "Hier ist noch nichts.", color = Ui.Muted, fontSize = 14.sp)
                        }
                    }
                  }
                }
            }
        }
    }

    /** Notebooks as tiles or rows (whichever view is chosen). */
    private fun androidx.compose.foundation.lazy.grid.LazyGridScope.notebooks(es: List<Entry>, showFolder: Boolean) {
        items(es, key = { it.id }) { e -> if (asList) NotebookRow(e, showFolder) else Card(e, showFolder) }
    }

    private fun androidx.compose.foundation.lazy.grid.LazyGridScope.section(title: String, count: Int? = null) {
        item(span = { GridItemSpan(maxLineSpan) }, key = "section:$title") {
            Row(Modifier.padding(top = 12.dp), verticalAlignment = Alignment.CenterVertically) {
                Caption(title)
                count?.let { Text("$it", color = Ui.Muted.copy(alpha = 0.8f), fontSize = 12.sp, fontFamily = Voice.metaFont, modifier = Modifier.padding(start = 10.dp)) }
                Box(Modifier.padding(start = 14.dp).weight(1f).height(1.dp).background(Ui.Border))
            }
        }
    }

    /**
     * The start: what was opened last (a row), the pinned ones, the folders, the notebooks in no
     * folder. Everything else is a tap away in the sidebar.
     */
    private fun androidx.compose.foundation.lazy.grid.LazyGridScope.homeItems(all: List<Entry>, order: Comparator<Entry>) {
        // The pages written on last, here or on another device: back to exactly that page.
        val pages = recentPages
        if (pages.isNotEmpty()) {
            section("Zuletzt bearbeitet")
            item(span = { GridItemSpan(maxLineSpan) }, key = "edited") {
                androidx.compose.foundation.lazy.LazyRow(horizontalArrangement = Arrangement.spacedBy(16.dp)) {
                    items(pages.size, key = { "edited:${pages[it].notebook}:${pages[it].sheet}" }) { i -> RecentPageCard(pages[i]) }
                }
            }
        }
        val opened = LibraryOrg.opened(Settings.str("lib_opened", ""))
        val recent = all.sortedByDescending { opened[it.id] ?: it.modified }.take(8)
        if (all.size > 3) {
            section("Zuletzt geöffnet")
            item(span = { GridItemSpan(maxLineSpan) }, key = "recent") {
                androidx.compose.foundation.lazy.LazyRow(horizontalArrangement = Arrangement.spacedBy(16.dp)) {
                    items(recent.size, key = { "recent:${recent[it].id}" }) { i -> Box(Modifier.width(150.dp)) { Card(recent[i], showFolder = true, small = true) } }
                }
            }
        }
        val pins = all.filter { it.pinned }.sortedWith(order)
        if (pins.isNotEmpty()) { section("Angeheftet", pins.size); notebooks(pins, showFolder = true) }
        val top = LibraryOrg.allFolders(all.map { it.folder }, extraFolders).filter { !it.contains('/') }
        section("Ordner", top.size)
        items(top, key = { "folder:$it" }) { f -> FolderTile(f, all) }
        item(key = "newfolder") { NewFolderTile() }
        val loose = all.filter { it.folder.isEmpty() && !it.pinned }.sortedWith(order)
        if (loose.isNotEmpty()) { section("Ohne Ordner", loose.size); notebooks(loose, showFolder = false) }
    }

    /** A page changed lately: its picture, its name, the notebook and when. */
    @Composable
    private fun RecentPageCard(p: RecentPage) {
        val img by androidx.compose.runtime.produceState<androidx.compose.ui.graphics.ImageBitmap?>(null, p.picture) {
            value = kotlinx.coroutines.withContext(kotlinx.coroutines.Dispatchers.IO) { runCatching { android.graphics.BitmapFactory.decodeFile(p.picture)?.asImageBitmap() }.getOrNull() }
        }
        val h = 170.dp
        val w = (h / p.aspect).coerceIn(110.dp, 260.dp)
        Column(Modifier.width(w).clip(RoundedCornerShape(12.dp)).clickable { open(p.notebook, p.sheet) }) {
            Box(Modifier.size(w, h).clip(RoundedCornerShape(10.dp)).background(Color.White).border(1.dp, Ui.Border, RoundedCornerShape(10.dp))) {
                img?.let { androidx.compose.foundation.Image(it, null, Modifier.fillMaxSize(), contentScale = androidx.compose.ui.layout.ContentScale.Crop, alignment = Alignment.TopCenter) }
            }
            Text(p.label, color = Ui.Chalk, fontSize = 13.sp, fontWeight = FontWeight.Medium, maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.padding(top = 7.dp))
            Text(Voice.cased("${p.notebookTitle} · ${age(p.changed)}"), color = Ui.Muted, fontSize = 11.sp, fontFamily = Voice.metaFont, maxLines = 1, overflow = TextOverflow.Ellipsis)
        }
    }

    @Composable
    private fun ViewBtn(g: Glyph, on: Boolean, onClick: () -> Unit) {
        Box(Modifier.size(36.dp).clip(RoundedCornerShape(9.dp)).background(if (on) Ui.Elevated else Color.Transparent).clickable(onClick = onClick), contentAlignment = Alignment.Center) {
            GlyphIcon(g, if (on) Ui.Chalk else Ui.Muted, 18.dp)
        }
    }

    @Composable
    private fun NewFolderTile() {
        Row(Modifier.clip(RoundedCornerShape(minOf(Ui.theme.style.card, 18f).dp)).border(1.dp, Ui.Border, RoundedCornerShape(minOf(Ui.theme.style.card, 18f).dp))
            .clickable { newFolderIn = "" }.padding(horizontal = 16.dp, vertical = 18.dp), verticalAlignment = Alignment.CenterVertically) {
            GlyphIcon(Glyph.PLUS, Ui.Muted, 22.dp)
            Text("Neuer Ordner", color = Ui.Muted, fontSize = 14.sp, modifier = Modifier.padding(start = 12.dp))
        }
    }

    /**
     * A notebook's touch: tap opens it (or picks it while picking), hold opens its menu, hold and
     * drag takes it (with the others picked) onto a folder, a tag or "pinned" in the sidebar.
     */
    @Composable
    private fun Modifier.notebookTouch(e: Entry, toggle: () -> Unit): Modifier {
        val haptic = androidx.compose.ui.platform.LocalHapticFeedback.current
        var origin by remember { mutableStateOf(androidx.compose.ui.geometry.Offset.Zero) }
        return this.onGloballyPositioned { origin = it.positionInRoot() }.pointerInput(e.id) {
            awaitEachGesture {
                val down = awaitFirstDown(requireUnconsumed = false)
                val long = awaitLongPressOrCancellation(down.id)
                if (long == null) {
                    val up = currentEvent.changes.firstOrNull { it.id == down.id }
                    // Not when a button on the card (⋯) took the tap.
                    if (up != null && up.changedToUp() && (up.position - down.position).getDistance() < viewConfiguration.touchSlop) {
                        if (picked != null) toggle() else open(e.id)
                    }
                    return@awaitEachGesture
                }
                haptic.performHapticFeedback(androidx.compose.ui.hapticfeedback.HapticFeedbackType.LongPress)
                val start = origin + long.position
                val sel = picked
                val group = if (sel != null && e.id in sel) entries.orEmpty().filter { it.id in sel } else listOf(e)
                var moved = false
                drag(long.id) { ch ->
                    ch.consume()
                    val at = origin + ch.position
                    if (!moved && (at - start).getDistance() > viewConfiguration.touchSlop * 2) { moved = true; dragging = group; menuFor = null }
                    if (moved) { dragAt = at; dropOn = targets.entries.firstOrNull { it.value.contains(at) }?.key }
                }
                if (moved) {
                    dropOn?.let { dropTo(it, group) }
                    dragging = null; dropOn = null
                } else if (picked == null) menuFor = e.id else toggle()
            }
        }
    }

    /** Dropped onto a sidebar entry: into a folder (or none), a tag on, pinned. */
    private fun dropTo(key: String, group: List<Entry>) {
        when {
            key == "top" -> change(group.filter { it.folder.isNotEmpty() }.map { it.id }) { _, nb -> nb.setFolder("") }
            key == "pin" -> change(group.filter { !it.pinned }.map { it.id }) { _, nb -> nb.setPinned(true) }
            key.startsWith("folder:") -> {
                val path = key.removePrefix("folder:")
                change(group.filter { it.folder != path }.map { it.id }) { _, nb -> nb.setFolder(path) }
            }
            key.startsWith("tag:") -> {
                val t = key.removePrefix("tag:")
                change(group.filter { e -> e.tags.none { it.equals(t, ignoreCase = true) } }.map { it.id }) { id, nb -> nb.setTags(group.first { it.id == id }.tags + t) }
            }
        }
        picked?.let { picked = emptySet() }
    }

    /** A notebook as a row of the list: small cover, title, where and how big, tags, menu. */
    @Composable
    private fun NotebookRow(e: Entry, showFolder: Boolean) {
        val sel = picked
        val chosen = sel?.contains(e.id) == true
        fun toggle() { picked = sel?.let { if (chosen) it - e.id else it + e.id } }
        Box {
            Row(Modifier.fillMaxWidth().clip(RoundedCornerShape(12.dp)).background(if (chosen) Ui.Elevated else Ui.Surface)
                .then(if (chosen) Modifier.border(1.5.dp, Ui.Red, RoundedCornerShape(12.dp)) else Modifier)
                .notebookTouch(e) { toggle() }.padding(horizontal = 12.dp, vertical = 8.dp), verticalAlignment = Alignment.CenterVertically) {
                if (sel != null) Box(Modifier.padding(end = 12.dp).size(22.dp).clip(androidx.compose.foundation.shape.CircleShape).background(if (chosen) Ui.Red else Color.Transparent)
                    .border(1.5.dp, if (chosen) Ui.Red else Ui.Muted, androidx.compose.foundation.shape.CircleShape), contentAlignment = Alignment.Center) { if (chosen) GlyphIcon(Glyph.CHECK, Ui.OnAccent, 14.dp) }
                CoverArt(e.cover, e.title, e.id, Modifier.size(width = 40.dp, height = 52.dp))
                Column(Modifier.weight(1f).padding(start = 14.dp)) {
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        if (e.pinned) Box(Modifier.padding(end = 6.dp)) { GlyphIcon(Glyph.PIN, Ui.Muted, 14.dp) }
                        Text(e.title, color = Ui.Chalk, fontSize = 15.sp, fontFamily = Voice.titleFont, fontWeight = FontWeight.Medium, maxLines = 1, overflow = TextOverflow.Ellipsis)
                    }
                    val meta = (if (showFolder && e.folder.isNotEmpty()) "${e.folder.replace("/", " › ")} · " else "") + "${e.sheets} ${if (e.sheets == 1) "Seite" else "Seiten"}" + if (e.modified > 0) " · ${age(e.modified)}" else ""
                    Text(Voice.cased(meta), color = Ui.Muted, fontSize = 12.sp, fontFamily = Voice.metaFont, maxLines = 1, overflow = TextOverflow.Ellipsis)
                }
                e.tags.take(4).forEach { t ->
                    Row(Modifier.padding(start = 10.dp), verticalAlignment = Alignment.CenterVertically) {
                        Box(Modifier.size(8.dp).clip(androidx.compose.foundation.shape.CircleShape).background(Color(LibraryOrg.tagColor(t))))
                        Text(t, color = Ui.Muted, fontSize = 12.sp, maxLines = 1, modifier = Modifier.padding(start = 5.dp).widthIn(max = 100.dp), overflow = TextOverflow.Ellipsis)
                    }
                }
                if (sel == null) Box(Modifier.padding(start = 10.dp).size(34.dp).clip(RoundedCornerShape(10.dp)).clickable { menuFor = e.id }, contentAlignment = Alignment.Center) {
                    Text("⋯", color = Ui.Chalk, fontSize = 16.sp)
                }
            }
            if (menuFor == e.id) CardMenu(e)
        }
    }

    @Composable
    private fun SearchBox(modifier: Modifier) {
        val st = Ui.theme.style
        val shape = RoundedCornerShape(minOf(st.button, 12f).dp)
        Row(modifier.clip(shape).background(if (st.fill == Fill.TONAL) Voice.tonal else Ui.Black).border(1.dp, Ui.Border, shape).padding(horizontal = 12.dp, vertical = 9.dp),
            verticalAlignment = Alignment.CenterVertically) {
            GlyphIcon(Glyph.SEARCH, Ui.Muted, 17.dp)
            Box(Modifier.weight(1f).padding(start = 8.dp)) {
                if (query.isEmpty()) Text("Suchen", color = Ui.Muted, fontSize = 14.sp)
                BasicTextField(query, { query = it }, singleLine = true, textStyle = TextStyle(color = Ui.Chalk, fontSize = 14.sp), cursorBrush = SolidColor(Ui.Red), modifier = Modifier.fillMaxWidth())
            }
            if (query.isNotEmpty()) Text("✕", color = Ui.Muted, fontSize = 13.sp, modifier = Modifier.clip(RoundedCornerShape(6.dp)).clickable { query = "" }.padding(horizontal = 4.dp))
        }
    }

    /** A subfolder in the grid: tap opens it, hold its menu. */
    @OptIn(androidx.compose.foundation.ExperimentalFoundationApi::class)
    @Composable
    private fun FolderTile(path: String, all: List<Entry>) {
        val n = all.count { it.folder == path || it.folder.startsWith("$path/") }
        Row(Modifier.cardBox().combinedClickable(onLongClick = { folderFor = path }) { place = LibraryOrg.Place.Folder(path) }.padding(horizontal = 16.dp, vertical = 18.dp),
            verticalAlignment = Alignment.CenterVertically) {
            GlyphIcon(Glyph.FOLDER, Ui.Chalk, 26.dp)
            Column(Modifier.padding(start = 12.dp)) {
                Text(LibraryOrg.name(path), color = Ui.Chalk, fontSize = 15.sp, fontWeight = FontWeight.Medium, maxLines = 1, overflow = TextOverflow.Ellipsis)
                Text(Voice.cased("$n ${if (n == 1) "Notizbuch" else "Notizbücher"}"), color = Ui.Muted, fontSize = 12.sp, fontFamily = Voice.metaFont)
            }
        }
    }

    /** Picking several notebooks: what to do with all of them. */
    @Composable
    private fun SelectBar(sel: Set<String>, list: List<Entry>, modifier: Modifier) {
        val chosen = list.filter { it.id in sel }
        var sure by remember { mutableStateOf(false) }
        Row(modifier.bar(24f).padding(horizontal = 14.dp, vertical = 8.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            Text(Voice.cased("${sel.size} ausgewählt"), color = Ui.Chalk, fontSize = 13.sp, fontFamily = Voice.metaFont, modifier = Modifier.padding(end = 8.dp))
            val any = chosen.isNotEmpty()
            BarBtn(Glyph.FOLDER, "Verschieben", any) { moving = chosen }
            BarBtn(Glyph.TAG, "Tags", any) { tagging = chosen }
            val allPinned = any && chosen.all { it.pinned }
            BarBtn(Glyph.PIN, if (allPinned) "Lösen" else "Anheften", any) { change(chosen.map { it.id }) { _, nb -> nb.setPinned(!allPinned) } }
            BarBtn(Glyph.TRASH, if (sure) "Wirklich?" else "Löschen", any) {
                if (!sure) sure = true else { sure = false; chosen.forEach { trash(it) }; picked = emptySet() }
            }
            Box(Modifier.padding(horizontal = 6.dp).width(1.dp).height(22.dp).background(Ui.Border))
            BarBtn(Glyph.CLOSE, "Fertig", true) { picked = null }
        }
    }

    @Composable
    private fun BarBtn(g: Glyph, label: String, enabled: Boolean, onClick: () -> Unit) {
        Column(Modifier.clip(RoundedCornerShape(12.dp)).clickable(enabled = enabled, onClick = onClick).padding(horizontal = 8.dp, vertical = 2.dp),
            horizontalAlignment = Alignment.CenterHorizontally) {
            GlyphIcon(g, if (enabled) Ui.Chalk else Ui.Muted.copy(alpha = 0.5f), 20.dp)
            Text(label, color = Ui.Muted, fontSize = 10.sp)
        }
    }

    /** A notebook's menu, light: one short line per action. */
    @Composable
    private fun CardMenu(e: Entry) {
        val close = { menuFor = null; sureTrash = false }
        val density = androidx.compose.ui.platform.LocalDensity.current
        androidx.compose.ui.window.Popup(alignment = Alignment.TopEnd, offset = with(density) { androidx.compose.ui.unit.IntOffset(-8.dp.roundToPx(), 44.dp.roundToPx()) },
            onDismissRequest = close, properties = androidx.compose.ui.window.PopupProperties(focusable = true)) {
            N0teType {
                Column(Modifier.width(220.dp).panel().padding(vertical = 6.dp)) {
                    MenuRow(Glyph.PEN, "Umbenennen") { close(); renaming = e }
                    MenuRow(Glyph.FOLDER, "Verschieben …") { close(); moving = listOf(e) }
                    MenuRow(Glyph.TAG, "Tags …") { close(); tagging = listOf(e) }
                    MenuRow(Glyph.PIN, if (e.pinned) "Lösen" else "Anheften") { close(); change(listOf(e.id)) { _, nb -> nb.setPinned(!e.pinned) } }
                    MenuRow(Glyph.PAGE, "Cover …") { close(); covering = e }
                    MenuRow(Glyph.LAPTOP, "An den Laptop") { close(); sendToLaptop(e) }
                    Box(Modifier.padding(vertical = 4.dp, horizontal = 14.dp).fillMaxWidth().height(1.dp).background(Ui.Border))
                    MenuRow(Glyph.TRASH, if (sureTrash) "Wirklich löschen?" else "Löschen", danger = true) { if (sureTrash) { close(); trash(e) } else sureTrash = true }
                }
            }
        }
    }

    @Composable
    private fun MenuRow(g: Glyph, label: String, danger: Boolean = false, onClick: () -> Unit) {
        Row(Modifier.fillMaxWidth().clickable(onClick = onClick).padding(horizontal = 16.dp, vertical = 10.dp), verticalAlignment = Alignment.CenterVertically) {
            GlyphIcon(g, if (danger) Ui.Red else Ui.Chalk.copy(alpha = 0.85f), 18.dp)
            Text(label, color = if (danger) Ui.Red else Ui.Chalk, fontSize = 14.sp, modifier = Modifier.padding(start = 14.dp))
        }
    }

    /** A small dialog: heading, content, buttons at the bottom right. */
    @Composable
    private fun SmallDialog(title: String, onDismiss: () -> Unit, width: androidx.compose.ui.unit.Dp = 440.dp, content: @Composable () -> Unit) {
        Dialog(onDismissRequest = onDismiss) {
            N0teType {
                Column(Modifier.width(width).panel().padding(24.dp), verticalArrangement = Arrangement.spacedBy(14.dp)) {
                    Heading(title, 20.sp)
                    content()
                }
            }
        }
    }

    @Composable
    private fun RenameDialog(e: Entry) {
        var title by remember(e.id) { mutableStateOf(e.title) }
        val save = { renaming = null; change(listOf(e.id)) { _, nb -> nb.setTitle(title.trim().ifBlank { "Ohne Titel" }) } }
        SmallDialog("Umbenennen", { renaming = null }) {
            Field(title, "Ohne Titel") { title = it }
            Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.End) {
                Btn("Abbrechen", BtnKind.QUIET) { renaming = null }
                Spacer(Modifier.width(8.dp))
                Btn("Speichern", BtnKind.PRIMARY) { save() }
            }
        }
    }

    @OptIn(androidx.compose.foundation.layout.ExperimentalLayoutApi::class)
    @Composable
    private fun CoverDialog(e: Entry) {
        SmallDialog("Cover", { covering = null }, 480.dp) {
            androidx.compose.foundation.layout.FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                val now = e.cover.ifBlank { "page" }
                COVERS.forEach { (id, label) ->
                    Column(horizontalAlignment = Alignment.CenterHorizontally, modifier = Modifier.clip(RoundedCornerShape(10.dp)).clickable {
                        covering = null
                        change(listOf(e.id)) { _, nb -> nb.setCover(id) }
                    }.padding(2.dp)) {
                        CoverArt(id, e.title, e.id, Modifier.size(width = 64.dp, height = 82.dp).border(2.dp, if (now == id) Ui.Red else Color.Transparent, RoundedCornerShape(maxOf(0f, minOf(Ui.theme.style.card - 8f, 10f)).dp)))
                        Text(label, color = if (now == id) Ui.Chalk else Ui.Muted, fontSize = 11.sp, modifier = Modifier.padding(top = 3.dp))
                    }
                }
            }
        }
    }

    /** Where notebooks go: a folder from the tree (or none), or a new one made right here. */
    @Composable
    private fun MoveDialog(es: List<Entry>, list: List<Entry>) {
        val common = es.map { it.folder }.distinct().singleOrNull() ?: ""
        var target by remember { mutableStateOf(common) }
        var newName by remember { mutableStateOf("") }
        val folders = LibraryOrg.tree(list.map { it.folder }.filter { it.isNotEmpty() }, extraFolders)
        SmallDialog(if (es.size == 1) "„${es[0].title}“ verschieben" else "${es.size} Notizbücher verschieben", { moving = null }, 460.dp) {
            Column(Modifier.heightIn(max = 340.dp).verticalScroll(rememberScrollState())) {
                FolderPick("Ohne Ordner", 0, target == "") { target = "" }
                folders.forEach { n -> FolderPick(n.name, n.depth + 1, target == n.path) { target = n.path } }
            }
            Row(verticalAlignment = Alignment.CenterVertically) {
                Box(Modifier.weight(1f)) { Field(newName, if (target.isEmpty()) "Neuer Ordner" else "Neuer Ordner in „${LibraryOrg.name(target)}“") { newName = it } }
                Spacer(Modifier.width(8.dp))
                Btn("Anlegen", enabled = newName.isNotBlank()) {
                    val path = LibraryOrg.clean(if (target.isEmpty()) newName else "$target/$newName")
                    if (path !in extraFolders) { extraFolders += path; keepFolders() }
                    target = path; newName = ""
                }
            }
            Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.End) {
                Btn("Abbrechen", BtnKind.QUIET) { moving = null }
                Spacer(Modifier.width(8.dp))
                Btn("Hierher", BtnKind.PRIMARY) {
                    moving = null
                    val to = target
                    if (extraFolders.remove(to)) keepFolders()
                    change(es.filter { it.folder != to }.map { it.id }) { _, nb -> nb.setFolder(to) }
                    picked?.let { picked = emptySet() }
                }
            }
        }
    }

    @Composable
    private fun FolderPick(name: String, depth: Int, on: Boolean, onClick: () -> Unit) {
        Row(Modifier.fillMaxWidth().clip(RoundedCornerShape(10.dp)).background(if (on) Ui.Elevated else Color.Transparent).clickable(onClick = onClick)
            .padding(start = (10 + depth * 16).dp, end = 10.dp, top = 9.dp, bottom = 9.dp), verticalAlignment = Alignment.CenterVertically) {
            GlyphIcon(if (depth == 0) Glyph.PAGE else Glyph.FOLDER, if (on) Ui.Chalk else Ui.Muted, 18.dp)
            Text(name, color = Ui.Chalk, fontSize = 14.sp, fontWeight = if (on) FontWeight.Medium else FontWeight.Normal, modifier = Modifier.weight(1f).padding(start = 12.dp))
            if (on) GlyphIcon(Glyph.CHECK, Ui.Red, 16.dp)
        }
    }

    /** Tags for notebooks: the library's tags to switch on and off (for all of them), a new one to add. */
    @OptIn(androidx.compose.foundation.layout.ExperimentalLayoutApi::class)
    @Composable
    private fun TagsDialog(es: List<Entry>, list: List<Entry>) {
        // What the dialog shows changes at once; the notebooks follow in the background.
        var have by remember { mutableStateOf(es.associate { it.id to it.tags }) }
        var newTag by remember { mutableStateOf("") }
        val known = (tagsOf(list).map { it.first } + have.values.flatten()).distinctBy { it.lowercase() }.sortedBy { it.lowercase() }
        fun apply(next: Map<String, List<String>>) {
            val changed = next.filter { (id, t) -> t != have[id] }
            have = next
            change(changed.keys) { id, nb -> nb.setTags(changed.getValue(id)) }
        }
        SmallDialog(if (es.size == 1) "Tags für „${es[0].title}“" else "Tags für ${es.size} Notizbücher", { tagging = null; picked?.let { picked = emptySet() } }, 480.dp) {
            if (known.isEmpty()) Text("Noch keine Tags. Einfach unten einen eintippen.", color = Ui.Muted, fontSize = 13.sp)
            androidx.compose.foundation.layout.FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                known.forEach { t ->
                    val on = have.values.all { tags -> tags.any { it.equals(t, ignoreCase = true) } }
                    Row(Modifier.clip(RoundedCornerShape(16.dp)).background(if (on) Ui.Elevated else Color.Transparent).border(1.dp, if (on) Color(LibraryOrg.tagColor(t)) else Ui.Border, RoundedCornerShape(16.dp))
                        .clickable {
                            apply(have.mapValues { (_, tags) -> if (on) tags.filterNot { it.equals(t, ignoreCase = true) } else if (tags.any { it.equals(t, ignoreCase = true) }) tags else tags + t })
                        }.padding(horizontal = 12.dp, vertical = 7.dp), verticalAlignment = Alignment.CenterVertically) {
                        Box(Modifier.size(9.dp).clip(androidx.compose.foundation.shape.CircleShape).background(Color(LibraryOrg.tagColor(t))))
                        Text(t, color = Ui.Chalk, fontSize = 13.sp, modifier = Modifier.padding(start = 8.dp))
                        if (on) Text("  ✓", color = Ui.Chalk, fontSize = 12.sp)
                    }
                }
            }
            Row(verticalAlignment = Alignment.CenterVertically) {
                Box(Modifier.weight(1f)) { Field(newTag, "Neuer Tag, z. B. Prüfung") { newTag = it } }
                Spacer(Modifier.width(8.dp))
                Btn("Dazu", enabled = newTag.isNotBlank()) {
                    val t = newTag.trim()
                    newTag = ""
                    apply(have.mapValues { (_, tags) -> if (tags.any { it.equals(t, ignoreCase = true) }) tags else tags + t })
                }
            }
            Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.End) {
                Btn("Fertig", BtnKind.PRIMARY) { tagging = null; picked?.let { picked = emptySet() } }
            }
        }
    }

    /** A folder's menu (held in the sidebar or as a tile): rename, a folder inside, dissolve. */
    @Composable
    private fun FolderDialog(path: String, list: List<Entry>) {
        var name by remember(path) { mutableStateOf(LibraryOrg.name(path)) }
        var sure by remember(path) { mutableStateOf(false) }
        val inside = list.filter { LibraryOrg.renamed(it.folder, path, path) != null }
        SmallDialog("Ordner „${LibraryOrg.name(path)}“", { folderFor = null }) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Box(Modifier.weight(1f)) { Field(name, "Name") { name = it.replace("/", " ") } }
                Spacer(Modifier.width(8.dp))
                Btn("Umbenennen", enabled = name.isNotBlank() && name.trim() != LibraryOrg.name(path)) {
                    val to = LibraryOrg.clean(LibraryOrg.parent(path) + "/" + name)
                    folderFor = null
                    // Empty folders and the place shown go along.
                    extraFolders.replaceAll { LibraryOrg.renamed(it, path, to) ?: it }
                    keepFolders()
                    (place as? LibraryOrg.Place.Folder)?.let { f -> LibraryOrg.renamed(f.path, path, to)?.let { place = LibraryOrg.Place.Folder(it) } }
                    change(inside.map { it.id }) { id, nb -> LibraryOrg.renamed(list.first { it.id == id }.folder, path, to)?.let { nb.setFolder(it) } }
                }
            }
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                Btn("Neuer Ordner darin") { folderFor = null; newFolderIn = path }
                Btn(if (sure) "Wirklich auflösen?" else "Auflösen", BtnKind.DANGER) {
                    if (!sure) { sure = true; return@Btn }
                    folderFor = null
                    extraFolders.removeAll { it == path || it.startsWith("$path/") }
                    keepFolders()
                    if ((place as? LibraryOrg.Place.Folder)?.path?.let { it == path || it.startsWith("$path/") } == true) place = LibraryOrg.Place.Folder(LibraryOrg.parent(path)).takeIf { LibraryOrg.parent(path).isNotEmpty() } ?: LibraryOrg.Place.Home
                    change(inside.map { it.id }) { id, nb -> LibraryOrg.dissolved(list.first { it.id == id }.folder, path)?.let { nb.setFolder(it) } }
                }
            }
            if (sure) Text("Die ${inside.size} Notizbücher darin kommen eine Ebene höher. Gelöscht wird nichts.", color = Ui.Muted, fontSize = 12.sp)
        }
    }

    @Composable
    private fun NewFolderDialog(parent: String) {
        var name by remember { mutableStateOf("") }
        SmallDialog(if (parent.isEmpty()) "Neuer Ordner" else "Neuer Ordner in „${LibraryOrg.name(parent)}“", { newFolderIn = null }) {
            Field(name, "z. B. WS26") { name = it.replace("/", " ") }
            Text("Notizbücher kommen hinein, indem du sie auf den Ordner ziehst oder „Verschieben“ wählst.", color = Ui.Muted, fontSize = 12.sp)
            Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.End) {
                Btn("Abbrechen", BtnKind.QUIET) { newFolderIn = null }
                Spacer(Modifier.width(8.dp))
                Btn("Anlegen", BtnKind.PRIMARY, enabled = name.isNotBlank()) {
                    val path = LibraryOrg.clean("$parent/$name")
                    if (path !in extraFolders) { extraFolders += path; keepFolders() }
                    newFolderIn = null
                    place = LibraryOrg.Place.Folder(path); query = ""
                }
            }
        }
    }

    /** A tag's menu (held in the sidebar): rename it everywhere, or take it off every notebook. */
    @Composable
    private fun TagDialog(tag: String, list: List<Entry>) {
        var name by remember(tag) { mutableStateOf(tag) }
        val with = list.filter { e -> e.tags.any { it.equals(tag, ignoreCase = true) } }
        SmallDialog("Tag „$tag“", { tagFor = null }) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Box(Modifier.weight(1f)) { Field(name, "Name") { name = it } }
                Spacer(Modifier.width(8.dp))
                Btn("Umbenennen", enabled = name.isNotBlank() && name.trim() != tag) {
                    val to = name.trim()
                    tagFor = null
                    if ((place as? LibraryOrg.Place.Tag)?.name?.equals(tag, ignoreCase = true) == true) place = LibraryOrg.Place.Tag(to)
                    change(with.map { it.id }) { id, nb -> nb.setTags(list.first { it.id == id }.tags.map { if (it.equals(tag, ignoreCase = true)) to else it }) }
                }
            }
            Btn("Von allen ${with.size} Notizbüchern entfernen", BtnKind.DANGER) {
                tagFor = null
                if ((place as? LibraryOrg.Place.Tag)?.name?.equals(tag, ignoreCase = true) == true) place = LibraryOrg.Place.All
                change(with.map { it.id }) { id, nb -> nb.setTags(list.first { it.id == id }.tags.filterNot { it.equals(tag, ignoreCase = true) }) }
            }
        }
    }

    /** "● Laptop verbunden" once a laptop is paired. */
    @Composable
    private fun SyncChip() {
        val l = SyncHub.laptop ?: return
        val st = Ui.theme.style
        val shape = RoundedCornerShape(minOf(st.button, 12f).dp)
        Row(
            Modifier.clip(shape).background(if (st.fill == Fill.TONAL) Voice.tonal else Ui.Surface)
                .then(if (Voice.line.alpha > 0f) Modifier.border(1.dp, Voice.line, shape) else Modifier)
                .clickable { startActivity(Intent(this@LibraryActivity, SettingsActivity::class.java)) }
                .padding(horizontal = 12.dp, vertical = 8.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Box(Modifier.size(8.dp).clip(androidx.compose.foundation.shape.CircleShape).background(if (l.online) Ui.Red else Ui.Muted))
            Text(Voice.cased(if (l.online) "Laptop verbunden" else "Laptop nicht erreichbar"), color = if (l.online) Ui.Chalk else Ui.Muted,
                fontSize = if (Voice.mono) 11.sp else 13.sp, fontFamily = Voice.metaFont, letterSpacing = Voice.labelSpacing, modifier = Modifier.padding(start = 8.dp))
        }
    }

    @Composable
    private fun IconBtn(g: Glyph, onClick: () -> Unit) {
        val st = Ui.theme.style
        val shape = RoundedCornerShape(minOf(st.button, 21f).dp)
        Box(
            Modifier.size(42.dp).clip(shape).background(if (st.fill == Fill.TONAL) Voice.tonal else Color.Transparent)
                .then(if (Voice.line.alpha > 0f) Modifier.border(1.dp, if (st.fill == Fill.RULE) Ui.Chalk.copy(alpha = 0.5f) else Ui.Border, shape) else Modifier)
                .clickable(onClick = onClick),
            contentAlignment = Alignment.Center,
        ) { GlyphIcon(g, Ui.Chalk, 20.dp) }
    }

    /**
     * A notebook: its cover, title, size and age, its tags. Tap opens, hold (or ⋯) opens its
     * menu; while picking several, a tap picks it.
     */
    @OptIn(androidx.compose.foundation.ExperimentalFoundationApi::class)
    @Composable
    private fun Card(e: Entry, showFolder: Boolean, small: Boolean = false) {
        val st = Ui.theme.style
        val sel = picked
        val chosen = sel?.contains(e.id) == true
        fun toggle() { picked = sel?.let { if (chosen) it - e.id else it + e.id } }
        Box {
            Column(Modifier.cardBox().then(if (chosen) Modifier.border(2.dp, Ui.Red, RoundedCornerShape(minOf(st.card, 18f).dp)) else Modifier)
                .notebookTouch(e) { toggle() }.padding(if (small) 9.dp else 12.dp)) {
                Box {
                    CoverArt(e.cover, e.title, e.id, Modifier.fillMaxWidth().aspectRatio(0.78f))
                    if (sel != null) {
                        Box(Modifier.align(Alignment.TopStart).padding(6.dp).size(28.dp).clip(androidx.compose.foundation.shape.CircleShape)
                            .background(if (chosen) Ui.Red else Ui.Bar).border(1.5.dp, if (chosen) Ui.Red else Ui.Chalk.copy(alpha = 0.6f), androidx.compose.foundation.shape.CircleShape),
                            contentAlignment = Alignment.Center) { if (chosen) GlyphIcon(Glyph.CHECK, Ui.OnAccent, 16.dp) }
                    } else {
                        if (e.pinned) Box(Modifier.align(Alignment.TopStart).padding(6.dp).size(28.dp).clip(RoundedCornerShape(minOf(st.button, 14f).dp)).background(Ui.Bar), contentAlignment = Alignment.Center) {
                            GlyphIcon(Glyph.PIN, Ui.Chalk, 16.dp)
                        }
                        Box(Modifier.align(Alignment.TopEnd).padding(6.dp).size(32.dp).clip(RoundedCornerShape(minOf(st.button, 16f).dp)).background(Ui.Bar).clickable { menuFor = e.id },
                            contentAlignment = Alignment.Center) { Text("⋯", color = Ui.Chalk, fontSize = 16.sp) }
                    }
                }
                Spacer(Modifier.height(10.dp))
                Text(e.title, color = Ui.Chalk, fontSize = if (small) 13.sp else 15.sp, fontFamily = Voice.titleFont, fontWeight = FontWeight.Medium, maxLines = 1, overflow = TextOverflow.Ellipsis)
                // Someone else's notebook (joined as a guest): whose, and read-only if so.
                val host = SyncHub.joinedRole(e.id)
                val meta = if (host != null) "Gast bei ${host.ownerName}" + if (host.role == uniffi.n0te_ffi.FfiRole.READ) " · nur lesen" else ""
                    else (if (showFolder && e.folder.isNotEmpty()) "${LibraryOrg.name(e.folder)} · " else "") +
                        "${e.sheets} ${if (e.sheets == 1) "Seite" else "Seiten"}" + if (e.modified > 0) " · ${age(e.modified)}" else ""
                Text(Voice.cased(meta), color = Ui.Muted, fontSize = if (Voice.mono) 11.sp else 12.sp, fontFamily = Voice.metaFont, letterSpacing = Voice.labelSpacing, maxLines = 1, overflow = TextOverflow.Ellipsis)
                if (e.tags.isNotEmpty() && !small) Row(Modifier.padding(top = 6.dp), horizontalArrangement = Arrangement.spacedBy(6.dp), verticalAlignment = Alignment.CenterVertically) {
                    e.tags.take(3).forEach { t ->
                        Row(verticalAlignment = Alignment.CenterVertically) {
                            Box(Modifier.size(7.dp).clip(androidx.compose.foundation.shape.CircleShape).background(Color(LibraryOrg.tagColor(t))))
                            Text(t, color = Ui.Muted, fontSize = 11.sp, maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.padding(start = 4.dp).widthIn(max = 70.dp))
                        }
                    }
                    if (e.tags.size > 3) Text("+${e.tags.size - 3}", color = Ui.Muted, fontSize = 11.sp)
                }
            }
            if (menuFor == e.id) CardMenu(e)
        }
    }

    private fun age(ms: Long): String = timeAgo(ms)

    /** At the top of the start screen: an update being downloaded, ready to install, installing. */
    @Composable
    private fun UpdateButton() {
        when (val st = Updater.state) {
            is Updater.State.Ready -> {
                Btn("Update ${st.release.version} installieren", BtnKind.PRIMARY) { Updater.install(this) }
                Spacer(Modifier.width(10.dp))
            }
            is Updater.State.Downloading -> {
                val pct = if (st.total > 0) " ${(st.done * 100 / st.total).toInt()} %" else ""
                Text(Voice.cased("Update ${st.release.version} lädt …$pct"), color = Ui.Muted, fontSize = 12.sp, fontFamily = Voice.metaFont)
                Spacer(Modifier.width(14.dp))
            }
            is Updater.State.Installing -> {
                Text(Voice.cased("Update wird installiert …"), color = Ui.Muted, fontSize = 12.sp, fontFamily = Voice.metaFont)
                Spacer(Modifier.width(14.dp))
            }
            else -> {}
        }
    }

    /** A new version is downloaded: install now (Android confirms) or later (the button stays). */
    @Composable
    private fun UpdateDialog(r: Updater.Release) {
        val later = { Updater.asked = r.version }
        Dialog(onDismissRequest = later) {
            N0teType {
                Column(Modifier.width(520.dp).panel().padding(26.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                    Caption("Update bereit")
                    Heading("n0te ${r.version}", 24.sp)
                    // The notes scroll however long they are; the buttons always stay in view.
                    Column(Modifier.weight(1f, fill = false).verticalScroll(rememberScrollState()), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                        Text("Installiert ist ${Updater.current(this@LibraryActivity)}. Die neue Version ist heruntergeladen und kann gleich installiert werden; n0te startet danach neu.",
                            color = Ui.Muted, fontSize = 14.sp)
                        for ((head, items) in Updater.noteSections(r.notes)) {
                            if (head.isNotEmpty()) Caption(head, Modifier.padding(top = 6.dp))
                            items.forEach { ChangeItem(it) }
                        }
                    }
                    Row(Modifier.fillMaxWidth().padding(top = 6.dp), verticalAlignment = Alignment.CenterVertically) {
                        Btn("Später", BtnKind.QUIET) { later() }
                        Spacer(Modifier.weight(1f))
                        Btn("Update installieren", BtnKind.PRIMARY) { Updater.asked = r.version; Updater.install(this@LibraryActivity) }
                    }
                }
            }
        }
    }

    /** What's new in this version (once after an install or update); all of it in the settings. */
    @Composable
    private fun NewsDialog(e: Changelog.Entry) {
        val close = { Changelog.markSeen(this); news = null }
        Dialog(onDismissRequest = close) {
            N0teType {
                Column(Modifier.width(520.dp).panel().padding(26.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                    Caption("Neu in Version ${e.version}")
                    Heading(e.title, 24.sp)
                    // A long list scrolls; the buttons always stay in view.
                    Column(Modifier.weight(1f, fill = false).verticalScroll(rememberScrollState()), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                        e.items.forEach { ChangeItem(it) }
                    }
                    Row(Modifier.fillMaxWidth().padding(top = 6.dp), verticalAlignment = Alignment.CenterVertically) {
                        Btn("Alle Neuigkeiten", BtnKind.QUIET) {
                            close()
                            startActivity(Intent(this@LibraryActivity, SettingsActivity::class.java).putExtra(SettingsActivity.EXTRA_SECTION, "news"))
                        }
                        Spacer(Modifier.weight(1f))
                        Btn("Los geht's", BtnKind.PRIMARY) { close() }
                    }
                }
            }
        }
    }


    companion object {
        val COVERS = listOf("page" to "Erste Seite", "initial" to "Initiale", "dots" to "Punkte", "stripes" to "Streifen",
            "black" to "Schwarz", "red" to "Rot", "blue" to "Blau", "green" to "Grün", "orange" to "Orange", "purple" to "Lila")
        val COVER_COLORS = mapOf("black" to 0xFF161618, "red" to 0xFFD71921, "blue" to 0xFF1E5BD8, "green" to 0xFF138A47, "orange" to 0xFFE08A00, "purple" to 0xFF7B3FD0)
    }

    @Composable
    private fun EmptyState(onCreate: () -> Unit) {
        Column(Modifier.widthIn(max = 620.dp).group().padding(28.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            Heading("Noch leer", 24.sp)
            Text("Ein Notizbuch pro Fach, das ganze Semester darin. Oder eine PDF importieren, zum Beispiel Vorlesungsfolien.", color = Ui.Muted, fontSize = 14.sp)
            Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                Btn("+ Neues Notizbuch", BtnKind.PRIMARY, onClick = onCreate)
                Btn("PDF importieren") { pickFile.launch(arrayOf("application/pdf", "application/octet-stream", "*/*")) }
            }
        }
    }

    @OptIn(androidx.compose.foundation.layout.ExperimentalLayoutApi::class)
    @Composable
    private fun CreateDialog(onDismiss: () -> Unit, folder: String = "", onCreate: (String, String, FfiSheetSpec) -> Unit) {
        var title by remember { mutableStateOf("") }
        var folder by remember { mutableStateOf(folder) }
        var bg by remember { mutableStateOf(Settings.defaultPaper.takeIf { it != FfiBackground.TEXT } ?: FfiBackground.GRID) }
        var spacing by remember { mutableStateOf(Settings.spacingFor(bg)) }
        var size by remember { mutableStateOf(Settings.newSize) }
        var asDefault by remember { mutableStateOf(false) }
        Dialog(onDismissRequest = onDismiss, properties = androidx.compose.ui.window.DialogProperties(usePlatformDefaultWidth = false)) {
            N0teType {
                Column(Modifier.width(760.dp).panel().padding(26.dp), verticalArrangement = Arrangement.spacedBy(14.dp)) {
                    Heading("Neues Notizbuch", 22.sp)
                    Row(horizontalArrangement = Arrangement.spacedBy(22.dp)) {
                        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                            Field(title, "z. B. Analysis I", label = "Titel") { title = it }
                            FolderChooser(folder) { folder = it }
                            Caption("Papier")
                            androidx.compose.foundation.layout.FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                                listOf(FfiBackground.BLANK to "Leer", FfiBackground.LINES to "Linien", FfiBackground.GRID to "Kästchen", FfiBackground.DOTS to "Punkte", FfiBackground.CORNELL to "Cornell").forEach { (b, label) ->
                                    Choice(label, bg == b) { if (b != bg) { bg = b; spacing = Settings.spacingFor(b) } }
                                }
                            }
                            if (bg != FfiBackground.BLANK) {
                                val what = when (bg) { FfiBackground.GRID -> "Kästchengröße"; FfiBackground.DOTS -> "Punktabstand"; else -> "Linienabstand" }
                                Row(verticalAlignment = Alignment.CenterVertically) {
                                    Caption(what, Modifier.weight(1f))
                                    // 1 pt = 0.3528 mm: what you'd measure on paper.
                                    Text("%.1f mm".format(spacing * 0.3528f), color = Ui.Chalk, fontSize = 13.sp, fontFamily = Voice.metaFont)
                                }
                                N0Slider(spacing, { spacing = it }, 8f..40f)
                                Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                                    listOf(4f to "4 mm", 5f to "5 mm", 7f to "7 mm", 8.5f to "8,5 mm").forEach { (mm, label) ->
                                        val pt = mm / 0.3528f
                                        Choice(label, kotlin.math.abs(spacing - pt) < 0.6f, keepCase = true) { spacing = pt }
                                    }
                                }
                            }
                            Caption("Format")
                            FormatPicker(size.first, size.second) { w, h -> size = w to h }
                        }
                        // The page as it will look (the pattern to scale).
                        Column(Modifier.width(220.dp), horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(8.dp)) {
                            Caption("Vorschau")
                            PaperPreview(bg, spacing, size.first, size.second, Modifier.width(if (size.first > size.second) 220.dp else 190.dp))
                            Text("Neue Seiten übernehmen Papier und Format. Jede Seite lässt sich später drehen.", color = Ui.Muted, fontSize = 12.sp, lineHeight = 15.sp)
                        }
                    }
                    Row(Modifier.fillMaxWidth().clickable { asDefault = !asDefault }.padding(vertical = 4.dp), verticalAlignment = Alignment.CenterVertically) {
                        Text("Als Standard für neue Notizbücher merken", color = Ui.Chalk, fontSize = 14.sp, modifier = Modifier.weight(1f))
                        Switch(asDefault)
                    }
                    Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.End) {
                        Btn("Abbrechen", BtnKind.QUIET, onClick = onDismiss)
                        Spacer(Modifier.width(8.dp))
                        Btn("Anlegen", BtnKind.PRIMARY) {
                            if (asDefault) {
                                Settings.set("paper", bg.name)
                                when (bg) { FfiBackground.GRID -> Settings.set("grid_size", spacing); FfiBackground.DOTS -> Settings.set("dot_spacing", spacing); FfiBackground.BLANK -> {}; else -> Settings.set("line_spacing", spacing) }
                                Settings.setNewSize(size.first, size.second)
                            }
                            onCreate(title.trim(), folder.trim(), FfiSheetSpec(size.first, size.second, bg, spacing, null, 0u))
                        }
                    }
                }
            }
        }
    }

    /**
     * Where a new notebook goes, as pictures: no folder, every folder (nested ones say where they
     * are), and a new one made right here (inside the one chosen).
     */
    @Composable
    private fun FolderChooser(folder: String, onPick: (String) -> Unit) {
        var adding by remember { mutableStateOf(false) }
        var name by remember { mutableStateOf("") }
        val folders = LibraryOrg.tree(entries.orEmpty().map { it.folder }.filter { it.isNotEmpty() }, extraFolders).map { it.path }
        val row = androidx.compose.foundation.lazy.rememberLazyListState()
        // The one chosen is in view (also one made just now, at the end).
        androidx.compose.runtime.LaunchedEffect(folder) { val i = folders.indexOf(folder); if (i >= 0) row.animateScrollToItem(i + 1) }
        Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Caption("Ordner", Modifier.weight(1f))
                Text(if (folder.isEmpty()) "Ohne Ordner" else folder.split('/').joinToString(" › "), color = Ui.Muted, fontSize = 12.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
            }
            androidx.compose.foundation.lazy.LazyRow(state = row, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                item(key = "") { FolderTile("Ohne Ordner", null, folder.isEmpty(), kind = 1) { onPick("") } }
                items(folders.size, key = { folders[it] }) { i ->
                    val p = folders[i]
                    FolderTile(LibraryOrg.name(p), LibraryOrg.parent(p).takeIf { it.isNotEmpty() }?.let { "in ${LibraryOrg.name(it)}" }, folder == p) { onPick(p) }
                }
                item(key = "+") { FolderTile("Neuer Ordner", null, adding, kind = 2) { adding = !adding } }
            }
            if (adding) Row(verticalAlignment = Alignment.CenterVertically) {
                Box(Modifier.weight(1f)) { Field(name, if (folder.isEmpty()) "Name des Ordners" else "Neuer Ordner in „${LibraryOrg.name(folder)}“") { name = it } }
                Spacer(Modifier.width(8.dp))
                Btn("Anlegen", enabled = name.isNotBlank()) {
                    val path = LibraryOrg.clean(if (folder.isEmpty()) name else "$folder/$name")
                    if (path.isNotEmpty() && path !in extraFolders) { extraFolders += path; keepFolders() }
                    onPick(path); name = ""; adding = false
                }
            }
        }
    }

    /** A folder as a picture to tap; [kind] 1: no folder (a loose page), 2: make a new one. */
    @Composable
    private fun FolderTile(name: String, sub: String?, on: Boolean, kind: Int = 0, onClick: () -> Unit) {
        val shape = RoundedCornerShape(14.dp)
        Column(Modifier.width(104.dp).clip(shape).background(if (on) Ui.Elevated else Color.Transparent)
            // Red is the folder chosen; "new folder" open is only lit.
            .border(if (on) 2.dp else 1.dp, if (on) (if (kind == 2) Ui.Chalk.copy(alpha = 0.7f) else Ui.Red) else Ui.Border, shape).clickable(onClick = onClick).padding(horizontal = 8.dp, vertical = 10.dp),
            horizontalAlignment = Alignment.CenterHorizontally) {
            val ink = if (on) (if (kind == 2) Ui.Chalk else Ui.Red) else Ui.Muted
            androidx.compose.foundation.Canvas(Modifier.size(58.dp, 44.dp)) {
                val w = size.width; val h = size.height
                val r = androidx.compose.ui.geometry.CornerRadius(5.dp.toPx())
                when (kind) {
                    // A loose page.
                    1 -> {
                        val pw = h * 0.78f
                        drawRoundRect(ink, androidx.compose.ui.geometry.Offset((w - pw) / 2, 0f), androidx.compose.ui.geometry.Size(pw, h), r, style = androidx.compose.ui.graphics.drawscope.Stroke(2.dp.toPx()))
                        for (k in 1..3) drawLine(ink.copy(alpha = 0.6f), androidx.compose.ui.geometry.Offset((w - pw) / 2 + 7.dp.toPx(), h * k / 4.5f), androidx.compose.ui.geometry.Offset((w + pw) / 2 - 7.dp.toPx(), h * k / 4.5f), 1.5.dp.toPx())
                    }
                    // A dashed folder with a plus: make one.
                    2 -> {
                        drawRoundRect(ink, androidx.compose.ui.geometry.Offset(0f, h * 0.12f), androidx.compose.ui.geometry.Size(w, h * 0.88f), r,
                            style = androidx.compose.ui.graphics.drawscope.Stroke(2.dp.toPx(), pathEffect = androidx.compose.ui.graphics.PathEffect.dashPathEffect(floatArrayOf(6.dp.toPx(), 4.dp.toPx()))))
                        val c = androidx.compose.ui.geometry.Offset(w / 2, h * 0.56f)
                        drawLine(ink, c.copy(x = c.x - 9.dp.toPx()), c.copy(x = c.x + 9.dp.toPx()), 2.5.dp.toPx())
                        drawLine(ink, c.copy(y = c.y - 9.dp.toPx()), c.copy(y = c.y + 9.dp.toPx()), 2.5.dp.toPx())
                    }
                    // A folder: the back with its tab, the front over it.
                    else -> {
                        val back = androidx.compose.ui.graphics.Path().apply {
                            moveTo(0f, r.x); quadraticTo(0f, 0f, r.x, 0f)
                            lineTo(w * 0.38f, 0f); lineTo(w * 0.48f, h * 0.14f)
                            lineTo(w - r.x, h * 0.14f); quadraticTo(w, h * 0.14f, w, h * 0.14f + r.x)
                            lineTo(w, h); lineTo(0f, h); close()
                        }
                        drawPath(back, ink.copy(alpha = 0.5f))
                        drawRoundRect(ink, androidx.compose.ui.geometry.Offset(0f, h * 0.3f), androidx.compose.ui.geometry.Size(w, h * 0.7f), r)
                    }
                }
            }
            Text(name, color = Ui.Chalk, fontSize = 13.sp, fontWeight = if (on) FontWeight.SemiBold else FontWeight.Normal, maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.padding(top = 7.dp))
            Text(sub ?: " ", color = Ui.Muted, fontSize = 10.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
        }
    }

    /** A page of [bg] paper at [spacing], drawn to scale for a [w]×[h] pt page. */
    @Composable
    private fun PaperPreview(bg: FfiBackground, spacing: Float, w: Float, h: Float, modifier: Modifier) {
        androidx.compose.foundation.Canvas(modifier.aspectRatio(w / h).clip(RoundedCornerShape(3.dp)).background(Color.White).border(1.dp, Ui.Border, RoundedCornerShape(3.dp))) {
            val k = size.width / w
            val st = spacing * k
            val line = Color(0xFFCBD2DD)
            val o = androidx.compose.ui.geometry.Offset.Zero
            when (bg) {
                FfiBackground.LINES -> { var y = st * 2; while (y < size.height) { drawLine(line, o.copy(y = y), androidx.compose.ui.geometry.Offset(size.width, y), 1f); y += st } }
                FfiBackground.GRID -> {
                    var x = st; while (x < size.width) { drawLine(line, o.copy(x = x), androidx.compose.ui.geometry.Offset(x, size.height), 1f); x += st }
                    var y = st; while (y < size.height) { drawLine(line, o.copy(y = y), androidx.compose.ui.geometry.Offset(size.width, y), 1f); y += st }
                }
                FfiBackground.DOTS -> { var x = st; while (x < size.width) { var y = st; while (y < size.height) { drawCircle(Color(0xFFAEB4BF), 1.3f, androidx.compose.ui.geometry.Offset(x, y)); y += st }; x += st } }
                FfiBackground.CORNELL -> {
                    var y = st * 3; while (y < size.height * 0.78f) { drawLine(line, androidx.compose.ui.geometry.Offset(size.width * 0.28f, y), androidx.compose.ui.geometry.Offset(size.width, y), 1f); y += st }
                    val red = Color(0x99D71921)
                    drawLine(red, androidx.compose.ui.geometry.Offset(size.width * 0.28f, st * 2), androidx.compose.ui.geometry.Offset(size.width * 0.28f, size.height * 0.78f), 1.5f)
                    drawLine(red, androidx.compose.ui.geometry.Offset(0f, size.height * 0.78f), androidx.compose.ui.geometry.Offset(size.width, size.height * 0.78f), 1.5f)
                }
                else -> {}
            }
        }
    }
}

/** Hands a file to n0bridge's share target (it sends it to the laptop); any app if n0bridge is missing. */
fun shareToN0bridge(ctx: android.content.Context, f: java.io.File, mime: String) {
    val uri = androidx.core.content.FileProvider.getUriForFile(ctx, "${ctx.packageName}.files", f)
    val send = Intent(Intent.ACTION_SEND).setType(mime).putExtra(Intent.EXTRA_STREAM, uri).addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
    runCatching { ctx.startActivity(Intent(send).setPackage("dev.n0bridge.app")) }
        .onFailure { ctx.startActivity(Intent.createChooser(send, f.name)) }
}
