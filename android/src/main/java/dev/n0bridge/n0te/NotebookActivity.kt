package dev.n0bridge.n0te

import android.content.Intent
import android.os.Bundle
import android.view.View
import android.view.ViewGroup
import android.widget.FrameLayout
import android.widget.Toast
import androidx.activity.ComponentActivity
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.gestures.detectDragGestures
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.gestures.detectHorizontalDragGestures
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.input.pointer.changedToUpIgnoreConsumed
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.gestures.awaitLongPressOrCancellation
import androidx.compose.foundation.gestures.drag
import androidx.compose.foundation.gestures.scrollBy
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.layout.positionInParent
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.verticalScroll
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.nativeCanvas
import androidx.compose.ui.geometry.Offset
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.layout.heightIn
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.platform.ComposeView
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.layout.boundsInRoot
import androidx.compose.ui.layout.positionInRoot
import androidx.compose.foundation.layout.widthIn
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.window.Dialog
import androidx.core.content.FileProvider
import androidx.core.view.WindowCompat
import androidx.core.view.WindowInsetsCompat
import androidx.core.view.WindowInsetsControllerCompat
import uniffi.n0te_ffi.FfiBackground
import uniffi.n0te_ffi.FfiOutlineRow
import java.io.File

/** The selection button's modes, in the order a swipe goes through them. */
private val SELECT_MODES = listOf("free", "rect", "snip", "space")

/** One notebook: the pages ([NotebookView]) with the low-latency ink layer and the tools. */
@OptIn(androidx.compose.foundation.ExperimentalFoundationApi::class, androidx.compose.foundation.layout.ExperimentalLayoutApi::class)
/** A text box (or a text page's body) being edited. */
internal class TextEdit(val sheetId: String, val boxId: String, val body: Boolean)

/** Where a dragged outline row would land (data classes: the same place is no new state, so a move
 * by a pixel doesn't recompose every row of the outline). */
internal sealed class Drop {
    /** Between rows: into [parent] (null = main column) at [index], indicator at [lineY]/[depth]. */
    data class Gap(val parent: String?, val index: Int, val depth: Int, val lineY: Float) : Drop()
    /** Onto a group: at its end. */
    data class Into(val group: String) : Drop()
}

/** Sends the lasso selection as an image to the laptop clipboard (through n0bridge). */
/** What the LaTeX dialog is about: the selected handwriting and who read it. */
internal class LatexFound(val sheetId: String, val ids: Set<String>, val bounds: android.graphics.RectF, val engine: String)

class NotebookActivity : ComponentActivity() {

    companion object {
        const val EXTRA_ID = "notebook_id"
        /** Open at this page (sheet id): "Zuletzt bearbeitet" on the start page. */
        const val EXTRA_SHEET = "sheet_id"
        /** Opened by the laptop's LIVE button (the device id). */
        const val EXTRA_LIVE = "live_device"
        /** Shared pictures/PDF (content URIs) to put in once the notebook is open. */
        const val EXTRA_SHARE = "shared_items"
        private val PAPERS = listOf(FfiBackground.BLANK to "Leer", FfiBackground.LINES to "Linien", FfiBackground.GRID to "Kästchen", FfiBackground.DOTS to "Punkte", FfiBackground.CORNELL to "Cornell")
        private const val STATE_PANES = "panes"
        private const val STATE_CAMERAS = "pane_cameras"
        private val PRESETS = listOf(
            0xFF111111, 0xFF4A4A4F, 0xFF8A8A92, 0xFF1E5BD8, 0xFF2F7DF6, 0xFF0AA4B8, 0xFF138A47, 0xFF34B36B, 0xFF8CBF26,
            0xFFD71921, 0xFFF0483E, 0xFFE0408A, 0xFFE08A00, 0xFFF5B400, 0xFFFFD60A, 0xFF7B3FD0, 0xFFA066F0, 0xFF8B5A2B,
        ).map { it.toInt() }
    }


    // UI state mirrored from the view/model.
    // Pen, highlighter or eraser, colour and width as they were last time (the pen type, the
    // colours and widths themselves are kept in Tools already).
    private var tool by mutableStateOf(rememberedTool())

    private var colorIdx by mutableIntStateOf(Tools.savedColor)

    private var widthIdx by mutableIntStateOf(Tools.savedWidth)


    private var nbToolsSaved = ""


    /** This notebook's last pen, colour, width, pen type and palette (else the last ones anywhere). */
    private fun restoreNotebookTools(id: String) {
        val f = Settings.str("nb_tools_$id", "").split('|')
        if (f.size < 5) return
        tool = parseTool(f[0])
        f[4].toIntOrNull()?.let { Tools.choosePalette(it) }
        f[1].toIntOrNull()?.let { colorIdx = it.coerceIn(0, Tools.inks.size - 1) }
        f[2].toIntOrNull()?.let { widthIdx = it.coerceIn(0, Tools.widths.size - 1) }
        runCatching { Tools.choosePen(PenType.valueOf(f[3])) }
        nbToolsSaved = Settings.str("nb_tools_$id", "")
    }


    private fun parseTool(name: String?): NotebookView.Tool = when (name) {
        "MARKER" -> if (Settings.toolMarker) NotebookView.Tool.MARKER else NotebookView.Tool.PEN
        "ERASER" -> NotebookView.Tool.ERASER
        else -> NotebookView.Tool.PEN
    }


    private fun rememberedTool(): NotebookView.Tool = when (Tools.savedTool) {
        "MARKER" -> if (Settings.toolMarker) NotebookView.Tool.MARKER else NotebookView.Tool.PEN
        "ERASER" -> NotebookView.Tool.ERASER
        else -> NotebookView.Tool.PEN // lasso, ruler, laser, text: chosen for a moment, not to start with
    }

    private var penOnly by mutableStateOf(Settings.penOnlyDefault)

    private var tick by mutableIntStateOf(0) // bumps when presets change, so swatches redraw

    private var settingsOpen = false
    /** The default paper when the settings were opened (new pages follow it only when it changed). */
    private var paperBeforeSettings = Settings.defaultPaper

    /** Following the laptop's view (its device id). */
    private var following by mutableStateOf<String?>(null)

    /** The laptop was asked to follow this view. */
    private var laptopFollows by mutableStateOf(false)


    private val themeId = Ui.theme.id

    /** Something is on the ink clipboard (shows "Einfügen"). */
    private var canPaste by mutableStateOf(N0te.clip != null)

    private var rulerOn by mutableStateOf(false)

    /** A picture from the gallery or files: upright, JPEG on white, onto the page in view. */
    private val pickImage = registerForActivityResult(androidx.activity.result.contract.ActivityResultContracts.GetContent()) { uri ->
        val p = pickFor?.takeIf { it in panes } ?: active ?: return@registerForActivityResult
        pickFor = null
        if (uri == null) { p.pickAt = null; return@registerForActivityResult }
        p.status = "Bild wird eingefügt …"
        N0te.background({
            val bytes = contentResolver.openInputStream(uri)?.use { it.readBytes() } ?: error("nicht lesbar")
            val (jpeg, w, h) = Images.prepare(bytes) ?: error("kein Bild")
            Triple(N0te.library.putBlob(jpeg), w, h)
        }) { r ->
            p.status = null
            val at = p.pickAt.also { p.pickAt = null }
            if (p !in panes) return@background
            r.onSuccess { (blob, w, h) -> tool = NotebookView.Tool.LASSO; applyTools(); if (at != null) p.view.insertImageAt(blob, w, h, at.x, at.y) else p.view.insertImage(blob, w, h) }
                .onFailure { p.flash("Bild einfügen fehlgeschlagen: ${it.message}") }
        }
    }

    private val pickPdf = registerForActivityResult(androidx.activity.result.contract.ActivityResultContracts.OpenDocument()) { uri ->
        val p = pickFor?.takeIf { it in panes } ?: active ?: return@registerForActivityResult
        pickFor = null
        if (uri == null) return@registerForActivityResult
        val m = p.model ?: return@registerForActivityResult
        p.status = "Importiere PDF …"
        N0te.background({ contentResolver.openInputStream(uri)?.use { it.readBytes() } ?: error("nicht lesbar") }) { r ->
            r.onSuccess { bytes ->
                m.importPdfAfter(bytes, p.view.currentSheet()?.node) { res ->
                    p.status = res.fold({ "$it Seiten eingefügt" }, { "Import fehlgeschlagen: ${it.message}" })
                    N0te.main.postDelayed({ p.status = null }, 2500)
                }
            }.onFailure { p.status = "PDF nicht lesbar"; N0te.main.postDelayed({ p.status = null }, 2500) }
        }
    }

    /** Which way the last swipe went (the content slides that way). */
    private var paletteDir by mutableIntStateOf(1)

    private var eraserDir by mutableIntStateOf(1)


    private var selectDir by mutableIntStateOf(1)

    /** The selection tool's mode (Settings.selectMode), as state: the button follows the menu. */
    private var selectMode by mutableStateOf(Settings.selectMode)

    /** The eraser's mode (Settings.eraserArea), as state: the button follows the menu. */
    private var eraserArea by mutableStateOf(Settings.eraserArea)


    // --- the panes: one notebook, or two side by side ------------------------------------------

    /** The notebooks on screen, left to right (top to bottom when upright): one or two. */
    private val panes = androidx.compose.runtime.mutableStateListOf<Pane>()
    /** The pane the toolbar works on: the one touched last. */
    private var active by mutableStateOf<Pane?>(null)
    /** ★: the pane the laptop follows and shows, where the laser and summons go. */
    private var mainPane by mutableStateOf<Pane?>(null)
    /** A pane shown alone for a while (⤢); the other stays open behind it. */
    private var maxed by mutableStateOf<Pane?>(null)
    /** The first pane's share of the room. */
    private var split by androidx.compose.runtime.mutableFloatStateOf(0.5f)
    /** While the divider is dragged towards an edge: the pane that would close (or -1). */
    private var closingPane by mutableIntStateOf(-1)
    /** Where the panes are (px, window coordinates), in [panes] order. */
    private var paneRects by mutableStateOf<List<Area>>(emptyList())
    /** The shared toolbar's bounds: touching it doesn't change the active pane. */
    private var toolbarRect: androidx.compose.ui.geometry.Rect? = null
    private var toolbarBottom by androidx.compose.runtime.mutableFloatStateOf(0f)
    private lateinit var splitLayout: SplitLayout
    /** The pane that asked for a picture or a PDF (the answer goes there). */
    private var pickFor: Pane? = null
    private val act: Pane get() = active ?: panes.first()
    private val isSplit get() = panes.size > 1 && maxed == null

    private val syncListener: (uniffi.n0te_ffi.FfiSyncEvent) -> Unit = { e -> onSync(e) }
    /** Remote changes per notebook when paused (see [SyncHub.remoteChangeCount]). */
    private val remoteSeen = HashMap<String, Int>()
    private val handwritingListener: () -> Unit = {
        tick++
        if (Handwriting.ready) panes.mapNotNull { it.model }.distinct().forEach { it.indexAll() }
    }
    private val redraw: () -> Unit = { panes.forEach { it.view.invalidate() } }
    private val redrawPlots: () -> Unit = { panes.forEach { it.view.invalidate() }; tick++ }

    override fun onCreate(savedInstanceState: Bundle?) {
        val createdAt = android.os.SystemClock.uptimeMillis()
        super.onCreate(savedInstanceState)
        WindowCompat.setDecorFitsSystemWindows(window, false)
        Themes.applyWindow(this)
        WindowInsetsControllerCompat(window, window.decorView).apply {
            hide(WindowInsetsCompat.Type.navigationBars())
            systemBarsBehavior = WindowInsetsControllerCompat.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE
        }
        val id = intent.getStringExtra(EXTRA_ID) ?: return finish()
        // The remembered pen / highlighter / eraser, colour and width from the start (this
        // notebook's, else the last ones used anywhere).
        restoreNotebookTools(id)
        splitLayout = SplitLayout(this).apply {
            gap = (resources.displayMetrics.density * 6).toInt()
            setBackgroundColor(android.graphics.Color.BLACK)
            onLaidOut = { r -> if (r != paneRects) paneRects = r }
        }
        // The design's type for all of it (the sidebar, dialogs and the status line had the system font).
        val chrome = ComposeView(this).apply { setContent { N0teType { Chrome() } } }
        setContentView(FrameLayout(this).apply {
            addView(splitLayout, ViewGroup.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT))
            // The keyboard: keep the cursor above it.
            androidx.core.view.ViewCompat.setOnApplyWindowInsetsListener(this) { v, insets ->
                panes.forEach { p -> if (p.inlineReady) p.inline.post { p.keepCursorVisible() } }
                androidx.core.view.ViewCompat.onApplyWindowInsets(v, insets)
            }
            addView(chrome, ViewGroup.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT))
        })
        PdfTiles.listener = redraw
        Images.listener = redraw
        Plots.listener = redrawPlots
        // Handwriting search: the recogniser's model (loaded once over Wi-Fi); once it is
        // there, the notebooks' pages are read in the background.
        Handwriting.onChange(handwritingListener)
        if (Settings.handwritingSearch) Handwriting.check(fetch = true)
        // Rebuilt (the system ended the app while away): the same panes again.
        val saved = SplitGeometry.readState(savedInstanceState?.getString(STATE_PANES))
        if (saved != null) {
            split = saved.fraction
            splitLayout.fraction = split
            val cams = savedInstanceState?.getStringArrayList(STATE_CAMERAS).orEmpty()
            saved.ids.forEachIndexed { i, nb -> addPane(Pane(nb, null, null, null, cams.getOrNull(i)?.takeIf { it.isNotEmpty() })) }
            mainPane = panes.getOrNull(saved.main) ?: panes.first()
            active = panes.getOrNull(saved.active) ?: panes.first()
        } else {
            val first = Pane(id, intent.getStringExtra(EXTRA_SHEET), intent.getStringArrayListExtra(EXTRA_SHARE), intent.getStringExtra(EXTRA_LIVE), null)
            addPane(first)
            mainPane = first
            active = first
            // Frame times for performance work (adb only, see PerfBench).
            intent.getStringExtra(PerfBench.EXTRA)?.let { PerfBench.start(this, first.view, intent.getStringExtra(EXTRA_SHEET), it, createdAt) }
        }
        // Only for the first opening (not again when rebuilt).
        intent.removeExtra(EXTRA_SHEET)
        intent.removeExtra(EXTRA_SHARE)
        intent.removeExtra(EXTRA_LIVE)
    }

    override fun onSaveInstanceState(outState: Bundle) {
        super.onSaveInstanceState(outState)
        if (panes.isEmpty()) return
        outState.putString(STATE_PANES, SplitGeometry.saveState(panes.map { it.nbId }, panes.indexOf(mainPane).coerceAtLeast(0), panes.indexOf(active).coerceAtLeast(0), split))
        outState.putStringArrayList(STATE_CAMERAS, ArrayList(panes.map { it.view.cameraState() ?: "" }))
    }

    @Deprecated("predictive back handled by the toolbar")
    @Suppress("DEPRECATION")
    override fun onBackPressed() {
        if (panes.isEmpty()) return super.onBackPressed()
        val a = act
        a.graph?.let { g -> if (g.keys.active != null) { g.keys.active = null; return } }
        // A graph being edited: back finishes it (kept). It used to close the notebook and the edits were gone.
        if (a.graph != null) { a.finishGraph(true); return }
        if (a.view.board != null) { a.view.exitBoard(); return }
        // Followed a link in a PDF: back goes back to where it was.
        if (a.view.linkBack()) return
        // Two notebooks: back first shows one alone again, then closes the second.
        if (maxed != null) { showAlone(null); return }
        if (panes.size > 1) { closePane(panes.first { it !== mainPane }); return }
        super.onBackPressed()
    }

    override fun onResume() {
        super.onResume()
        SyncHub.listen(syncListener)
        SyncHub.refreshGuests()
        // Changed elsewhere while paused (nobody listened): read it now, not only at the next change.
        for (m in panes.mapNotNull { it.model }.distinct()) {
            val id = panes.first { it.model === m }.nbId
            if (remoteSeen[id]?.let { it != SyncHub.remoteChangeCount(id) } == true) m.refreshFromDisk(true, emptyList()) { ids -> panes.filter { it.model === m }.forEach { it.reloaded(ids) } }
        }
        publishOpen()
        SyncHub.openHere = { nb, device -> openFromOutside(nb, device) }
        if (Ui.theme.id != themeId) { recreate(); return }
        canPaste = N0te.clip != null
        if (settingsOpen) {
            // Back from the settings: apply what changed.
            settingsOpen = false
            if (!Settings.toolMarker && tool == NotebookView.Tool.MARKER) tool = NotebookView.Tool.PEN
            if (!Settings.toolLasso && tool == NotebookView.Tool.LASSO) tool = NotebookView.Tool.PEN
            if (!Settings.toolRuler) rulerOn = false
            if (!Settings.toolLaser && tool == NotebookView.Tool.LASER) tool = NotebookView.Tool.PEN
            Pencil.refresh()
            // Only when the default changed there: else every visit reset a grid notebook's new pages to lines.
            if (Settings.defaultPaper != paperBeforeSettings) panes.forEach { it.newPaper = Settings.defaultPaper }
            panes.mapNotNull { it.model }.distinct().forEach { it.relayout() }
            applyTools()
            tick++
            panes.forEach { it.view.invalidate() }
        }
    }

    override fun onPause() {
        if (isFinishing && laptopFollows) {
            SyncHub.laptop?.let { SyncHub.setFollow(it.id, false) }
            laptopFollows = false
        }
        SyncHub.unlisten(syncListener)
        remoteSeen.clear()
        for (p in panes) remoteSeen[p.nbId] = SyncHub.remoteChangeCount(p.nbId)
        if (SyncHub.openNotebook == mainPane?.nbId) SyncHub.openNotebook = null
        if (SyncHub.openNotebooks == panes.map { it.nbId }.toSet()) SyncHub.openNotebooks = emptySet()
        SyncHub.openHere = null
        // The main pane last: when one notebook is open twice, its view is where it opens next time.
        panes.sortedBy { it === mainPane }.forEach { it.pause() }
        // Closed: the laptop and guests no longer see a frame of where this tablet looked.
        if (isFinishing) for (nb in panes.map { it.nbId }.distinct()) if (SyncHub.sharing(nb)) SyncHub.presence(nb, null, FloatArray(4), 0f, null)
        super.onPause()
    }

    override fun onDestroy() {
        panes.forEach { it.inkOverlay.release() }
        Handwriting.offChange(handwritingListener)
        if (PdfTiles.listener === redraw) PdfTiles.listener = null
        if (Images.listener === redraw) Images.listener = null
        if (Plots.listener === redrawPlots) Plots.listener = null
        super.onDestroy()
    }

    /** A touch in a pane makes it the active one (not on the shared toolbar). */
    override fun dispatchTouchEvent(ev: android.view.MotionEvent): Boolean {
        if (ev.actionMasked == android.view.MotionEvent.ACTION_DOWN && panes.size > 1) {
            val onBar = toolbarRect?.contains(androidx.compose.ui.geometry.Offset(ev.x, ev.y)) == true
            if (!onBar) paneRects.indexOfFirst { it.contains(ev.x.toInt(), ev.y.toInt()) }.takeIf { it >= 0 }?.let { panes.getOrNull(it)?.let { p -> activate(p) } }
        }
        return super.dispatchTouchEvent(ev)
    }

    /** The pen, colour and width (and pen-only, ruler) for every pane; remembered for the active notebook. */
    private fun applyTools() {
        Tools.saveSelection(tool.name, colorIdx, widthIdx)
        // And per notebook: pen, colour, width, pen type and palette as last used in this one.
        active?.nbId?.let { id ->
            val v = "${tool.name}|$colorIdx|$widthIdx|${Tools.pen.name}|${Tools.palette}"
            if (v != nbToolsSaved) { nbToolsSaved = v; Settings.set("nb_tools_$id", v) }
        }
        panes.forEach { p ->
            p.view.tool = tool
            p.view.colorIdx = colorIdx
            p.view.widthIdx = widthIdx
            p.view.penOnly = penOnly
            p.view.rulerOn = rulerOn
        }
    }

    private fun addPane(p: Pane, index: Int = panes.size) {
        val container = p.build()
        panes.add(index, p)
        splitLayout.addView(container, index)
        applyTools()
        p.start()
        publishOpen()
    }

    /** Closes [p] (the other stays, alone). */
    private fun closePane(p: Pane) {
        if (panes.size < 2 || p !in panes) return
        val wasMain = mainPane === p
        p.close()
        panes.remove(p)
        splitLayout.removeView(p.container)
        showAlone(null)
        val rest = panes.first()
        if (active === p) activate(rest)
        if (wasMain) setMain(rest)
        publishOpen()
    }

    /** Puts notebook [nb] (at page [sheet]) where [old] is. */
    private fun replacePane(old: Pane, nb: String, sheet: String? = null, live: String? = null) {
        val i = panes.indexOf(old)
        if (i < 0) return
        val wasMain = mainPane === old
        val wasActive = active === old
        val wasMaxed = maxed === old
        old.close()
        panes.removeAt(i)
        splitLayout.removeView(old.container)
        val p = Pane(nb, sheet, null, live, null)
        addPane(p, i)
        if (wasActive) active = p
        if (wasMain) { mainPane = null; setMain(p) }
        // Shown alone: the new one is (the layout and the other pane's visibility follow too).
        if (wasMaxed) showAlone(p) else if (maxed != null) showAlone(maxed)
    }

    /** Opens [nb] (at [sheet]) beside [from]: in the other pane when there are two. */
    private fun openBeside(from: Pane, nb: String, sheet: String?) {
        from.menu = null
        val other = panes.firstOrNull { it !== from }
        if (other != null) {
            showAlone(null)
            if (other.nbId == nb) { if (sheet != null) other.view.showPage(sheet) }
            else replacePane(other, nb, sheet)
            return
        }
        addPane(Pane(nb, sheet, null, null, null), panes.indexOf(from) + 1)
        if (Settings.str("split_hint_seen", "") != "1") {
            Settings.set("split_hint_seen", "1")
            from.flash("Zwei nebeneinander: der Strich in der Mitte ändert die Breite, ★ ist für Laptop und Gäste")
        }
    }

    /** Swaps left and right (top and bottom). */
    private fun swapPanes() {
        if (panes.size < 2) return
        val a = panes[0]
        panes.removeAt(0)
        panes.add(a)
        splitLayout.removeView(a.container)
        splitLayout.addView(a.container)
        split = 1f - split
        splitLayout.fraction = split
        // The one shown alone moved: the layout's index follows (else the hidden one got the room).
        splitLayout.maxed = maxed?.let { panes.indexOf(it) } ?: -1
    }

    /** [p] alone on screen for a while (null: both again). */
    private fun showAlone(p: Pane?) {
        maxed = p
        panes.forEach { it.container.visibility = if (p == null || it === p) View.VISIBLE else View.GONE }
        splitLayout.maxed = p?.let { panes.indexOf(it) } ?: -1
        if (p != null) activate(p)
    }

    private fun activate(p: Pane) {
        val prev = active
        if (prev === p) return
        active = p
        nbToolsSaved = Settings.str("nb_tools_${p.nbId}", "")
        prev?.let {
            it.menu = null
            it.held = null
            // The keyboard goes with the pane: typing there ends.
            it.finishText()
        }
    }

    /** ★ to [p]: the laptop (following or followed) and the laser go with it. */
    private fun setMain(p: Pane) {
        val old = mainPane
        if (old === p) return
        mainPane = p
        publishOpen()
        old?.view?.followDevice = null
        val laptop = SyncHub.laptop?.takeIf { it.online }
        if (laptop != null && (laptopFollows || following != null) && old?.nbId != p.nbId) SyncHub.requestOpen(laptop.id, p.nbId)
        p.view.followDevice = following
        p.view.pushPresence()
    }

    /** The pane that tells others where I look in [nb]: the main one, else the first showing it. */
    private fun presenceOwner(nb: String): Pane? = mainPane?.takeIf { it.nbId == nb } ?: panes.firstOrNull { it.nbId == nb }

    private fun publishOpen() {
        SyncHub.openNotebook = mainPane?.nbId ?: panes.firstOrNull()?.nbId
        SyncHub.openNotebooks = panes.map { it.nbId }.toSet()
    }

    /**
     * The laptop's LIVE button or a summon for a notebook not on screen: with two panes it opens
     * in the main one (true); alone, a notebook opens on top as before (false).
     */
    private fun openFromOutside(nb: String, device: String?): Boolean {
        if (panes.size < 2) return false
        val m = mainPane ?: return false
        replacePane(m, nb, live = device)
        // It must be seen: not loaded into a pane hidden behind the other.
        showAlone(null)
        return true
    }

    /** Sync events to the panes showing that notebook; following the laptop is the main pane's. */
    private fun onSync(e: uniffi.n0te_ffi.FfiSyncEvent) {
        when (e) {
            is uniffi.n0te_ffi.FfiSyncEvent.FollowRequest, is uniffi.n0te_ffi.FfiSyncEvent.DeviceOffline -> mainPane?.onSync(e)
            else -> panes.toList().forEach { it.onSync(e) }
        }
    }

    /** The controls over the pages: the toolbar for the active pane, each pane's own, the divider. */
    @Composable
    private fun Chrome() {
        val density = androidx.compose.ui.platform.LocalDensity.current
        val rects = paneRects
        Box(Modifier.fillMaxSize()) {
            panes.forEachIndexed { i, p ->
                val r = rects.getOrNull(i)
                androidx.compose.runtime.key(p) {
                    if (r != null && !r.isEmpty) {
                        Box(Modifier.offset { androidx.compose.ui.unit.IntOffset(r.left, r.top) }.size(with(density) { r.width().toDp() }, with(density) { r.height().toDp() })) {
                            // Two open (also while one is shown alone): each has its head.
                            p.Overlay(panes.size > 1, kotlin.math.max(0f, toolbarBottom - r.top))
                        }
                    }
                }
            }
            if (isSplit && rects.size == 2) {
                // The active pane: a thin frame in the accent colour.
                val ai = panes.indexOf(active)
                rects.getOrNull(ai)?.let { r ->
                    Box(Modifier.offset { androidx.compose.ui.unit.IntOffset(r.left, r.top) }.size(with(density) { r.width().toDp() }, with(density) { r.height().toDp() })
                        .border(2.dp, Ui.Red.copy(alpha = 0.75f)))
                }
                SplitDivider(rects[0], rects[1])
            }
            // A page dragged out of the outline: where it would open beside (bright when it would).
            panes.firstOrNull { p -> p.dragNode != null && p.dragMoved && p.rows.any { it is FfiOutlineRow.Sheet && it.node == p.dragNode } }?.let { src ->
                val si = panes.indexOf(src)
                val zone = if (panes.size > 1) rects.getOrNull(1 - si)
                    else rects.getOrNull(si)?.let { r -> Area(r.left + (r.width() * 0.62f).toInt(), r.top, r.right, r.bottom) }
                val hot = src.besideSheet != null
                zone?.takeIf { !it.isEmpty }?.let { r ->
                    Box(Modifier.offset { androidx.compose.ui.unit.IntOffset(r.left, r.top) }.size(with(density) { r.width().toDp() }, with(density) { r.height().toDp() })
                        .padding(14.dp).clip(RoundedCornerShape(22.dp)).background(Ui.Red.copy(alpha = if (hot) 0.22f else 0.07f))
                        .border(if (hot) 3.dp else 1.dp, Ui.Red.copy(alpha = if (hot) 1f else 0.5f), RoundedCornerShape(22.dp)), contentAlignment = Alignment.Center) {
                        Text(Voice.cased(if (panes.size > 1) "Hier öffnen" else "Daneben öffnen"), color = if (hot) Ui.OnAccent else Ui.Chalk, fontSize = 15.sp, fontFamily = Voice.labelFont, fontWeight = FontWeight.Bold,
                            modifier = Modifier.clip(RoundedCornerShape(14.dp)).background(if (hot) Ui.Red else Ui.Bar).padding(horizontal = 16.dp, vertical = 9.dp))
                    }
                }
            }
            panes.forEach { androidx.compose.runtime.key(it) { it.Ghost() } }
            val a = active
            if (a == null || a.graph != null) toolbarRect = null
            if (a != null && a.graph == null) androidx.compose.runtime.key(a) {
                a.Toolbar(Modifier.align(Alignment.TopCenter).statusBarsPadding().padding(top = 8.dp).onGloballyPositioned { c ->
                    val b = c.boundsInRoot()
                    toolbarRect = b
                    toolbarBottom = b.bottom
                })
            }
        }
    }

    /** The strip between the panes: drag it to share the room (it snaps to thirds and the half); to an edge closes that pane. */
    @Composable
    private fun SplitDivider(r0: Area, r1: Area) {
        val density = androidx.compose.ui.platform.LocalDensity.current
        val across = r0.top == r1.top // side by side
        val total = if (across) r1.right.toFloat() else r1.bottom.toFloat()
        // The drag below keeps running across resizes of the window: it reads the size now.
        val totalNow by androidx.compose.runtime.rememberUpdatedState(total)
        val mid = if (across) (r0.right + r1.left) / 2f else (r0.bottom + r1.top) / 2f
        val touch = with(density) { 32.dp.toPx() }
        val closing = closingPane
        // The pane that would close: dimmed, saying so.
        if (closing >= 0) (if (closing == 0) r0 else r1).let { r ->
            Box(Modifier.offset { androidx.compose.ui.unit.IntOffset(r.left, r.top) }.size(with(density) { r.width().toDp() }, with(density) { r.height().toDp() }).background(Color.Black.copy(alpha = 0.55f)),
                contentAlignment = Alignment.Center) {
                Text(Voice.cased("Loslassen schließt"), color = Ui.OnAccent, fontSize = 14.sp, fontFamily = Voice.labelFont, fontWeight = FontWeight.Bold,
                    modifier = Modifier.clip(RoundedCornerShape(14.dp)).background(Ui.Red).padding(horizontal = 14.dp, vertical = 8.dp))
            }
        }
        val pos = androidx.compose.runtime.rememberUpdatedState(mid)
        val drag = Modifier.pointerInput(across) {
            var at = 0f
            detectDragGestures(
                onDragStart = { at = pos.value },
                onDragEnd = {
                    closingPane = -1
                    when (val rel = SplitGeometry.release(split)) {
                        is SplitGeometry.Release.Close -> panes.getOrNull(rel.pane)?.let { closePane(it) }
                        is SplitGeometry.Release.Snap -> { split = rel.fraction; splitLayout.fraction = split }
                    }
                },
                onDragCancel = { closingPane = -1; split = SplitGeometry.release(split).let { (it as? SplitGeometry.Release.Snap)?.fraction ?: 0.5f }; splitLayout.fraction = split },
            ) { change, d ->
                change.consume()
                at += if (across) d.x else d.y
                split = (at / totalNow).coerceIn(0.05f, 0.95f)
                closingPane = SplitGeometry.closing(split)
                splitLayout.fraction = split.coerceIn(0.1f, 0.9f)
            }
        }
        if (across) {
            Box(Modifier.offset { androidx.compose.ui.unit.IntOffset((mid - touch / 2).toInt(), 0) }.size(with(density) { touch.toDp() }, with(density) { r0.height().toDp() }).then(drag), contentAlignment = Alignment.Center) {
                Box(Modifier.size(6.dp, 56.dp).clip(RoundedCornerShape(3.dp)).background(Ui.Muted))
            }
        } else {
            Box(Modifier.offset { androidx.compose.ui.unit.IntOffset(0, (mid - touch / 2).toInt()) }.size(with(density) { r0.width().toDp() }, with(density) { touch.toDp() }).then(drag), contentAlignment = Alignment.Center) {
                Box(Modifier.size(56.dp, 6.dp).clip(RoundedCornerShape(3.dp)).background(Ui.Muted))
            }
        }
    }

    /** Notebooks to open beside: the ones opened lately first (id, title). */
    /** A notebook to open beside: its cover, where it is, how big, when it was last opened. */
    private class Book(val id: String, val title: String, val cover: String, val folder: String, val pages: Int, val opened: Long)

    /** All notebooks, the ones opened lately first. */
    private fun recentNotebooks(then: (List<Book>) -> Unit) {
        N0te.background({
            val opened = LibraryOrg.opened(Settings.str("lib_opened", ""))
            N0te.library.notebookIds().mapNotNull { id ->
                runCatching { N0te.library.loadNotebook(id).use { nb -> Book(id, nb.title(), nb.cover(), nb.folder(), nb.sheetCount().toInt(), opened[id] ?: 0L) } }.getOrNull()
            }.sortedByDescending { it.opened }
        }) { r -> then(r.getOrDefault(emptyList())) }
    }

    /** One notebook on screen (alone, or one of two side by side): its page view, ink layer, outline, menus. */
    @OptIn(androidx.compose.foundation.ExperimentalFoundationApi::class, androidx.compose.foundation.layout.ExperimentalLayoutApi::class)
    private inner class Pane(val nbId: String, var startSheet: String?, var shareItems: ArrayList<String>?, val liveDevice: String?, var startCamera: String?) : NotebookView.Listener {

        val view = NotebookView(this@NotebookActivity)
        val inkOverlay = InkOverlay(this@NotebookActivity)
        var model: NotebookModel? = null
        /** The pane's views: pages, live ink, the text editor. */
        lateinit var container: FrameLayout
        /** Closed: answers still arriving from the background are dropped. */
        var closed = false
        val inlineReady get() = ::inline.isInitialized

        /** Builds the pane's views (once, before it is added). */
        fun build(): FrameLayout {
            view.listener = this
            view.setOnDragListener(dragListener)
            view.inkLayer = inkOverlay
            view.tool = tool; view.colorIdx = colorIdx; view.widthIdx = widthIdx
            inline = InlineTextEditor(this@NotebookActivity, view).apply {
                visibility = View.GONE
                onText = { t -> inlineOn?.let { (sid, bid) -> model?.setText(sid, bid, t) }; post { keepCursorVisible() } }
                onBlock = { textBlock = it }
                onInline = { textInline = it }
            }
            view.live = object : NotebookView.LiveSink {
                override fun samples(sheetId: String, stroke: Long, seq: Int, brush: uniffi.n0te_ffi.FfiBrush, samples: ByteArray, done: Boolean) =
                    SyncHub.liveStroke(nbId, sheetId, stroke, seq, brush, samples, done)
                // One frame per notebook: the same notebook twice tells where the main pane looks.
                override fun presence(sheetId: String?, rect: FloatArray, zoom: Float, laser: FloatArray?) {
                    if (presenceOwner(nbId) === this@Pane) SyncHub.presence(nbId, sheetId, rect, zoom, laser)
                }
                override fun erase(sheetId: String, ids: List<String>, dabs: FloatArray) = SyncHub.liveErase(nbId, sheetId, ids, dabs)
                override fun laser(sheetId: String, samples: FloatArray, down: Boolean) = SyncHub.laser(nbId, sheetId, samples, down)
            }
            container = FrameLayout(this@NotebookActivity).apply {
                addView(view, ViewGroup.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT))
                addView(inkOverlay, ViewGroup.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT))
                addView(inline, FrameLayout.LayoutParams(ViewGroup.LayoutParams.WRAP_CONTENT, ViewGroup.LayoutParams.WRAP_CONTENT))
            }
            return container
        }

        /** The notebook's model of another pane showing it (one model per notebook: edits show in both at once). */
        private fun sharedModel(): NotebookModel? = panes.firstOrNull { it !== this && it.nbId == nbId && it.model != null }?.model

        /** Opens the notebook (or joins the other pane's model when it shows the same one). */
        fun start() {
            if (liveDevice != null) flash("Live mit dem Laptop")
            sharedModel()?.let { attach(it); return }
            N0te.background({ N0te.library.loadNotebook(nbId) }) { r ->
                if (closed) return@background
                sharedModel()?.let { attach(it); return@background }
                val nb = r.getOrNull() ?: run {
                    Toast.makeText(this@NotebookActivity, "Notizbuch nicht lesbar", Toast.LENGTH_LONG).show()
                    if (panes.size > 1) closePane(this) else finish()
                    return@background
                }
                lateinit var m: NotebookModel
                m = NotebookModel(nb) { panes.filter { it.model === m }.forEach { it.onModelChanged() } }
                attach(m)
                m.refreshOutline { m.indexAll() }
            }
        }

        private fun attach(m: NotebookModel) {
            model = m
            view.model = m
            if (m.placed.isNotEmpty()) onModelChanged()
        }

        /** The first pane holding this model: it reloads and speaks for both. */
        private val leads get() = panes.firstOrNull { it.model === model && model != null } === this

        /** Leaving (the app, or this pane): text done, picture and position kept, saved. */
        fun pause() {
            // Leaving for good or rebuilt: a graph being edited is stored (its edits live only here).
            if (graph != null && (isFinishing || isChangingConfigurations)) finishGraph(true)
            finishText()
            saveThumbnail()
            // Next time the notebook opens where it was left.
            view.cameraState()?.let { Settings.set("last_view_$nbId", it) }
            model?.flush()
        }

        fun close() {
            if (closed) return
            if (graph != null) finishGraph(true)
            pause()
            closed = true
            menu = null
            // Nobody looks at it here any more: the others' frame of mine goes.
            if (panes.none { it !== this && it.nbId == nbId } && SyncHub.sharing(nbId)) SyncHub.presence(nbId, null, FloatArray(4), 0f, null)
            inkOverlay.release()
        }

        /** Two panes: which notebook, which page, ★ (laptop and guests), shown alone, close. */
        @Composable
        fun PaneHead(modifier: Modifier) {
            val isMain = mainPane === this
            Row(modifier.bar(18f).padding(start = 4.dp, end = 4.dp, top = 2.dp, bottom = 2.dp), verticalAlignment = Alignment.CenterVertically) {
                Box(Modifier.size(34.dp).clip(CircleShape).clickable(onClickLabel = if (isMain) "Hauptbereich" else "Zum Hauptbereich machen") {
                    if (isMain) flash("★ Hauptbereich: der Laptop, der Laser und Herbeirufen gehören hierher") else { setMain(this@Pane); flash("★ Laptop und Gäste sehen jetzt diesen Bereich") }
                }, contentAlignment = Alignment.Center) { GlyphIcon(Glyph.STAR, if (isMain) Ui.Red else Ui.Muted, 17.dp) }
                Text(title, color = Ui.Chalk, fontSize = 13.sp, fontWeight = FontWeight.SemiBold, maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.widthIn(max = 200.dp))
                if (pageCount > 0) Text(Voice.cased(" · $currentPage/$pageCount"), color = Ui.Muted, fontSize = 11.sp, fontFamily = Voice.metaFont, maxLines = 1)
                Spacer(Modifier.width(4.dp))
                val alone = maxed === this@Pane
                Box(Modifier.size(34.dp).clip(CircleShape).clickable(onClickLabel = if (alone) "Beide zeigen" else "Nur diesen zeigen") { showAlone(if (alone) null else this@Pane) },
                    contentAlignment = Alignment.Center) { GlyphIcon(if (alone) Glyph.RESTORE else Glyph.MAXIMIZE, Ui.Chalk, 16.dp) }
                Box(Modifier.size(34.dp).clip(CircleShape).clickable(onClickLabel = "Schließen") { closePane(this@Pane) },
                    contentAlignment = Alignment.Center) { GlyphIcon(Glyph.CLOSE, Ui.Chalk, 16.dp) }
            }
        }

        /**
         * ⧉: what goes beside this one — this notebook again at a page picked from its pictures, or
         * another notebook (its cover; the ones opened lately first, a search right away). With two
         * open: swap, one alone, close the other first.
         */
        @Composable
        fun SplitMenu() {
            var books by remember { mutableStateOf<List<Book>?>(null) }
            var find by remember { mutableStateOf("") }
            var typing by remember { mutableStateOf(false) }
            androidx.compose.runtime.LaunchedEffect(Unit) { recentNotebooks { books = it } }
            val other = panes.firstOrNull { it !== this@Pane }
            // Searching: only the search and what it finds (above the keyboard).
            val searching = typing || find.isNotEmpty()
            if (other != null && !searching) {
                Section("Zwei nebeneinander")
                Row(Modifier.padding(start = 14.dp, end = 14.dp, top = 4.dp, bottom = 10.dp), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    Tile(Glyph.SPLIT, "Tauschen", Modifier.weight(1f)) { menu = null; swapPanes() }
                    if (maxed == null) Tile(Glyph.MAXIMIZE, "Nur diesen", Modifier.weight(1f)) { menu = null; showAlone(this@Pane) }
                    else Tile(Glyph.RESTORE, "Beide zeigen", Modifier.weight(1f)) { menu = null; showAlone(null) }
                    Tile(Glyph.CLOSE, "Anderen zu", Modifier.weight(1f)) { menu = null; closePane(other) }
                }
                MenuDivider()
            }
            // This notebook again: the page to show, from its pictures (the one in view marked).
            val pages = rows.filterIsInstance<FfiOutlineRow.Sheet>()
            val cur = view.currentSheet()?.sheetId
            val strip = androidx.compose.foundation.lazy.rememberLazyListState()
            if (!searching) Section(if (other != null) "Im anderen Bereich: eine Seite von hier" else "Eine Seite von hier daneben")
            if (!searching) androidx.compose.foundation.lazy.LazyRow(state = strip, contentPadding = androidx.compose.foundation.layout.PaddingValues(horizontal = 14.dp, vertical = 6.dp),
                horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                items(pages.size, key = { pages[it].node }) { i ->
                    val r = pages[i]
                    val here = r.sheetId == cur
                    Column(Modifier.width(70.dp).clip(RoundedCornerShape(8.dp)).clickable(onClickLabel = "Daneben öffnen") { openBeside(this@Pane, nbId, r.sheetId) }.padding(bottom = 2.dp),
                        horizontalAlignment = Alignment.CenterHorizontally) {
                        PageThumb(r, 70.dp, current = here, box = true)
                        Text(if (here) "Diese Seite" else "Seite ${model?.numberOf(r.node) ?: i + 1}", color = if (here) Ui.Red else Ui.Muted, fontSize = 11.sp,
                            fontWeight = if (here) FontWeight.SemiBold else FontWeight.Normal, maxLines = 1, modifier = Modifier.padding(top = 4.dp))
                    }
                }
            }
            if (!searching) androidx.compose.runtime.LaunchedEffect(Unit) { pages.indexOfFirst { it.sheetId == cur }.takeIf { it > 0 }?.let { strip.scrollToItem(it - 1) } }
            if (!searching) MenuDivider()
            // Another notebook: search right away, the ones opened lately first.
            Section("Anderes Notizbuch")
            Row(Modifier.padding(horizontal = 14.dp, vertical = 4.dp).fillMaxWidth().clip(RoundedCornerShape(10.dp)).background(Ui.Black)
                .border(1.dp, Ui.Border, RoundedCornerShape(10.dp)).padding(horizontal = 10.dp, vertical = 8.dp), verticalAlignment = Alignment.CenterVertically) {
                GlyphIcon(Glyph.SEARCH, Ui.Muted, 16.dp)
                Box(Modifier.weight(1f).padding(start = 8.dp)) {
                    if (find.isEmpty()) Text("Titel oder Ordner", color = Ui.Muted, fontSize = 14.sp)
                    BasicTextField(find, { find = it }, singleLine = true, textStyle = TextStyle(color = Ui.Chalk, fontSize = 14.sp), cursorBrush = SolidColor(Ui.Red),
                        modifier = Modifier.fillMaxWidth().onFocusChanged { typing = it.isFocused })
                }
                if (find.isNotEmpty()) Text("✕", color = Ui.Muted, fontSize = 14.sp, modifier = Modifier.clickable { find = "" }.padding(start = 8.dp))
            }
            val list = books
            if (list == null) {
                Text("…", color = Ui.Muted, fontSize = 13.sp, modifier = Modifier.padding(horizontal = 18.dp, vertical = 10.dp))
                return
            }
            val others = list.filter { it.id != nbId && it.id != other?.nbId }
            val shown = others.filter { LibraryOrg.matches(it.title, it.folder, emptyList(), find) }
            Column(Modifier.padding(top = 4.dp).heightIn(max = 300.dp).verticalScroll(androidx.compose.foundation.rememberScrollState())) {
                shown.forEach { b -> androidx.compose.runtime.key(b.id) { BookRow(b) } }
                if (shown.isEmpty()) Text(if (others.isEmpty()) "Noch keine anderen Notizbücher" else "Nichts gefunden", color = Ui.Muted, fontSize = 13.sp,
                    modifier = Modifier.padding(horizontal = 18.dp, vertical = 10.dp))
            }
        }

        /** A notebook in the split menu: cover, title, folder, size, when last opened. */
        @Composable
        fun BookRow(b: Book) {
            Row(Modifier.fillMaxWidth().clickable(onClickLabel = "Daneben öffnen") { openBeside(this@Pane, b.id, null) }.padding(horizontal = 14.dp, vertical = 6.dp),
                verticalAlignment = Alignment.CenterVertically) {
                CoverArt(b.cover, b.title, b.id, Modifier.size(width = 34.dp, height = 44.dp).border(1.dp, Ui.Border, RoundedCornerShape(minOf(Ui.theme.style.card - 8f, 10f).coerceAtLeast(0f).dp)))
                Column(Modifier.weight(1f).padding(start = 12.dp)) {
                    Text(b.title.ifBlank { "Ohne Titel" }, color = Ui.Chalk, fontSize = 14.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
                    val meta = listOfNotNull(LibraryOrg.clean(b.folder).takeIf { it.isNotBlank() }?.replace("/", " › "),
                        "${b.pages} ${if (b.pages == 1) "Seite" else "Seiten"}", b.opened.takeIf { it > 0 }?.let { timeAgo(it) })
                    Text(meta.joinToString(" · "), color = Ui.Muted, fontSize = 11.5.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
                }
            }
        }

        /** An outline page dragged here opens beside (the other pane, or the right third when alone). */
        var besideSheet by mutableStateOf<String?>(null)
        /** The page being dragged out of the outline and where the finger is (window px): its picture follows. */
        var ghost by mutableStateOf<Pair<FfiOutlineRow.Sheet, androidx.compose.ui.geometry.Offset>?>(null)

        /** The dragged page's picture under the finger (drawn over everything: it crosses to the other pane). */
        @Composable
        fun Ghost() {
            val (row, at) = ghost ?: return
            val dens = androidx.compose.ui.platform.LocalDensity.current
            val half = with(dens) { 45.dp.toPx() }
            Box(Modifier.offset { androidx.compose.ui.unit.IntOffset((at.x - half).toInt(), (at.y - half * 1.4f).toInt()) }
                .graphicsLayer { alpha = 0.9f; shadowElevation = 16f; rotationZ = -3f }) { PageThumb(row, 90.dp, box = true) }
        }
        /** Where each outline row sits in the window (for dragging a page beside). */
        val rowRoot = HashMap<String, androidx.compose.ui.geometry.Offset>()

        fun inBesideZone(x: Float, y: Float): Boolean {
            val me = panes.indexOf(this)
            val other = panes.indices.firstOrNull { it != me } ?: return paneRects.getOrNull(me)?.let { x > it.left + it.width() * 0.62f } == true
            return paneRects.getOrNull(other)?.takeIf { !it.isEmpty }?.contains(x.toInt(), y.toInt()) == true
        }

        var canUndo by mutableStateOf(false)
        var canRedo by mutableStateOf(false)
        var title by mutableStateOf("")
        var rows by mutableStateOf<List<FfiOutlineRow>>(emptyList())
        var sidebar by mutableStateOf(false)
        /** The outline as pages with pictures (else the list with headings), only the marked pages, which are marked. */
        var outlinePics by mutableStateOf(Settings.str("outline_view", "list") == "pics")
        var onlyMarked by mutableStateOf(false)
        var marks by mutableStateOf<Set<String>>(emptySet())
        /** The page whose menu is open in the outline's pictures. */
        var picMenu by mutableStateOf<String?>(null)
        /** The open dropdown: "add", "export", "pen", "width:<i>", "color:<i>" or null. */
        var menu by mutableStateOf<String?>(null)
        val anchors = androidx.compose.runtime.mutableStateMapOf<String, androidx.compose.ui.geometry.Rect>()
        var newPaper by mutableStateOf(Settings.defaultPaper)
        var zoom by mutableIntStateOf(100)
        var lock by mutableStateOf(NotebookView.Lock.NONE)
        val zoomLocked get() = lock != NotebookView.Lock.NONE
        /** All pages (also in folded groups) while the "share selected pages" dialog is open. */
        var pagePicker by mutableStateOf<List<FfiOutlineRow>?>(null)
        var textEdit by mutableStateOf<TextEdit?>(null)
        /** The text editor on the page (over a text box or a text page). */
        lateinit var inline: InlineTextEditor
        var inlineOn: Pair<String, String>? = null
        /** Text pages: STIFT (draw on them) instead of writing. */
        var drawOnTextPages by mutableStateOf(false)
        var onTextPage by mutableStateOf(false)
        var textBlock by mutableStateOf("p")
        var textInline by mutableStateOf(emptySet<String>())
        /** The group whose menu is open (from its title in the page column). */
        var groupMenu by mutableStateOf<Header?>(null)
        var confirmGroupDelete by mutableStateOf(false)

        // Outline (sidebar) editing: the row showing quick actions, drag state, inline rename.
        var held by mutableStateOf<String?>(null)
        var dragNode by mutableStateOf<String?>(null)
        var dragMoved by mutableStateOf(false)
        var dragX = 0f
        var dragY by mutableStateOf(0f)
        var dragStartX = 0f
        var dragStartY = 0f
        var drop by mutableStateOf<Drop?>(null)
        var inlineRename by mutableStateOf<String?>(null)
        var confirmDelete by mutableStateOf<String?>(null)
        /** Top and bottom of each outline row in the sidebar's scrolled content (px). */
        val rowBounds = HashMap<String, Pair<Float, Float>>()

        var selection by mutableStateOf<NotebookView.Selection?>(null)
        /** The outline's search: what is typed, what was found (null: not searched yet). */
        var query by mutableStateOf("")
        var hits by mutableStateOf<List<uniffi.n0te_ffi.FfiSearchHit>?>(null)
        var searchSeq = 0
        /** Every page's headings, for the outline. */
        var heads by mutableStateOf<List<uniffi.n0te_ffi.FfiPageHeading>>(emptyList())
        /** Marking a heading: the page and its strokes, and the words (suggested by the recogniser). */
        var headingFor by mutableStateOf<Pair<String, Set<String>>?>(null)
        var headingText by mutableStateOf(androidx.compose.ui.text.input.TextFieldValue(""))
        /** The heading the selection already is (the dialog renames or unmarks it). */
        var headingOld by mutableStateOf<String?>(null)
        /** The coordinate system being edited: its page, the plot, whether it was just put there. */
        /** The heading row held in the outline (its quick action: no longer a heading). */
        var heldHeading by mutableStateOf<String?>(null)
        var currentPage by mutableIntStateOf(0)
        var pageCount by mutableIntStateOf(0)
        var renaming by mutableStateOf<Pair<String, String>?>(null)
        var status by mutableStateOf<String?>(null)

        var inBoard by mutableStateOf(false)
        /** Counts the model's changes (the outline's page pictures follow it). */
        var modelTick by mutableIntStateOf(0)
        var currentIsBoard by mutableStateOf(false)

        fun openSettings() {
            menu = null
            settingsOpen = true
            paperBeforeSettings = Settings.defaultPaper
            startActivity(Intent(this@NotebookActivity, SettingsActivity::class.java))
        }

        /** Saves the first page as the library preview (filesDir/thumbs/<id>.png). */
        fun saveThumbnail() {
            val id = nbId
            val bmp = runCatching { view.renderThumbnail(360) }.getOrNull() ?: return
            val dir = File(filesDir, "thumbs").apply { mkdirs() }
            N0te.io.execute {
                runCatching { File(dir, "$id.png").outputStream().use { bmp.compress(android.graphics.Bitmap.CompressFormat.PNG, 90, it) } }
                bmp.recycle()
            }
        }

        var defaultSizeApplied = false

        /**
         * A new page like the one on screen: its format, and its spacing while the paper stays the
         * same — so a notebook keeps the paper it was created with (5 mm boxes stay 5 mm).
         */
        fun newSpec(bg: FfiBackground = newPaper): uniffi.n0te_ffi.FfiSheetSpec {
            val cur = (view.currentSheet() ?: model?.placed?.firstOrNull { it.side == 0 })?.spec
                ?.takeIf { it.pdfBlob == null && it.width > 0f && it.height > 0f }
            val spacing = if (cur != null && cur.background == bg && cur.spacing >= 4f) cur.spacing else Settings.spacingFor(bg)
            return uniffi.n0te_ffi.FfiSheetSpec(cur?.width ?: 595f, cur?.height ?: 842f, bg, spacing, null, 0u)
        }

        fun onModelChanged() {
            val m = model ?: return
            modelTick++
            // Opening: the page at the default size (page width is what fitWidth already did).
            if (!defaultSizeApplied && m.placed.isNotEmpty()) {
                defaultSizeApplied = true
                applyReadOnly()
                // Opened because the owner summoned us: go where it looks (after the camera is set).
                SyncHub.takeSummon(nbId)?.let { e -> view.postDelayed({ summonedTo(e) }, 500) }
                // New pages get this notebook's paper (as created), not the global default.
                m.placed.firstOrNull { it.side == 0 && !it.isBoard && it.spec.pdfBlob == null && it.spec.background != FfiBackground.TEXT }?.let { newPaper = it.spec.background }
                // Where it was left last time (page, zoom, position); else the first page at the default size.
                val last = startCamera ?: Settings.str("last_view_$nbId", "")
                startCamera = null
                view.post {
                    val at = startSheet?.also { startSheet = null }?.takeIf { sid -> m.placed.any { it.sheetId == sid } }
                    if (at != null) view.showPage(at, animate = false)
                    else if (last.isEmpty() || !view.restoreCamera(last)) {
                        if (Settings.pageScale != "width") m.placed.firstOrNull { it.side == 0 }?.let { view.showPage(it.sheetId, animate = false) }
                    }
                    // Shared from another app ("Teilen → n0te"): onto the page in view, once.
                    shareItems?.takeIf { it.isNotEmpty() }?.let { items ->
                        shareItems = null
                        view.post { placeShared(items) }
                    }
                }
            }
            view.invalidate()
            // Undo / a change from elsewhere: into the editor — never over what is being typed.
            inlineOn?.let { (sid, bid) -> if (!m.isTyping(sid, bid)) m.textOf(sid, bid)?.let { if (it != inline.text?.toString()) inline.show(it, keepCursor = true) } }
            canUndo = m.canUndo(); canRedo = m.canRedo()
            title = m.title
            rows = m.rows
            heads = m.headings
            marks = m.marked
            pageCount = m.totalPages
            onViewState()
        }

        override fun onViewState() {
            val cur = view.currentSheet()
            onTextPage = cur?.spec?.background == FfiBackground.TEXT && view.board == null
            if (::inline.isInitialized) {
                updateWordMode(cur)
                placeInline()
            }
            currentPage = cur?.number ?: 0
            currentIsBoard = cur?.isBoard == true
            inBoard = view.board != null
            zoom = view.zoomPercent()
        }

        override fun onSelectionChanged(sel: NotebookView.Selection?) {
            selection = sel
        }

        /** The tools changed here: text being typed ends unless it is the text tool; then into every pane. */
        fun applyTools() {
            if (tool != NotebookView.Tool.TEXT && inlineOn != null && inlineOn?.second != BODY_ID) finishText()
            this@NotebookActivity.applyTools()
        }

        // --- actions ----------------------------------------------------------------------------

        fun addPage(where: String) {
            val m = model ?: return
            val cur = view.currentSheet()
            when (where) {
                "below" -> m.addSheetAfter(cur?.node, newSpec()) { view.showPage(it) }
                "board" -> m.addSheetAfter(cur?.node, NotebookModel.boardSpec()) { view.scrollTo(it) }
                "left", "right" -> cur?.let { m.addSideSheet(it.mainNode, where == "right", newSpec()) { s -> view.showPage(s) } }
                "group" -> m.addGroupAfter(cur?.node, "Neue Gruppe") { node -> renaming = node to "Neue Gruppe" }
                "pdf" -> { pickFor = this@Pane; pickPdf.launch(arrayOf("application/pdf")) }
                "text" -> m.addSheetAfter(cur?.node, NotebookModel.defaultSpec(FfiBackground.TEXT)) { sid ->
                    view.showPage(sid)
                    drawOnTextPages = false
                    view.post { attachInline(sid, BODY_ID, focus = true) }
                }
            }
            menu = null
        }

        // --- text ---------------------------------------------------------------------------------

        /** The text tool's taps: edit a box, start a new one, or write on a text page. */
        override fun onTextTap(p: Placed, boxId: String?, x: Float, y: Float) {
            if (readOnly) return
            val m = model ?: return
            // A tap elsewhere while editing a box ends it (on another box: edits that one).
            if (inlineOn != null && inlineOn?.second != BODY_ID) {
                finishText()
                if (boxId == null) return
            }
            when {
                p.spec.background == FfiBackground.TEXT -> setDrawOnTextPages(false, focus = true)
                boxId != null -> attachInline(p.sheetId, boxId, focus = true)
                else -> {
                    val w = (p.rect.width() - x - 36f).coerceIn(90f, 340f)
                    m.addTextBox(p.sheetId, x, y, w, 14f) { id -> attachInline(p.sheetId, id, focus = true) }
                }
            }
        }

        /** Puts the inline editor on a text box or a text page's body ([BODY_ID]). */
        fun attachInline(sheetId: String, boxId: String, focus: Boolean) {
            val m = model ?: return
            if (inlineOn != (sheetId to boxId)) {
                detachInline()
                inlineOn = sheetId to boxId
                textEdit = TextEdit(sheetId, boxId, boxId == BODY_ID)
                view.editingText = sheetId to boxId
                inline.hint = if (boxId == BODY_ID) "Schreib los …" else "Text"
                inline.show(m.textOf(sheetId, boxId).orEmpty(), keepCursor = false)
                textBlock = "p"
                inline.visibility = View.VISIBLE
                placeInline()
            }
            if (focus) {
                inline.focusAndShowKeyboard(atEnd = true)
                if (boxId != BODY_ID) view.post { view.revealText(sheetId, boxId) }
            }
        }

        /** Takes the inline editor away: writes the text now; an empty new box is removed again. */
        fun detachInline() {
            val on = inlineOn ?: return
            model?.let { m ->
                if (on.second != BODY_ID && inline.text.isNullOrBlank()) m.deleteTextBox(on.first, on.second) else m.commitPendingTexts()
            }
            inline.hideKeyboard()
            inline.visibility = View.GONE
            inlineOn = null
            textEdit = null
            view.editingText = null
        }

        /** Ends editing a box / leaves the keyboard on a text page. */
        fun finishText() {
            val on = inlineOn ?: return
            if (on.second == BODY_ID) { model?.commitPendingTexts(); inline.hideKeyboard() } else detachInline()
            onViewState()
        }

        fun deleteText() {
            val on = inlineOn ?: return
            if (on.second == BODY_ID) { inline.setText(""); return }
            // Emptied, detaching deletes it (deleting it here as well made two undo steps for one).
            inline.setText("")
            detachInline()
        }

        fun resizeText(delta: Float) {
            val on = inlineOn ?: return
            if (on.second == BODY_ID) return
            val m = model ?: return
            val b = m.sheets[on.first]?.texts?.firstOrNull { it.id == on.second } ?: return
            m.placeTextBox(on.first, b.id, b.x, b.y, b.w, (b.size + delta).coerceIn(8f, 72f))
            placeInline()
        }

        /** The switch on text pages: write like in a word processor, or draw on the page. */
        fun setDrawOnTextPages(draw: Boolean, focus: Boolean = false) {
            drawOnTextPages = draw
            if (draw) {
                if (inlineOn?.second == BODY_ID) detachInline()
            } else {
                val cur = view.currentSheet()
                if (cur != null && cur.spec.background == FfiBackground.TEXT) attachInline(cur.sheetId, BODY_ID, focus)
            }
            onViewState()
        }

        /** Keeps the inline editor on its box / page at the current zoom and scroll. */
        fun placeInline() {
            val (sid, bid) = inlineOn ?: return
            val m = model ?: return
            val p = m.placed.firstOrNull { it.sheetId == sid }
            if (p == null) { inline.visibility = View.INVISIBLE; return }
            val s = view.scale
            val left = view.tx + p.rect.left * s
            val top = view.ty + p.rect.top * s
            if (bid == BODY_ID) {
                inline.place(left, top, p.rect.width(), 12f, p.rect.height(), floatArrayOf(56f, 64f, 56f, 56f), s)
            } else {
                val b = m.sheets[sid]?.texts?.firstOrNull { it.id == bid } ?: return
                inline.place(left + b.x * s, top + b.y * s, b.w, b.size, b.size * 1.5f, floatArrayOf(0f, 0f, 0f, 0f), s)
            }
            if (inline.visibility != View.VISIBLE) inline.visibility = View.VISIBLE
        }

        /** Word mode follows the page in the middle; a page being typed on stays while visible. */
        fun updateWordMode(cur: Placed?) {
            val on = inlineOn
            if (on != null && on.second != BODY_ID) return // a text box is being edited
            val typingVisible = on != null && inline.hasFocus() && model?.placed?.firstOrNull { it.sheetId == on.first }?.let { p ->
                val top = view.ty + p.rect.top * view.scale; val bottom = view.ty + p.rect.bottom * view.scale
                bottom > 0 && top < view.height
            } == true
            if (typingVisible) return
            val want = if (!drawOnTextPages && !inBoard && cur != null && cur.spec.background == FfiBackground.TEXT) cur.sheetId else null
            if (want == null) { if (on != null) detachInline() }
            else if (on?.first != want) attachInline(want, BODY_ID, focus = false)
        }

        /** With the keyboard open: scrolls so the cursor stays above it. */
        fun keepCursorVisible() {
            if (inlineOn == null || !inline.hasFocus()) return
            val ime = androidx.core.view.ViewCompat.getRootWindowInsets(view)?.getInsets(androidx.core.view.WindowInsetsCompat.Type.ime())?.bottom ?: 0
            if (ime <= 0) return
            val bottom = inline.cursorBottomOnScreen() ?: return
            val d = resources.displayMetrics.density
            // The keyboard's top in this pane (the upper of two panes may end above it).
            val at = IntArray(2).also { view.getLocationInWindow(it) }
            val imeTop = (view.rootView.height - ime - at[1]).toFloat()
            val limit = minOf(view.height.toFloat(), imeTop) - 28 * d
            if (bottom > limit) view.nudge(-(bottom - limit + 72 * d))
            else if (bottom < 150 * d) view.nudge(150 * d - bottom + 40 * d)
        }

        /** Formatting (and on text pages the TEXT / STIFT switch) instead of the pens. */
        @Composable
        fun TextTools() {
            val e = textEdit ?: return
            if (onTextPage) ModeSwitch()
            TBtn("¶", textBlock == "p") { inline.setBlock("p") }
            TBtn("H1", textBlock == "h1") { inline.setBlock("h1") }
            TBtn("H2", textBlock == "h2") { inline.setBlock("h2") }
            TBtn("H3", textBlock == "h3") { inline.setBlock("h3") }
            Divider()
            TBtn("B", "**" in textInline, bold = true) { inline.toggleInline("**") }
            TBtn("I", "*" in textInline, italic = true) { inline.toggleInline("*") }
            TBtn("</>", "`" in textInline, mono = true) { inline.toggleInline("`") }
            TBtn("∑", "$" in textInline) { inline.toggleInline("$") }
            Divider()
            TBtn("•", textBlock == "ul") { inline.setBlock("ul") }
            TBtn("1.", textBlock == "ol") { inline.setBlock("ol") }
            TBtn("☐", textBlock == "todo") { inline.setBlock("todo") }
            TBtn("❝", textBlock == "quote") { inline.setBlock("quote") }
            if (!e.body) {
                Divider()
                TBtn("A−") { resizeText(-2f) }
                TBtn("A+") { resizeText(2f) }
                IconBtn(Glyph.TRASH) { deleteText() }
                IconBtn(Glyph.CHECK, selected = true) { finishText() }
            } else {
                Divider()
                IconBtn(Glyph.DOWN) { finishText() }
            }
        }

        /** TEXT | STIFT on a text page. */
        @Composable
        fun ModeSwitch() {
            Row(Modifier.padding(end = 6.dp).clip(RoundedCornerShape(18.dp)).background(Ui.Chip).padding(3.dp)) {
                for ((label, draw) in listOf("Text" to false, "Stift" to true)) {
                    val on = drawOnTextPages == draw
                    Text(Voice.cased(label), color = if (on) Ui.Black else Ui.Chalk, fontSize = 12.sp, fontFamily = Voice.labelFont, fontWeight = FontWeight.Bold,
                        modifier = Modifier.clip(RoundedCornerShape(15.dp)).background(if (on) Ui.Chalk else Color.Transparent)
                            .clickable { if (!on) setDrawOnTextPages(draw, focus = !draw) }.padding(horizontal = 10.dp, vertical = 6.dp))
                }
            }
        }

        @Composable
        fun TBtn(label: String, selected: Boolean = false, bold: Boolean = false, italic: Boolean = false, mono: Boolean = false, onClick: () -> Unit) {
            Box(Modifier.size(38.dp).clip(CircleShape).background(if (selected) Ui.Chalk else Color.Transparent).clickable(onClick = onClick), contentAlignment = Alignment.Center) {
                Text(label, color = if (selected) Ui.Black else Ui.Chalk, fontSize = if (label.length > 2) 12.sp else 15.sp,
                    fontWeight = if (bold || label.startsWith("H")) FontWeight.Bold else FontWeight.Medium,
                    fontStyle = if (italic) androidx.compose.ui.text.font.FontStyle.Italic else androidx.compose.ui.text.font.FontStyle.Normal,
                    fontFamily = if (mono) FontFamily.Monospace else FontFamily.Default)
            }
        }

        // --- live with the laptop -----------------------------------------------------------------

        /** Connected to the laptop: a chip (● LAPTOP); tap for open / follow. */
        @Composable
        fun LiveChip() {
            // The laptop goes with the main pane (★).
            if (mainPane !== this) return
            val laptop = SyncHub.laptop ?: return
            val open = menu == "live"
            val text = androidx.compose.ui.text.buildAnnotatedString {
                pushStyle(androidx.compose.ui.text.SpanStyle(color = if (laptop.online) Ui.Red else Ui.Muted))
                append(if (laptop.online) "● " else "○ ")
                pop()
                append(Voice.cased(if (laptopFollows && laptop.online) "Laptop · folgt dir" else "Laptop"))
            }
            Text(text, color = if (open) Ui.Black else if (laptop.online) Ui.Chalk else Ui.Muted, fontSize = 12.sp, fontFamily = Voice.metaFont,
                modifier = Modifier.anchor("live").clip(RoundedCornerShape(12.dp)).background(if (open) Ui.Chalk else Ui.Chip)
                    .clickable { menu = if (open) null else "live" }.padding(horizontal = 10.dp, vertical = 5.dp))
        }

        @Composable
        fun LiveMenu() {
            // Its switches are settings (not state): read the counter they bump, so a tap redraws.
            @Suppress("UNUSED_VARIABLE") val ticked = tick
            val laptop = SyncHub.laptop
            val id = nbId
            if (laptop == null || !laptop.online) {
                Section("Laptop · nicht verbunden")
                Text("Der Laptop ist gerade nicht erreichbar (aus, schläft oder ohne Netz). Sobald er da ist, wird hier wieder abgeglichen.",
                    color = Ui.Muted, fontSize = 13.sp, modifier = Modifier.width(320.dp).padding(horizontal = 18.dp, vertical = 8.dp))
                return
            }
            Section("${laptop.name} · verbunden")
            Item(Glyph.LAPTOP, "Am Laptop öffnen", "Er zeigt dieses Notizbuch, deinen Ausschnitt, Laser und Striche live") {
                SyncHub.requestOpen(laptop.id, id)
                view.pushPresence()
                menu = null
                flash("Laptop öffnet das Notizbuch")
            }
            Item(Glyph.HAND, "Laptop folgen", "Dieser Bildschirm zeigt, was der Laptop zeigt", trailing = if (following != null) "AN" else "AUS") {
                if (following != null) {
                    following = null
                    view.followDevice = null
                } else {
                    // The laptop opens this notebook too and sends where it is.
                    if (laptopFollows) { laptopFollows = false; SyncHub.setFollow(laptop.id, false) }
                    SyncHub.requestOpen(laptop.id, id)
                    following = laptop.id
                    view.followDevice = laptop.id
                    flash("Folgt dem Laptop")
                }
                menu = null
            }
            Item(Glyph.PAGE, "Rahmen des Laptops", "Sein Ausschnitt als roter Rahmen (der Laser bleibt)", trailing = if (Settings.remoteFrame) "AN" else "AUS") {
                Settings.set("remote_frame", !Settings.remoteFrame)
                view.invalidate()
                tick++
            }
            Item(Glyph.SEND, "Laptop folgt mir", "Der Laptop zeigt deinen Ausschnitt", trailing = if (laptopFollows) "AN" else "AUS") {
                val on = !laptopFollows
                if (on) {
                    if (following != null) { following = null; view.followDevice = null }
                    SyncHub.requestOpen(laptop.id, id)
                }
                SyncHub.setFollow(laptop.id, on)
                laptopFollows = on
                view.pushPresence()
                menu = null
                flash(if (on) "Der Laptop folgt dir" else "Laptop folgt nicht mehr")
            }
        }

        override fun onMessage(text: String) = flash(text)

        /**
         * Whether the system clipboard holds a picture, from its description only: reading the clip
         * itself (on every menu shown) made Android tell "n0te hat aus der Zwischenablage eingefügt".
         */
        fun clipHasImage(): Boolean = runCatching {
            val desc = (getSystemService(CLIPBOARD_SERVICE) as android.content.ClipboardManager).primaryClipDescription ?: return false
            (0 until desc.mimeTypeCount).any { desc.getMimeType(it).startsWith("image/") }
        }.getOrDefault(false)

        /** A picture on the system clipboard (copied in Samsung Gallery, Smart Select, a browser …). */
        fun systemClipImage(): android.net.Uri? = runCatching {
            val cm = getSystemService(CLIPBOARD_SERVICE) as android.content.ClipboardManager
            val clip = cm.primaryClip ?: return null
            val desc = clip.description
            if ((0 until desc.mimeTypeCount).none { desc.getMimeType(it).startsWith("image/") }) return null
            (0 until clip.itemCount).firstNotNullOfOrNull { clip.getItemAt(it).uri }
        }.getOrNull()

        fun pasteSystemImage() {
            val uri = systemClipImage() ?: return flash("Kein Bild in der Zwischenablage")
            insertPictures(listOf(uri), null)
        }

        /**
         * Pictures from elsewhere (drag & drop from Samsung Gallery in split screen, Smart Select,
         * the Edge Panel, the clipboard): stored upright as JPEG and placed at the drop point (or the
         * page in view). A dropped PDF becomes pages after the current one.
         */
        fun insertPictures(uris: List<android.net.Uri>, at: android.graphics.PointF?) {
            val m = model ?: return
            status = if (uris.size == 1) "Bild wird eingefügt …" else "${uris.size} Bilder werden eingefügt …"
            N0te.background({
                uris.mapNotNull { uri ->
                    val type = contentResolver.getType(uri) ?: ""
                    val bytes = contentResolver.openInputStream(uri)?.use { it.readBytes() } ?: return@mapNotNull null
                    if (type == "application/pdf" || (bytes.size > 4 && String(bytes, 0, 4, Charsets.ISO_8859_1) == "%PDF")) Pair(bytes, null)
                    else Images.prepare(bytes)?.let { (jpeg, w, h) -> Pair(ByteArray(0), Triple(N0te.library.putBlob(jpeg), w, h)) }
                }
            }) { r ->
                status = null
                r.onSuccess { items ->
                    if (items.isEmpty()) { flash("Nichts Einfügbares dabei"); return@onSuccess }
                    tool = NotebookView.Tool.LASSO; applyTools()
                    items.forEachIndexed { k, (pdf, img) ->
                        if (img != null) {
                            val (blob, w, h) = img
                            val off = k * 24f
                            if (at != null) view.insertImageAt(blob, w, h, at.x + off, at.y + off) else view.insertImage(blob, w, h)
                        } else {
                            m.importPdfAfter(pdf, view.currentSheet()?.node) { res -> flash(res.fold({ "$it PDF-Seiten eingefügt" }, { "PDF-Import fehlgeschlagen: ${it.message}" })) }
                        }
                    }
                }.onFailure { flash("Einfügen fehlgeschlagen: ${it.message}") }
            }
        }

        /** What "Teilen → n0te" prepared ("img|blob|w|h", "pdf|path"): onto the page in view. */
        fun placeShared(items: List<String>) {
            val m = model ?: return
            tool = NotebookView.Tool.LASSO; applyTools()
            for (it in items) {
                val f = it.split('|')
                when (f[0]) {
                    "img" -> if (f.size >= 4) view.insertImage(f[1], f[2].toIntOrNull() ?: 800, f[3].toIntOrNull() ?: 600)
                    "pdf" -> N0te.background({ java.io.File(f[1]).let { file -> file.readBytes().also { file.delete() } } }) { r ->
                        r.onSuccess { bytes -> m.importPdfAfter(bytes, view.currentSheet()?.node) { res -> flash(res.fold({ n -> "$n PDF-Seiten eingefügt" }, { e -> "PDF-Import fehlgeschlagen: ${e.message}" })) } }
                            .onFailure { flash("Die geteilte PDF ist nicht lesbar") }
                    }
                }
            }
        }

        /** Drag & drop onto the page (multi-window, Smart Select, Edge Panel): accepts pictures and PDFs. */
        var dropHint by mutableStateOf(false)

        fun acceptsDrag(d: android.content.ClipDescription?): Boolean =
            d != null && (0 until d.mimeTypeCount).any { val t = d.getMimeType(it); t.startsWith("image/") || t == "application/pdf" || t == "text/uri-list" || t == android.content.ClipDescription.MIMETYPE_TEXT_URILIST }

        val dragListener = android.view.View.OnDragListener { _, e ->
            when (e.action) {
                android.view.DragEvent.ACTION_DRAG_STARTED -> acceptsDrag(e.clipDescription)
                android.view.DragEvent.ACTION_DRAG_ENTERED -> { dropHint = true; true }
                android.view.DragEvent.ACTION_DRAG_EXITED, android.view.DragEvent.ACTION_DRAG_ENDED -> { dropHint = false; true }
                android.view.DragEvent.ACTION_DROP -> {
                    dropHint = false
                    // The other app's content: readable for this drop.
                    runCatching { requestDragAndDropPermissions(e) }
                    val clip = e.clipData
                    val uris = (0 until (clip?.itemCount ?: 0)).mapNotNull { clip?.getItemAt(it)?.uri }
                    if (uris.isEmpty()) false else { insertPictures(uris, android.graphics.PointF(e.x, e.y)); true }
                }
                else -> true
            }
        }

        override fun onSnipped() {
            canPaste = true
            status = "Ausschnitt kopiert · zum Einfügen den Finger auf die Stelle halten"
            N0te.main.removeCallbacks(clearStatus)
            N0te.main.postDelayed(clearStatus, 3500)
        }

        /**
         * Live events for this notebook. The same notebook in both panes shares one model: the
         * first pane reloads it and speaks (messages once), both views show what changed.
         */
        fun onSync(e: uniffi.n0te_ffi.FfiSyncEvent) {
            if (closed) return
            val id = nbId
            when (e) {
                is uniffi.n0te_ffi.FfiSyncEvent.RemoteChanged -> if (e.notebook == id && leads) {
                    val m = model
                    m?.refreshFromDisk(e.index, e.sheets) { ids -> panes.filter { it.model === m }.forEach { it.reloaded(ids) } }
                }
                is uniffi.n0te_ffi.FfiSyncEvent.Presence -> if (e.notebook == id) {
                    // Looking nowhere (e.g. the owner stopped sharing its view): its frame goes.
                    val sheet = e.sheet ?: run { lastViews.remove(e.device); return view.hideRemote(e.device) }
                    val v = e.viewport
                    if (v.size < 4) return
                    lastViews[e.device] = sheet to android.graphics.RectF(v[0], v[1], v[2], v[3])
                    // Guests' (and, as a guest, the owner's) frames only when wanted; everyone named.
                    val own = SyncHub.devices.any { it.id == e.device }
                    if (SyncHub.isGuest(e.device)) guestPages = guestPages + (e.device to sheet)
                    if (!own && !Settings.guestFrames) {
                        if (view.followDevice == e.device) view.lookAt(sheet, android.graphics.RectF(v[0], v[1], v[2], v[3]))
                        return
                    }
                    val name = SyncHub.nameOf(e.device)
                    view.showRemote(e.device, NotebookView.RemoteView(name, sheet, android.graphics.RectF(v[0], v[1], v[2], v[3]), e.laser?.takeIf { it.size >= 2 }?.let { android.graphics.PointF(it[0], it[1]) }, own))
                }
                is uniffi.n0te_ffi.FfiSyncEvent.LiveStroke -> if (e.notebook == id) view.remoteSamples(e.device, e.stroke.toLong(), e.sheet, e.brush, e.samples, e.done)
                is uniffi.n0te_ffi.FfiSyncEvent.RoleChanged -> if (e.notebook == id) {
                    applyReadOnly()
                    if (leads) flash(if (e.write) "Du darfst jetzt mitschreiben" else "Ab jetzt nur noch lesen")
                }
                // A new guest sees where I look right away (it can jump there / follow).
                is uniffi.n0te_ffi.FfiSyncEvent.GuestJoined -> if (e.notebook == id) { if (leads) flash("${e.name} ist dazugekommen"); view.pushPresence() }
                // The owner brings this view to its own (the main pane's, when the notebook is open twice).
                is uniffi.n0te_ffi.FfiSyncEvent.Summoned -> if (e.notebook == id && presenceOwner(id) === this) summonedTo(e)
                is uniffi.n0te_ffi.FfiSyncEvent.GuestWaiting -> if (e.notebook == id && e.on && leads) flash("${e.name} möchte herein · tippe unten rechts auf „wartet“")
                is uniffi.n0te_ffi.FfiSyncEvent.GuestLeft -> if (e.notebook == id) { guestPages = guestPages - e.device; view.hideRemote(e.device) }
                is uniffi.n0te_ffi.FfiSyncEvent.LiveErase -> if (e.notebook == id) view.remoteErase(e.sheet, e.ids)
                // Pointers: own devices', the owner's and guests' (each in its colour).
                is uniffi.n0te_ffi.FfiSyncEvent.Laser -> if (e.notebook == id) view.remoteLaser(e.who, e.sheet, e.samples.toFloatArray(), e.down, e.guest)
                is uniffi.n0te_ffi.FfiSyncEvent.FollowRequest -> {
                    following = if (e.on) e.device else null
                    view.followDevice = following
                    // The laptop leads now: it stopped following this view when it asked.
                    if (e.on) laptopFollows = false
                    flash(if (e.on) "Folgt dem Laptop" else "Folgen beendet")
                }
                is uniffi.n0te_ffi.FfiSyncEvent.DeviceOffline -> if (e.id == following || laptopFollows) {
                    // Gone (or restarting): nobody follows anyone any more.
                    following = null
                    view.followDevice = null
                    laptopFollows = false
                }
                is uniffi.n0te_ffi.FfiSyncEvent.AccessRevoked -> if (e.notebook == id && leads) flash("Die Einladung wurde widerrufen")
                else -> {}
            }
        }

        /** Sheets another device changed are in the model again: redraw them, take over text typed there. */
        fun reloaded(ids: List<String>) {
            ids.forEach { view.sheetReloaded(it) }
            // Typed on the laptop into the text being edited here: take it over (keep the cursor).
            inlineOn?.let { (sid, bid) -> if (sid in ids && model?.isTyping(sid, bid) != true) model?.textOf(sid, bid)?.let { inline.show(it, keepCursor = true) } }
        }

        /** Where the finger rested (the hold menu acts there). */
        var holdAt = android.graphics.PointF()
        /** A picture picked from the hold menu goes here (null: the page in view). */
        var pickAt: android.graphics.PointF? = null

        override fun onFollowStopped() {
            following = null
            flash("Folgen beendet")
        }

        // --- the writing magnifier ---------------------------------------------------------------

        var loupe by mutableStateOf(false)

        override fun onLoupeClosed() { loupe = false }
        var padTick by mutableIntStateOf(0)
        override fun onLoupePadMoved() { padTick++ }

        fun toggleLoupe() {
            if (loupe) { view.closeLoupe(); loupe = false; return }
            if (view.openLoupe()) {
                loupe = true
                if (!Settings.loupeHintSeen) {
                    Settings.set("loupe_hint_seen2", true)
                    flash("Im Schreibfeld groß schreiben. Rahmen auf der Seite: Finger zieht ihn, die Ecke ändert die Größe. Der Griff oben verschiebt das Schreibfeld.")
                }
            } else flash("Die Schreiblupe gibt es auf normalen Seiten, nicht auf Boards")
        }

        /** On the strip's top edge: frame back, on, new line; close. (Size: the frame's corner or a pinch in the strip.) */
        @Composable
        fun LoupeControls(modifier: Modifier) {
            Row(modifier.bar(21f).padding(horizontal = 4.dp, vertical = 2.dp), horizontalArrangement = Arrangement.spacedBy(2.dp), verticalAlignment = Alignment.CenterVertically) {
                PadBtn(Glyph.ARROW_LEFT, "Zurück") { view.loupeStep(forward = false) }
                PadBtn(Glyph.ARROW_RIGHT, "Weiter") { view.loupeStep(forward = true) }
                PadBtn(Glyph.RETURN, "Neue Zeile") { view.loupeNewLine() }
                Box(Modifier.padding(horizontal = 4.dp).size(width = 1.dp, height = 20.dp).background(Ui.Border))
                PadBtn(Glyph.CLOSE, "Schließen") { view.closeLoupe(); loupe = false }
            }
        }

        @Composable
        fun PadBtn(g: Glyph, label: String, onClick: () -> Unit) {
            // clickable's label names the button for TalkBack.
            Box(Modifier.size(38.dp).clip(CircleShape).clickable(onClickLabel = label, onClick = onClick), contentAlignment = Alignment.Center) { GlyphIcon(g, Ui.Chalk, 20.dp) }
        }

        /** Jumps to where the owner (or own laptop) asked; says who brought us here. */
        fun summonedTo(e: uniffi.n0te_ffi.FfiSyncEvent.Summoned) {
            val v = e.viewport
            if (v.size < 4) return
            if (view.lookAt(e.sheet, android.graphics.RectF(v[0], v[1], v[2], v[3]))) flash("${SyncHub.nameOf(e.device)} hat dich zu sich geholt")
        }

        override fun onFingerHold(x: Float, y: Float) {
            if (readOnly || textEdit != null) return
            holdAt = android.graphics.PointF(x, y)
            anchors["hold"] = pointAnchor(x, y)
            menu = "hold"
        }

        /**
         * A small rectangle around (x, y) of this pane's page view, in the window: menus open
         * there (the view's points are its own, the second pane's start at the divider).
         */
        fun pointAnchor(x: Float, y: Float): androidx.compose.ui.geometry.Rect {
            val at = IntArray(2).also { view.getLocationInWindow(it) }
            val cx = x + at[0]
            val cy = y + at[1]
            return androidx.compose.ui.geometry.Rect(cx - 4f, cy - 4f, cx + 4f, cy + 4f)
        }

        /** A finger held on the page: a light bar of what can go here (paste, picture, text, graph …). */
        @Composable
        fun HoldMenu() {
            val clip = N0te.clip
            val sys = remember { clipHasImage() }
            val at = holdAt
            Row(Modifier.padding(horizontal = 8.dp, vertical = 2.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(2.dp)) {
                when {
                    clip != null -> LabeledBtn(Glyph.PASTE, "Einfügen") { menu = null; view.pasteAt(at.x, at.y) }
                    sys -> LabeledBtn(Glyph.PASTE, "Einfügen") { menu = null; systemClipImage()?.let { insertPictures(listOf(it), at) } ?: flash("Kein Bild in der Zwischenablage") }
                }
                LabeledBtn(Glyph.PAGE, "Bild") { menu = null; pickAt = at; pickFor = this@Pane; pickImage.launch("image/*") }
                view.pageAt(at.x, at.y)?.let { (p, pt) ->
                    LabeledBtn(Glyph.TEXT, "Text") { menu = null; onTextTap(p, null, pt.x, pt.y) }
                    val text = p.spec.background == uniffi.n0te_ffi.FfiBackground.TEXT
                    if (!readOnly && !text) LabeledBtn(Glyph.PLOT, "Graph") { menu = null; view.insertPlot(at.x, at.y) { id, plot -> startGraph(id, plot, true) } }
                    if (!readOnly && Settings.toolLasso && !p.isBoard && !text) LabeledBtn(Glyph.SPACE, "Platz") {
                        menu = null
                        selectDir = 1; selectMode = "space"; Settings.set("select_mode", "space"); tool = NotebookView.Tool.LASSO; applyTools(); tick++
                        flash("Platz einfügen: Stift auf die Stelle, nach unten oder zur Seite ziehen")
                    }
                    if (!readOnly && Settings.toolLoupe && !loupe) LabeledBtn(Glyph.LOUPE, "Lupe") { menu = null; if (view.openLoupeAt(at.x, at.y)) loupe = true }
                    if (!readOnly) {
                        Divider()
                        LabeledBtn(Glyph.MENU, "Seite") {
                            pageFor = p.node
                            anchors["page"] = anchors["hold"] ?: return@LabeledBtn
                            menu = "page"
                        }
                    }
                }
            }
        }

        /** The sheet the page menu edits when it was opened by holding on a page (else the one in focus). */
        var pageFor by mutableStateOf<String?>(null)

        override fun onGroupMenu(h: Header, x: Float, y: Float) {
            if (readOnly) return
            anchors["group"] = pointAnchor(x, y)
            groupMenu = h
            confirmGroupDelete = false
            menu = "group"
        }

        override fun onAddPage(direction: Int, mainNode: String) {
            val m = model ?: return
            val spec = newSpec()
            when (direction) {
                -1 -> m.addSideSheet(mainNode, false, spec) { view.showPage(it) }
                1 -> m.addSideSheet(mainNode, true, spec) { view.showPage(it) }
                else -> m.addSheetAfter(mainNode, spec) { view.showPage(it) }
            }
        }

        /** Exports all pages, or the current row (main page and its side pages), and shares the PDF. */
        fun exportPdf(all: Boolean, toLaptop: Boolean = false) {
            val m = model ?: return
            val ids = if (all) null else view.currentSheet()?.let { cur -> m.placed.filter { it.mainNode == cur.node }.sortedBy { it.side }.map { it.sheetId } }
            exportIds(ids, if (all) "" else "-Seite-$currentPage", toLaptop)
        }

        /** Pages picked in the dialog (main pages and side pages each on their own), in notebook order. */
        fun exportPicked(rows: List<FfiOutlineRow.Sheet>, picked: Set<String>, toLaptop: Boolean) {
            pagePicker = null
            val ids = rows.flatMap { r -> r.left.reversed().map { it.sheetId } + r.sheetId + r.right.map { it.sheetId } }.filter { it in picked }
            if (ids.isEmpty()) return flash("Keine Seite ausgewählt")
            exportIds(ids, if (ids.size == 1) "-1-Seite" else "-${ids.size}-Seiten", toLaptop)
        }

        fun exportIds(ids: List<String>?, suffix: String, toLaptop: Boolean) {
            val m = model ?: return
            menu = null
            val dir = File(cacheDir, "share").apply { mkdirs() }
            val safe = title.ifBlank { "n0te" }.replace(Regex("[^\\p{L}\\p{N} _-]"), "_")
            val f = File(dir, "$safe$suffix.pdf")
            status = "Exportiere …"
            m.exportPdf(ids, f, Settings.exportSidesAtEnd) { r ->
                r.onSuccess { pages ->
                    status = null
                    val uri = FileProvider.getUriForFile(this@NotebookActivity, "$packageName.files", f)
                    val send = Intent(Intent.ACTION_SEND).setType("application/pdf").putExtra(Intent.EXTRA_STREAM, uri).addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
                    if (toLaptop) {
                        // Straight to n0bridge's share sheet (laptop, AirDrop, …).
                        runCatching { startActivity(Intent(send).setPackage("dev.n0bridge.app")) }
                            .onFailure { startActivity(Intent.createChooser(send, "PDF teilen")) }
                    } else {
                        startActivity(Intent.createChooser(send, "$pages Seiten teilen"))
                    }
                }.onFailure { status = "Export fehlgeschlagen: ${it.message}"; N0te.main.postDelayed({ status = null }, 3000) }
            }
        }

        /** An invite link to this notebook (guests connect straight to this tablet / the laptop). */
        fun invite(rw: Boolean, done: () -> Unit = {}) {
            menu = null
            val engine = SyncHub.engine ?: return flash("Sync startet noch …")
            val id = nbId
            val expires = (System.currentTimeMillis() / 1000 + 7 * 86_400).toULong()
            N0te.background({ engine.createInvite(id, if (rw) uniffi.n0te_ffi.FfiRole.READ_WRITE else uniffi.n0te_ffi.FfiRole.READ, expires) }) { r ->
                r.onSuccess { link ->
                    done()
                    (getSystemService(android.content.Context.CLIPBOARD_SERVICE) as android.content.ClipboardManager).setPrimaryClip(android.content.ClipData.newPlainText("n0te", link))
                    startActivity(Intent.createChooser(Intent(Intent.ACTION_SEND).setType("text/plain").putExtra(Intent.EXTRA_TEXT, link), "Einladung teilen"))
                }.onFailure { flash("Einladen fehlgeschlagen: ${it.message}") }
            }
        }

        /** The whole notebook as `.n0te` to the laptop (n0te there merges it with its copy). */
        fun sendNotebookToLaptop() {
            val m = model ?: return
            val id = nbId
            menu = null
            m.flush()
            status = "Packe Notizbuch …"
            N0te.background({
                val dir = File(cacheDir, "share").apply { mkdirs() }
                val safe = title.ifBlank { "Notizbuch" }.replace(Regex("[^\\p{L}\\p{N} _-]"), "_").trim().replace(' ', '_')
                File(dir, "$safe.n0te").also { N0te.library.exportBundle(id, it.path) }
            }) { r ->
                status = null
                r.onSuccess { shareToN0bridge(this@NotebookActivity, it, "application/octet-stream") }.onFailure { flash("Export fehlgeschlagen: ${it.message}") }
            }
        }

        var latexFound by mutableStateOf<LatexFound?>(null)
        var latexText by mutableStateOf("")

        /** The lasso's LaTeX button: the laptop reads the handwriting, a dialog shows the result. */
        fun recognizeSelection() {
            val sel = selection ?: return
            // The formula lands as a text box on one page: the lasso has to stay on it.
            if (sel.multi) { flash("LaTeX: bitte nur auf einer Seite auswählen"); return }
            view.selectionPng { png -> if (png != null) askLatex(sel, png) }
        }

        private fun askLatex(sel: NotebookView.Selection, png: ByteArray) {
            val asked = SyncHub.requestLatex(png) { latex, engine, error ->
                if (error != null) { flash("LaTeX: $error"); return@requestLatex }
                latexText = latex
                latexFound = LatexFound(sel.sheetId, sel.ids, android.graphics.RectF(sel.bounds), engine)
            }
            flash(if (asked) "Lese die Formel … (auf dem Laptop)" else "Laptop nicht verbunden — die Formel wird dort gelesen")
        }

        /** The recognized formula as a text box ($$…$$) below the handwriting, or in its place. */
        fun insertFormula(f: LatexFound, replace: Boolean) {
            val m = model ?: return
            val latex = latexText.trim()
            if (latex.isEmpty()) return
            val y = if (replace) f.bounds.top else f.bounds.bottom + 10f
            m.addTextBox(f.sheetId, f.bounds.left, y, maxOf(f.bounds.width(), 160f), 16f) { id ->
                m.setText(f.sheetId, id, "\$\$" + latex + "\$\$")
                m.commitPendingTexts()
            }
            if (replace) {
                m.deleteStrokes(f.sheetId, f.ids)
                view.clearSelection()
            }
            latexFound = null
            flash(if (replace) "Handschrift durch die Formel ersetzt" else "Formel eingefügt")
        }

        @Composable
        fun LatexDialog() {
            val f = latexFound ?: return
            androidx.compose.ui.window.Dialog(onDismissRequest = { latexFound = null }) {
                Column(Modifier.width(600.dp).panel().padding(22.dp),
                    verticalArrangement = Arrangement.spacedBy(12.dp)) {
                    Caption("Handschrift → LaTeX")
                    // A tall formula or a long text scrolls; the buttons stay on screen.
                    Column(Modifier.weight(1f, fill = false).verticalScroll(androidx.compose.foundation.rememberScrollState()), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                    // The formula as it will look (edits below update it).
                    androidx.compose.ui.viewinterop.AndroidView(
                        factory = { ctx -> android.widget.TextView(ctx).apply { setTextColor(android.graphics.Color.BLACK); textSize = 24f; gravity = android.view.Gravity.CENTER; setPadding(24, 28, 24, 28) } },
                        update = { tv ->
                            val t = latexText.trim()
                            tv.text = if (t.isEmpty()) "" else android.text.SpannableString(t).apply { setSpan(MathSpan(t, true), 0, length, android.text.Spanned.SPAN_EXCLUSIVE_EXCLUSIVE) }
                        },
                        modifier = Modifier.fillMaxWidth().heightIn(min = 90.dp).clip(RoundedCornerShape(14.dp)).background(Color.White))
                    Box(Modifier.fillMaxWidth().clip(RoundedCornerShape(12.dp)).background(Ui.Black).border(1.dp, Ui.Border, RoundedCornerShape(12.dp)).padding(12.dp)) {
                        BasicTextField(latexText, { latexText = it }, textStyle = TextStyle(color = Ui.Chalk, fontSize = 15.sp, fontFamily = FontFamily.Monospace),
                            cursorBrush = SolidColor(Ui.Red), modifier = Modifier.fillMaxWidth())
                    }
                    }
                    Text("Gelesen von: ${f.engine}", color = Ui.Muted, fontSize = 11.sp)
                    Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(8.dp, Alignment.End), verticalAlignment = Alignment.CenterVertically) {
                        DialogPill("Schließen") { latexFound = null }
                        DialogPill("Kopieren") {
                            (getSystemService(CLIPBOARD_SERVICE) as android.content.ClipboardManager).setPrimaryClip(android.content.ClipData.newPlainText("LaTeX", latexText.trim()))
                            flash("LaTeX kopiert")
                        }
                        DialogPill("Einfügen") { insertFormula(f, replace = false) }
                        DialogPill("Ersetzen", primary = true) { insertFormula(f, replace = true) }
                    }
                }
            }
        }

        @Composable
        fun DialogPill(label: String, primary: Boolean = false, onClick: () -> Unit) = Btn(label, if (primary) BtnKind.PRIMARY else BtnKind.SECONDARY, onClick = onClick)

        fun sendSelectionToLaptop() {
            val dir = File(cacheDir, "share").apply { mkdirs() }
            val f = File(dir, "n0te-auswahl.png")
            view.selectionPng { png ->
                png ?: return@selectionPng
                // Written on the I/O thread too.
                N0te.background({ f.writeBytes(png) }) { r -> if (r.isSuccess) offerToLaptop(f) else flash("Konnte das Bild nicht ablegen") }
            }
        }

        private fun offerToLaptop(f: File) {
            val uri = FileProvider.getUriForFile(this@NotebookActivity, "$packageName.files", f)
            val i = Intent("dev.n0bridge.action.N0TE_CLIP")
                .setPackage("dev.n0bridge.app")
                .setDataAndType(uri, "image/png")
                .addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
            runCatching { startActivity(i) }
                // Sent: the selection bar closes (like after copying).
                .onSuccess { view.clearSelection() }
                .onFailure { Toast.makeText(this@NotebookActivity, "n0bridge 0.14.2+ nötig, um an den Laptop zu senden", Toast.LENGTH_LONG).show() }
        }

        // --- chrome -------------------------------------------------------------------------------

        fun Modifier.anchor(key: String) = this.then(Modifier.onGloballyPositioned { anchors[key] = it.boundsInRoot() })

        @Composable
        /**
         * The pane's own controls (the toolbar is drawn once, for the active pane, by the activity):
         * outline, selection bar, chips, graph editor, menus. [split]: two panes (a head on top);
         * [barSpace]: px of this pane under the shared toolbar.
         */
        fun Overlay(split: Boolean, barSpace: Float) {
            val dens = androidx.compose.ui.platform.LocalDensity.current
            val barDp = with(dens) { barSpace.toDp() }
            val headUp = split && graph == null
            Box(Modifier.fillMaxSize()) {
                if (headUp) PaneHead(Modifier.align(Alignment.TopCenter).padding(top = barDp + 8.dp))
                val sideTop = maxOf(barDp + 8.dp, 16.dp) + (if (headUp) 48.dp else 0.dp)
                if (sidebar && graph == null) Sidebar(Modifier.align(Alignment.CenterStart).padding(start = 10.dp, top = if (split || barDp > 0.dp) sideTop else 76.dp, bottom = 16.dp))
                if (graph == null) selection?.let { SelectionBar(Modifier.align(Alignment.BottomCenter).padding(bottom = 22.dp)) }
                graph?.let { g ->
                    g.keys.watchHardware()
                    val keysUp = g.keys.shown
                    // A keyboard is plugged in: the small "Tastatur am Bildschirm" button sits bottom right (the hint keeps above it).
                    val pill = g.keys.active != null && !keysUp
                    val below = if (keysUp) GRAPH_KEYS_H + 10.dp else 16.dp
                    val toBig = { bigGraph(!graphBig) }
                    if (graphBig) {
                        // Groß: like GeoGebra — the panel, the bar and the graph beside each other,
                        // nothing floating; the graph fills all the room that is left.
                        // The bar across the whole top, so a narrow pane (split screen) still has room for it.
                        androidx.compose.foundation.layout.BoxWithConstraints(Modifier.fillMaxSize().background(Ui.Black)) {
                            // In a narrow pane the panel takes at most half the width.
                            val panelMax = maxWidth * 0.5f
                            Column(Modifier.fillMaxSize()) {
                                GraphToolbar(g, graphTick, graphPanel, true, { graphPanel = !graphPanel }, toBig,
                                    { graphPanel = true; g.openSettings(null); g.tab = 1 }, { finishGraph(true) }, { finishGraph(false) }, Modifier.fillMaxWidth(), docked = true)
                                Row(Modifier.weight(1f).fillMaxWidth()) {
                                    if (graphPanel) GraphPanel(g, graphTick, Modifier.fillMaxHeight().widthIn(max = panelMax), docked = true)
                                    Box(Modifier.weight(1f).fillMaxHeight()) {
                                        GraphCanvas(g, graphTick + tick, { graphBox(g) })
                                        GraphHint(g, graphTick, { id -> graphPanel = true; g.openSettings(id) }, Modifier.align(Alignment.BottomCenter).padding(bottom = if (pill) 62.dp else 16.dp, start = 16.dp, end = 16.dp))
                                        if (!keysUp) GraphKeyboard(g.keys, Modifier.align(Alignment.BottomEnd))
                                    }
                                }
                                if (keysUp) GraphKeyboard(g.keys)
                            }
                        }
                        return@let
                    }
                    if (graphPanel) GraphPanel(g, graphTick, Modifier.align(Alignment.TopStart).statusBarsPadding().padding(top = 80.dp, start = 10.dp, bottom = below))
                    GraphHint(g, graphTick, { id -> graphPanel = true; g.openSettings(id) },
                        Modifier.align(Alignment.BottomCenter).padding(bottom = if (keysUp) GRAPH_KEYS_H + 12.dp else if (pill) 66.dp else 22.dp, start = if (graphPanel) (GRAPH_PANEL_W + 20).dp else 16.dp, end = 16.dp))
                    // Centred over the graph's side of the screen.
                    GraphToolbar(g, graphTick, graphPanel, graphBig, { graphPanel = !graphPanel }, toBig,
                        { graphPanel = true; g.openSettings(null); g.tab = 1 }, { finishGraph(true) }, { finishGraph(false) },
                        Modifier.align(Alignment.TopCenter).statusBarsPadding().padding(top = 8.dp, start = if (graphPanel) (GRAPH_PANEL_W + 20).dp else 0.dp))
                    GraphKeyboard(g.keys, Modifier.align(Alignment.BottomCenter))
                    // The graph where it is on the page, out from under the panel: when the editor opens and when
                    // the panel or Groß toggles — typing never moves it (the keyboard at most nudges it up).
                    androidx.compose.runtime.LaunchedEffect(g, graphBig, graphPanel) { kotlinx.coroutines.delay(60); fitGraph() }
                    androidx.compose.runtime.LaunchedEffect(g, keysUp) { if (keysUp) { kotlinx.coroutines.delay(60); keepGraphAboveKeys() } }
                }
                LatexDialog()
                // Live: this view's position every 2 s while the laptop is connected (it shows a frame).
                androidx.compose.runtime.LaunchedEffect(Unit) {
                    while (true) {
                        kotlinx.coroutines.delay(2000)
                        // Only while in front: stopped (Home, screen off) the laptop and guests kept seeing a frame.
                        if (SyncHub.sharing(nbId) && lifecycle.currentState.isAtLeast(androidx.lifecycle.Lifecycle.State.RESUMED)) view.pushPresence()
                    }
                }
                // The writing magnifier's strip is drawn by the page view; its buttons sit on its top edge.
                @Suppress("UNUSED_VARIABLE") val padMoved = padTick
                val strip = if (loupe) view.loupeStrip() else null
                val loupeTop = strip?.let { with(dens) { (view.height - it.top).toDp() } } ?: 0.dp
                // Never below 0 (a pane dragged very short: Compose throws on negative padding).
                if (strip != null) LoupeControls(Modifier.align(Alignment.BottomStart).padding(start = 30.dp, bottom = (loupeTop - 21.dp).coerceAtLeast(0.dp)))
                // The chips step above the strip while it lies at the bottom.
                val chipsLift = strip != null && strip.bottom > view.height - with(dens) { 90.dp.toPx() }
                // While a graph is edited only its own controls show.
                if (graph == null) Row(Modifier.align(Alignment.BottomEnd).padding(start = 18.dp, top = 18.dp, end = 18.dp, bottom = if (chipsLift) (loupeTop + 14.dp).coerceAtLeast(0.dp) else 18.dp),
                    horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
                    GuestChip()
                    LiveChip()
                    following?.let {
                        // Tap: stop following (moving the page by hand stops it too).
                        Text(Voice.cased("● Folgt ${SyncHub.nameOf(it)}"), color = Ui.OnAccent, fontSize = 11.sp, fontFamily = Voice.metaFont, fontWeight = FontWeight.Bold,
                            modifier = Modifier.clip(RoundedCornerShape(12.dp)).background(Ui.Red).clickable { following = null; view.followDevice = null }.padding(horizontal = 10.dp, vertical = 5.dp))
                    }
                    if (Settings.showZoom) ZoomChip()
                    PageChip(Modifier)
                }
                if (dropHint) Box(Modifier.fillMaxSize().padding(24.dp).border(3.dp, Ui.Red, RoundedCornerShape(24.dp)), contentAlignment = Alignment.Center) {
                    Text(Voice.cased("Hier ablegen: Bild oder PDF"), color = Ui.OnAccent, fontSize = 16.sp, fontFamily = Voice.labelFont, fontWeight = FontWeight.Bold,
                        modifier = Modifier.clip(RoundedCornerShape(16.dp)).background(Ui.Red).padding(horizontal = 18.dp, vertical = 10.dp))
                }
                undoLabel?.let { label ->
                    Row(
                        Modifier.align(Alignment.BottomCenter).padding(bottom = 132.dp).bar(16f).padding(start = 18.dp, end = 6.dp, top = 6.dp, bottom = 6.dp),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        Text(label, color = Ui.Chalk, fontSize = 14.sp, fontFamily = Type.Sans)
                        Spacer(Modifier.width(14.dp))
                        Btn("Rückgängig", BtnKind.PRIMARY) {
                            undoLabel = null
                            model?.undoDelete { ok -> if (!ok) flash("Nichts zum Rückgängigmachen") else undoSheet?.let { view.showPage(it) } }
                        }
                    }
                }
                status?.let {
                    Text(it, color = Ui.Chalk, fontSize = 13.sp, fontFamily = Voice.metaFont,
                        modifier = Modifier.align(Alignment.BottomCenter).padding(bottom = 80.dp).clip(RoundedCornerShape(14.dp)).background(Ui.Bar)
                            .border(1.dp, Ui.Border, RoundedCornerShape(14.dp)).padding(horizontal = 14.dp, vertical = 8.dp))
                }
            }
            when (val m = menu) {
                "add" -> Dropdown("add") { AddMenu() }
                "export" -> ExportDialog()
                "pen" -> Dropdown("pen") { PenMenu() }
                "zoom" -> Dropdown("zoom") { ZoomMenu() }
                "page" -> Dropdown("page") { PageMenu() }
                "select" -> Dropdown("select") { SelectMenu() }
                "group" -> Dropdown("group") { GroupMenu() }
                "live" -> Dropdown("live") { LiveMenu() }
                "eraser" -> Dropdown("eraser") { EraserMenu() }
                "hold" -> Dropdown("hold", compact = true) { HoldMenu() }
                "guests" -> Dropdown("guests") { GuestsMenu() }
                "split" -> Dropdown("split", width = 440.dp) { SplitMenu() }
                null -> {}
                else -> when {
                    m.startsWith("width:") -> m.removePrefix("width:").toIntOrNull()?.let { i -> Dropdown("width$i") { WidthMenu(i) } }
                    m.startsWith("color:") -> m.removePrefix("color:").toIntOrNull()?.let { i -> Dropdown("color$i") { ColorMenu(i) } }
                }
            }
            renaming?.let { (node, old) -> RenameDialog(node, old) }
            headingFor?.let { (sheetId, ids) -> HeadingDialog(sheetId, ids) }
            pagePicker?.let { PagePicker(it) }
            if (guestsOpen) GuestsDialog()
            sizeWarning?.let { (o, apply) -> SizeWarning(o, apply) }
        }

        /** A card below [anchorKey]'s control; a tap outside closes it. */
        @Composable
        fun Dropdown(anchorKey: String, compact: Boolean = false, width: androidx.compose.ui.unit.Dp? = null, content: @Composable () -> Unit) {
            val a = anchors[anchorKey] ?: return
            val density = androidx.compose.ui.platform.LocalDensity.current
            val gap = with(density) { 10.dp.roundToPx() }
            val provider = remember(a) {
                object : androidx.compose.ui.window.PopupPositionProvider {
                    override fun calculatePosition(anchorBounds: androidx.compose.ui.unit.IntRect, windowSize: androidx.compose.ui.unit.IntSize,
                        layoutDirection: androidx.compose.ui.unit.LayoutDirection, popupContentSize: androidx.compose.ui.unit.IntSize): androidx.compose.ui.unit.IntOffset {
                        val x = (a.center.x - popupContentSize.width / 2f).toInt().coerceIn(gap, (windowSize.width - popupContentSize.width - gap).coerceAtLeast(gap))
                        val below = a.bottom + gap
                        val y = if (below + popupContentSize.height + gap > windowSize.height && a.top - gap - popupContentSize.height >= 0)
                            (a.top - gap - popupContentSize.height).toInt() // no room below (bottom chips): open upwards
                        else below.toInt().coerceAtMost((windowSize.height - popupContentSize.height - gap).coerceAtLeast(0))
                        return androidx.compose.ui.unit.IntOffset(x, y)
                    }
                }
            }
            androidx.compose.ui.window.Popup(popupPositionProvider = provider, onDismissRequest = { menu = null; pageFor = null },
                properties = androidx.compose.ui.window.PopupProperties(focusable = true)) {
                val st = Ui.theme.style
                val shape = RoundedCornerShape(minOf(st.panel, 22f).dp)
                N0teType {
                    Column(
                        Modifier.then(if (width != null) Modifier.width(width) else if (compact) Modifier else Modifier.widthIn(min = 220.dp, max = 400.dp))
                            .then(if (st.lift) Modifier.shadow(12.dp, shape) else Modifier)
                            .clip(shape).background(Ui.Bar)
                            .then(if (Voice.line.alpha > 0f) Modifier.border(1.dp, if (st.fill == Fill.RULE) Ui.Chalk.copy(alpha = 0.4f) else Ui.Border, shape) else Modifier)
                            .padding(vertical = if (compact) 6.dp else 6.dp)
                    ) { content() }
                }
            }
        }

        /** A group label in a menu, in the design's voice (written in sentence case; mono designs shout). */
        @Composable
        fun Section(text: String) = Caption(text, Modifier.padding(start = 16.dp, end = 16.dp, top = 8.dp, bottom = 2.dp))

        /**
         * A menu entry, kept light: the icon and a short name on one line (what it does should be
         * clear from the name; [sub] is no longer shown), a state at the end if it has one.
         */
        @Composable
        fun Item(g: Glyph, title: String, @Suppress("UNUSED_PARAMETER") sub: String? = null, trailing: String? = null, onClick: () -> Unit) {
            Row(
                Modifier.fillMaxWidth().clickable(onClick = onClick).padding(horizontal = 16.dp, vertical = 9.dp),
                verticalAlignment = Alignment.CenterVertically
            ) {
                GlyphIcon(g, Ui.Chalk.copy(alpha = 0.85f), 19.dp)
                Text(title, color = Ui.Chalk, fontSize = 14.sp, maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f).padding(start = 14.dp))
                if (trailing != null) Text(trailing, color = Ui.Muted, fontSize = 12.sp, fontFamily = Voice.metaFont, modifier = Modifier.padding(start = 10.dp))
            }
        }

        @Composable
        fun MenuDivider() {
            Box(Modifier.padding(vertical = 6.dp, horizontal = 16.dp).fillMaxWidth().height(1.dp).background(Ui.Border))
        }

        /** A square-ish tile: icon over a short label (compact menus). */
        @Composable
        fun Tile(g: Glyph, label: String, modifier: Modifier, enabled: Boolean = true, onClick: () -> Unit) {
            val st = Ui.theme.style
            Column(
                modifier.clip(RoundedCornerShape(minOf(st.button, 12f).dp)).background(if (st.fill == Fill.TONAL) Voice.tonal else Ui.Elevated)
                    .clickable(enabled = enabled, onClick = onClick).padding(vertical = 10.dp, horizontal = 4.dp).alpha(if (enabled) 1f else 0.4f),
                horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(5.dp),
            ) {
                GlyphIcon(g, Ui.Chalk, 22.dp)
                Text(label, color = Ui.Chalk, fontSize = 12.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
            }
        }

        /** Tiles three to a row. */
        @Composable
        fun TileRows(tiles: List<Triple<Glyph, String, () -> Unit>>) {
            tiles.chunked(3).forEach { row ->
                Row(Modifier.padding(start = 12.dp, end = 12.dp, bottom = 8.dp), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    row.forEach { (g, l, f) -> Tile(g, l, Modifier.weight(1f), onClick = f) }
                    repeat(3 - row.size) { Spacer(Modifier.weight(1f)) }
                }
            }
        }

        @Composable
        fun AddMenu() {
            Column(Modifier.width(360.dp)) {
                Section("Neue Seite")
                Row(Modifier.padding(start = 12.dp, end = 12.dp, bottom = 8.dp), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    Tile(Glyph.DOWN, "Darunter", Modifier.weight(1f)) { addPage("below") }
                    if (Settings.sidePages) {
                        Tile(Glyph.LEFT, "Links", Modifier.weight(1f)) { addPage("left") }
                        Tile(Glyph.RIGHT, "Rechts", Modifier.weight(1f)) { addPage("right") }
                    }
                }
                // Other kinds of page, then things to put on a page.
                val kinds = buildList<Triple<Glyph, String, () -> Unit>> {
                    add(Triple(Glyph.TEXT, "Textseite") { addPage("text") })
                    if (Settings.boards) add(Triple(Glyph.BOARD, "Board") { addPage("board") })
                    if (Settings.groups) add(Triple(Glyph.GROUP, "Gruppe") { addPage("group") })
                }
                TileRows(kinds)
                Section("Einfügen")
                val clipImage = N0te.clip?.image != null
                val sysImage = remember { clipHasImage() }
                TileRows(buildList {
                    add(Triple(Glyph.PAGE, "Bild") { menu = null; pickFor = this@Pane; pickImage.launch("image/*") })
                    add(Triple(Glyph.PDF, "PDF") { addPage("pdf") })
                    if (!readOnly) add(Triple(Glyph.PLOT, "Graph") {
                        menu = null
                        view.startPlotBox { id, p -> startGraph(id, p, true) }
                        flash("Koordinatensystem: ein Rechteck aufziehen (antippen: übliche Größe)")
                    })
                    if (clipImage) add(Triple(Glyph.CUT, "Ausschnitt") { menu = null; view.paste() })
                    else if (N0te.clip != null) add(Triple(Glyph.PASTE, "Einfügen") { menu = null; view.paste() })
                    if (sysImage) add(Triple(Glyph.PASTE, "Zwischenabl.") { menu = null; pasteSystemImage() })
                })
                Section("Papier für neue Seiten")
                Row(Modifier.padding(horizontal = 14.dp, vertical = 4.dp), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    PAPERS.forEach { (b, label) -> PaperChoice(b, label, newPaper == b) { newPaper = b } }
                }
            }
        }

        @Composable
        fun PaperChoice(bg: FfiBackground, label: String, selected: Boolean, onClick: () -> Unit) {
            Column(Modifier.clip(RoundedCornerShape(12.dp)).clickable(onClick = onClick).padding(4.dp), horizontalAlignment = Alignment.CenterHorizontally) {
                androidx.compose.foundation.Canvas(
                    Modifier.size(width = 44.dp, height = 58.dp).clip(RoundedCornerShape(6.dp)).background(Ui.Paper)
                        .border(2.dp, if (selected) Ui.Red else Color.Transparent, RoundedCornerShape(6.dp))
                ) {
                    val line = Color(0xFFCBD2DD); val st = 6.dp.toPx()
                    when (bg) {
                        FfiBackground.LINES -> { var y = st * 1.5f; while (y < size.height) { drawLine(line, androidx.compose.ui.geometry.Offset(0f, y), androidx.compose.ui.geometry.Offset(size.width, y), 1f); y += st } }
                        FfiBackground.GRID -> {
                            var x = st; while (x < size.width) { drawLine(line, androidx.compose.ui.geometry.Offset(x, 0f), androidx.compose.ui.geometry.Offset(x, size.height), 1f); x += st }
                            var y = st; while (y < size.height) { drawLine(line, androidx.compose.ui.geometry.Offset(0f, y), androidx.compose.ui.geometry.Offset(size.width, y), 1f); y += st }
                        }
                        FfiBackground.DOTS -> { var x = st; while (x < size.width) { var y = st; while (y < size.height) { drawCircle(line, 1.2f, androidx.compose.ui.geometry.Offset(x, y)); y += st }; x += st } }
                        FfiBackground.CORNELL -> {
                            val red = Color(0x99D71921)
                            drawLine(red, androidx.compose.ui.geometry.Offset(size.width * 0.28f, size.height * 0.1f), androidx.compose.ui.geometry.Offset(size.width * 0.28f, size.height * 0.78f), 1.5f)
                            drawLine(red, androidx.compose.ui.geometry.Offset(0f, size.height * 0.78f), androidx.compose.ui.geometry.Offset(size.width, size.height * 0.78f), 1.5f)
                        }
                        FfiBackground.BLANK, FfiBackground.TEXT -> {}
                    }
                }
                Text(label, color = if (selected) Ui.Chalk else Ui.Muted, fontSize = 10.sp, modifier = Modifier.padding(top = 4.dp))
            }
        }

        /**
         * Export and share as a dialog: the PDF's scope as tiles (whole notebook, this page, a
         * selection), then where it goes; the notebook itself to the laptop; guests.
         */
        @Composable
        fun ExportDialog() {
            var scope by remember { mutableStateOf("all") }
            fun go(toLaptop: Boolean) {
                when (scope) {
                    "all" -> exportPdf(all = true, toLaptop = toLaptop)
                    "page" -> exportPdf(all = false, toLaptop = toLaptop)
                    else -> { menu = null; model?.allRows { pagePicker = it } }
                }
            }
            androidx.compose.ui.window.Dialog(onDismissRequest = { menu = null }) {
                N0teType {
                    Column(Modifier.width(600.dp).panel().padding(24.dp), verticalArrangement = Arrangement.spacedBy(14.dp)) {
                        Row(verticalAlignment = Alignment.CenterVertically) {
                            Heading("Exportieren & teilen", 22.sp)
                            Spacer(Modifier.weight(1f))
                            Btn("Schließen", BtnKind.QUIET) { menu = null }
                        }
                        Caption("Als PDF")
                        Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                            ScopeTile("Ganzes Notizbuch", "$pageCount Seiten mit Seitenblättern", scope == "all", Modifier.weight(1f)) { scope = "all" }
                            ScopeTile("Diese Seite", if (currentPage > 0) "Seite $currentPage und ihre Seitenblätter" else "Mit Seitenblättern", scope == "page", Modifier.weight(1f)) { scope = "page" }
                            ScopeTile("Auswahl …", "Bestimmte Seiten wählen", scope == "pick", Modifier.weight(1f)) { scope = "pick" }
                        }
                        // Where side pages go in the PDF (remembered).
                        var sidesAtEnd by remember { mutableStateOf(Settings.exportSidesAtEnd) }
                        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                            Text("Seitenblätter", color = Ui.Muted, fontSize = 13.sp, modifier = Modifier.weight(1f))
                            SmallChoice("Direkt nach ihrer Seite", !sidesAtEnd) { sidesAtEnd = false; Settings.set("export_sides_at_end", false) }
                            SmallChoice("Am Ende", sidesAtEnd) { sidesAtEnd = true; Settings.set("export_sides_at_end", true) }
                        }
                        Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(10.dp, Alignment.End), verticalAlignment = Alignment.CenterVertically) {
                            // Only worth saying where the screen is dark.
                            Text(if (Ui.theme.dark) "Die Seiten bleiben weiß." else "", color = Ui.Muted, fontSize = 12.sp, modifier = Modifier.weight(1f))
                            Btn("An den Laptop") { go(toLaptop = true) }
                            Btn(if (scope == "pick") "Seiten wählen …" else "PDF teilen …", BtnKind.PRIMARY) { go(toLaptop = false) }
                        }
                        Spacer(Modifier.height(4.dp))
                        Caption("Als Notizbuch")
                        Row(verticalAlignment = Alignment.CenterVertically) {
                            Column(Modifier.weight(1f)) {
                                Text("An den Laptop schicken", color = Ui.Chalk, fontSize = 15.sp)
                                Text("Als .n0te-Datei, zum Öffnen in n0te. Mit Sync nicht nötig.", color = Ui.Muted, fontSize = 12.sp)
                            }
                            Btn("Senden") { sendNotebookToLaptop() }
                        }
                        Row(verticalAlignment = Alignment.CenterVertically) {
                            Column(Modifier.weight(1f)) {
                                Text("Gäste einladen", color = Ui.Chalk, fontSize = 15.sp)
                                Text("Ein Link, direkt von diesem Gerät, ohne Server", color = Ui.Muted, fontSize = 12.sp)
                            }
                            Btn("Gäste …") { menu = null; guestsOpen = true }
                        }
                    }
                }
            }
        }

        @Composable
        fun ScopeTile(title: String, sub: String, on: Boolean, modifier: Modifier, onClick: () -> Unit) {
            Column(modifier.option(on).clickable(onClick = onClick).padding(horizontal = 14.dp, vertical = 12.dp), verticalArrangement = Arrangement.spacedBy(3.dp)) {
                Text(title, color = Ui.Chalk, fontSize = 15.sp, fontFamily = Voice.titleFont, fontWeight = FontWeight.Medium)
                Text(sub, color = Ui.Muted, fontSize = 12.sp, lineHeight = 15.sp)
            }
        }

        @Composable
        fun PenMenu() {
            Column(Modifier.width(380.dp)) {
            Section("Stift")
            Row(Modifier.padding(horizontal = 12.dp), horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                PenType.entries.forEach { t ->
                    val on = Tools.pen == t && tool == NotebookView.Tool.PEN
                    val st = Ui.theme.style
                    Column(
                        Modifier.weight(1f).clip(RoundedCornerShape(minOf(st.button, 12f).dp))
                            .background(if (on) (if (st.fill == Fill.TONAL) Voice.tonal else Ui.Elevated) else Color.Transparent)
                            .then(if (on) Modifier.border(1.5.dp, Ui.Red, RoundedCornerShape(minOf(st.button, 12f).dp)) else Modifier)
                            .clickable { Tools.choosePen(t); tool = NotebookView.Tool.PEN; applyTools(); tick++; if (t != PenType.FOUNTAIN) menu = null }
                            .padding(vertical = 8.dp),
                        horizontalAlignment = Alignment.CenterHorizontally,
                    ) {
                        StrokePreview(t, Tools.inks[colorIdx], Tools.widths[widthIdx], Modifier.size(width = 70.dp, height = 26.dp))
                        Text(if (t == PenType.BALLPOINT) "Kugelschr." else t.label, color = Ui.Chalk, fontSize = 12.sp, maxLines = 1, modifier = Modifier.padding(top = 4.dp))
                    }
                }
            }
            if (Tools.pen == PenType.FOUNTAIN) FountainOptions()
            Spacer(Modifier.height(4.dp))
            }
        }

        /** The fountain pen's options, right in the pen menu, with a live sample (compact). */
        @Composable
        fun FountainOptions() {
            var ver by remember { mutableIntStateOf(0) }
            var sens by remember { mutableStateOf(Tools.fountainSensitivity) }
            var speed by remember { mutableStateOf(Tools.fountainSpeed) }
            var nib by remember { mutableStateOf(Tools.fountainNib) }
            var taper by remember { mutableStateOf(Tools.fountainTaper) }
            MenuDivider()
            Section("Füller")
            PenSample(PenType.FOUNTAIN, Tools.inks[colorIdx], Tools.widths[widthIdx], Modifier.padding(horizontal = 16.dp, vertical = 2.dp).fillMaxWidth().height(48.dp), version = ver)
            @Composable fun Opt(label: String, v: Float, range: ClosedFloatingPointRange<Float>, set: (Float) -> Unit) {
                Row(Modifier.padding(horizontal = 16.dp).height(40.dp), verticalAlignment = Alignment.CenterVertically) {
                    Text(label, color = Ui.Chalk, fontSize = 13.sp, modifier = Modifier.width(60.dp))
                    N0Slider(v, set, range, Modifier.weight(1f))
                }
            }
            Opt("Druck", sens, 0.2f..0.95f) { sens = it; Tools.setFountain(sensitivity = it); ver++ }
            Opt("Tempo", speed, 0f..1f) { speed = it; Tools.setFountain(speed = it); ver++ }
            Row(Modifier.padding(start = 16.dp, end = 12.dp, top = 4.dp, bottom = 6.dp), verticalAlignment = Alignment.CenterVertically) {
                Text("Feder", color = Ui.Chalk, fontSize = 13.sp, modifier = Modifier.width(60.dp))
                Row(Modifier.weight(1f), horizontalArrangement = Arrangement.spacedBy(5.dp)) {
                    listOf("round" to "Rund", "oblique" to "Schräg", "flat" to "Breit").forEach { (v, l) ->
                        SmallChoice(l, nib == v) { nib = v; Tools.setFountain(nib = v); ver++ }
                    }
                }
            }
            Row(Modifier.fillMaxWidth().clickable { taper = !taper; Tools.setFountain(taper = taper); ver++ }.padding(start = 16.dp, end = 16.dp, top = 2.dp, bottom = 8.dp),
                verticalAlignment = Alignment.CenterVertically) {
                Text("Spitz beginnen und enden", color = Ui.Chalk, fontSize = 13.sp, modifier = Modifier.weight(1f))
                dev.n0bridge.n0te.Switch(taper)
            }
        }

        @Composable
        fun StrokePreview(t: PenType, color: Int, width: Float, modifier: Modifier) = PenSample(t, color, width, modifier, t.inkAlpha())

        @Composable
        fun WidthMenu(i: Int) {
            var w by remember { mutableStateOf(Tools.widths[i]) }
            Section("Strichstärke ${i + 1}")
            StrokePreview(Tools.pen, Tools.inks[colorIdx], w, Modifier.padding(horizontal = 16.dp, vertical = 6.dp).fillMaxWidth().height(44.dp))
            Row(Modifier.padding(horizontal = 16.dp), verticalAlignment = Alignment.CenterVertically) {
                N0Slider(w, { w = it; Tools.setWidth(i, it); widthIdx = i; applyTools(); tick++ }, Tools.MIN_W..Tools.MAX_W, Modifier.weight(1f))
                Text(penMm(w), color = Ui.Chalk, fontSize = 12.sp, fontFamily = Voice.metaFont, modifier = Modifier.width(72.dp).padding(start = 10.dp))
            }
        }

        /**
         * Colour slot [i]: a 2D picker (saturation × brightness) with a hue bar, the pipette, recent
         * colours, presets and the palette switcher.
         */
        @Composable
        fun ColorMenu(i: Int) {
            @Suppress("UNUSED_VARIABLE") val t = tick
            val old = Tools.inks[i]
            val hsv = remember(i) { FloatArray(3).also { android.graphics.Color.colorToHSV(old, it) } }
            var h by remember(i) { mutableStateOf(hsv[0]) }
            var sat by remember(i) { mutableStateOf(hsv[1]) }
            var v by remember(i) { mutableStateOf(hsv[2]) }
            val cur = android.graphics.Color.HSVToColor(floatArrayOf(h, sat, v))
            val pure = Color(android.graphics.Color.HSVToColor(floatArrayOf(h, 1f, 1f)))
            Column(Modifier.width(340.dp)) {
                Section("Farbe ${i + 1}")
                // Saturation (x) and brightness (y) for the hue below.
                androidx.compose.foundation.Canvas(
                    Modifier.padding(horizontal = 16.dp, vertical = 4.dp).fillMaxWidth().height(170.dp).clip(RoundedCornerShape(12.dp))
                        .pointerInput(i) {
                            fun set(o: Offset) { sat = (o.x / size.width).coerceIn(0f, 1f); v = 1f - (o.y / size.height).coerceIn(0f, 1f) }
                            detectTapGestures { set(it) }
                        }
                        .pointerInput(i) {
                            fun set(o: Offset) { sat = (o.x / size.width).coerceIn(0f, 1f); v = 1f - (o.y / size.height).coerceIn(0f, 1f) }
                            detectDragGestures(onDragStart = { set(it) }) { c, _ -> set(c.position) }
                        }
                ) {
                    drawRect(Brush.horizontalGradient(listOf(Color.White, pure)))
                    drawRect(Brush.verticalGradient(listOf(Color.Transparent, Color.Black)))
                    val o = Offset(sat * size.width, (1f - v) * size.height)
                    drawCircle(Color.White, 11.dp.toPx(), o, style = androidx.compose.ui.graphics.drawscope.Stroke(2.5.dp.toPx()))
                    drawCircle(Color(cur), 8.dp.toPx(), o)
                }
                // Hue.
                androidx.compose.foundation.Canvas(
                    Modifier.padding(horizontal = 16.dp, vertical = 8.dp).fillMaxWidth().height(26.dp).clip(RoundedCornerShape(13.dp))
                        .pointerInput(i) { detectTapGestures { h = (it.x / size.width).coerceIn(0f, 1f) * 360f } }
                        .pointerInput(i) { detectDragGestures(onDragStart = { h = (it.x / size.width).coerceIn(0f, 1f) * 360f }) { c, _ -> h = (c.position.x / size.width).coerceIn(0f, 1f) * 360f } }
                ) {
                    drawRect(Brush.horizontalGradient((0..6).map { Color(android.graphics.Color.HSVToColor(floatArrayOf(it * 60f, 1f, 1f))) }))
                    val x = (h / 360f) * size.width
                    drawCircle(Color.White, size.height / 2 - 1.dp.toPx(), Offset(x.coerceIn(size.height / 2, size.width - size.height / 2), size.height / 2), style = androidx.compose.ui.graphics.drawscope.Stroke(2.5.dp.toPx()))
                }
                // Old → new, hex, apply.
                Row(Modifier.padding(horizontal = 16.dp, vertical = 6.dp), verticalAlignment = Alignment.CenterVertically) {
                    Box(Modifier.size(34.dp).clip(CircleShape).background(Color(old)).border(1.dp, Ui.Border, CircleShape))
                    Text("→", color = Ui.Muted, fontSize = 14.sp, modifier = Modifier.padding(horizontal = 6.dp))
                    Box(Modifier.size(34.dp).clip(CircleShape).background(Color(cur)).border(1.dp, Ui.Border, CircleShape))
                    Text("#%06X".format(cur and 0xFFFFFF), color = Ui.Chalk, fontSize = 13.sp, fontFamily = FontFamily.Monospace, modifier = Modifier.padding(start = 10.dp).weight(1f))
                    Pill("Übernehmen", primary = true) { pickColor(i, cur) }
                }
                Item(Glyph.PIPETTE, "Farbe aufnehmen", "Danach auf eine Stelle der Seite tippen") {
                    menu = null
                    status = "Tippe auf die Farbe, die du aufnehmen willst"
                    view.eyedropper = { c -> status = null; pickColor(i, c) }
                }
                if (Tools.recent.isNotEmpty()) {
                    Section("Zuletzt")
                    Row(Modifier.padding(horizontal = 16.dp, vertical = 4.dp), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        Tools.recent.take(8).forEach { c -> Swatch(c, Tools.inks[i] == c, 28.dp) { pickColor(i, c) } }
                    }
                }
                Section("Vorlagen")
                androidx.compose.foundation.layout.FlowRow(Modifier.padding(horizontal = 16.dp, vertical = 4.dp),
                    horizontalArrangement = Arrangement.spacedBy(7.dp), verticalArrangement = Arrangement.spacedBy(7.dp)) {
                    PRESETS.forEach { c -> Swatch(c, Tools.inks[i] == c, 28.dp) { pickColor(i, c) } }
                }
                MenuDivider()
                PaletteSwitcher()
            }
        }

        /** "‹ Pastell ›" with the palette's colours; swipe or tap the arrows to switch palettes. */
        @Composable
        fun androidx.compose.foundation.layout.ColumnScope.PaletteSwitcher() {
            @Suppress("UNUSED_VARIABLE") val t = tick
            Section("Palette · wischen zum Wechseln")
            Row(
                Modifier.padding(horizontal = 10.dp, vertical = 4.dp).fillMaxWidth().paletteSwipe(),
                verticalAlignment = Alignment.CenterVertically
            ) {
                Box(Modifier.size(34.dp).clip(CircleShape).clickable { switchPalette(-1) }, contentAlignment = Alignment.Center) { GlyphIcon(Glyph.LEFT, Ui.Chalk, 18.dp) }
                Column(Modifier.weight(1f), horizontalAlignment = Alignment.CenterHorizontally) {
                    Text(Palettes.names[Tools.palette], color = Ui.Chalk, fontSize = 13.sp, fontWeight = FontWeight.Medium)
                    Row(Modifier.padding(top = 4.dp), horizontalArrangement = Arrangement.spacedBy(5.dp)) {
                        Tools.inks.forEach { c -> Box(Modifier.size(16.dp).clip(CircleShape).background(Color(c))) }
                    }
                    Row(Modifier.padding(top = 5.dp), horizontalArrangement = Arrangement.spacedBy(4.dp)) {
                        Palettes.names.indices.forEach { k -> Box(Modifier.size(5.dp).clip(CircleShape).background(if (k == Tools.palette) Ui.Red else Ui.Border)) }
                    }
                }
                Box(Modifier.size(34.dp).clip(CircleShape).clickable { switchPalette(1) }, contentAlignment = Alignment.Center) { GlyphIcon(Glyph.RIGHT, Ui.Chalk, 18.dp) }
            }
            Text("Palette zurücksetzen", color = Ui.Muted, fontSize = 11.sp,
                modifier = Modifier.align(Alignment.CenterHorizontally).clip(RoundedCornerShape(8.dp)).clickable { Tools.resetPalette(); tick++; applyTools() }.padding(horizontal = 10.dp, vertical = 6.dp))
        }

        @Composable
        fun Swatch(c: Int, selected: Boolean, size: androidx.compose.ui.unit.Dp, onClick: () -> Unit) {
            Box(Modifier.size(size).clip(CircleShape).background(Color(c)).border(2.dp, if (selected) Ui.Chalk else Ui.Border.copy(alpha = 0.4f), CircleShape).clickable(onClick = onClick))
        }

        fun slideDir(dir: Int): androidx.compose.animation.ContentTransform {
            val spec = androidx.compose.animation.core.tween<androidx.compose.ui.unit.IntOffset>(220)
            return (androidx.compose.animation.slideInHorizontally(spec) { w -> w * dir } + androidx.compose.animation.fadeIn(androidx.compose.animation.core.tween(220))) togetherWith
                (androidx.compose.animation.slideOutHorizontally(spec) { w -> -w * dir } + androidx.compose.animation.fadeOut(androidx.compose.animation.core.tween(160)))
        }

        /**
         * The eraser: tap selects it (tap again or hold: its menu); a swipe sideways switches between
         * whole strokes and area, the icon sliding over like the colour palettes.
         */
        @OptIn(androidx.compose.foundation.ExperimentalFoundationApi::class)
        @Composable
        fun EraserBtn() {
            // One state for the button and the eraser menu (each had its own: the icon kept the old mode).
            var area by this@NotebookActivity::eraserArea
            val selected = tool == NotebookView.Tool.ERASER
            Box(
                Modifier.anchor("eraser").size(width = 44.dp, height = 38.dp).clip(CircleShape).background(if (selected) Ui.Chalk else Color.Transparent)
                    .pointerInput(Unit) {
                        var acc = 0f
                        val step = 28.dp.toPx()
                        detectHorizontalDragGestures(onDragStart = { acc = 0f }) { change, dx ->
                            acc += dx
                            if (kotlin.math.abs(acc) >= step) {
                                change.consume()
                                eraserDir = if (acc < 0) 1 else -1
                                area = !area
                                Settings.set("eraser_mode", if (area) "area" else "stroke")
                                tool = NotebookView.Tool.ERASER; applyTools()
                                status = if (area) "Radierer: nur Bereich" else "Radierer: ganze Striche"
                                N0te.main.removeCallbacks(clearStatus)
                                N0te.main.postDelayed(clearStatus, 1200)
                                acc = 0f
                            }
                        }
                    }
                    .toolTap(onLongClick = { menu = "eraser" }) {
                        if (selected) menu = "eraser" else { tool = NotebookView.Tool.ERASER; applyTools() }
                    },
                contentAlignment = Alignment.Center,
            ) {
                androidx.compose.animation.AnimatedContent(targetState = area, transitionSpec = { slideDir(eraserDir) }, label = "eraser") { a ->
                    Box(contentAlignment = Alignment.Center) {
                        GlyphIcon(Glyph.ERASER, tint = if (selected) Ui.Black else Ui.Chalk)
                        // Area mode: a small dotted patch at the eraser's foot.
                        if (a) Box(Modifier.align(Alignment.BottomEnd).offset(x = 7.dp, y = 2.dp).size(9.dp).clip(CircleShape)
                            .border(1.5.dp, if (selected) Ui.Black else Ui.Red, CircleShape))
                    }
                }
            }
        }

        /**
         * The selection tool: free lasso, box, or screenshot of a part of the page. Tap selects it
         * (tap again or hold: the mode menu); a swipe sideways steps through the modes, the icon
         * sliding like the eraser's and the palettes.
         */
        @OptIn(androidx.compose.foundation.ExperimentalFoundationApi::class)
        @Composable
        fun SelectBtn() {
            val mode = selectMode
            val modes = SELECT_MODES
            val names = mapOf("free" to "Auswahl: Lasso", "rect" to "Auswahl: Rechteck", "snip" to "Ausschnitt: Bereich aufziehen, wird kopiert",
                "space" to "Platz einfügen: an einer Stelle nach unten oder zur Seite ziehen")
            val selected = tool == NotebookView.Tool.LASSO
            fun set(m: String, dir: Int) {
                selectDir = dir; selectMode = m; Settings.set("select_mode", m)
                tool = NotebookView.Tool.LASSO; applyTools()
                status = names[m]; N0te.main.removeCallbacks(clearStatus); N0te.main.postDelayed(clearStatus, 1500)
            }
            Box(
                Modifier.anchor("select").size(width = 44.dp, height = 38.dp).clip(CircleShape).background(if (selected) Ui.Chalk else Color.Transparent)
                    .pointerInput(Unit) {
                        var acc = 0f
                        val step = 28.dp.toPx()
                        detectHorizontalDragGestures(onDragStart = { acc = 0f }) { change, dx ->
                            acc += dx
                            if (kotlin.math.abs(acc) >= step) {
                                change.consume()
                                val d = if (acc < 0) 1 else -1
                                set(modes[(modes.indexOf(Settings.selectMode) + d + modes.size) % modes.size], d)
                                acc = 0f
                            }
                        }
                    }
                    .toolTap(onLongClick = { menu = "select" }) {
                        if (selected) menu = "select" else { tool = NotebookView.Tool.LASSO; applyTools() }
                    },
                contentAlignment = Alignment.Center,
            ) {
                androidx.compose.animation.AnimatedContent(targetState = mode, transitionSpec = { slideDir(selectDir) }, label = "select") { m ->
                    val tint = if (selected) Ui.Black else Ui.Chalk
                    when (m) {
                        "rect" -> androidx.compose.foundation.Canvas(Modifier.size(20.dp)) {
                            drawRect(tint, topLeft = androidx.compose.ui.geometry.Offset(2.dp.toPx(), 3.dp.toPx()), size = androidx.compose.ui.geometry.Size(16.dp.toPx(), 14.dp.toPx()),
                                style = androidx.compose.ui.graphics.drawscope.Stroke(1.7.dp.toPx(), pathEffect = androidx.compose.ui.graphics.PathEffect.dashPathEffect(floatArrayOf(4.dp.toPx(), 2.5.dp.toPx()))))
                        }
                        "snip" -> GlyphIcon(Glyph.CUT, tint)
                        "space" -> GlyphIcon(Glyph.SPACE, tint)
                        else -> GlyphIcon(Glyph.LASSO, tint)
                    }
                }
            }
        }

        @Composable
        fun SelectMenu() {
            Section("Auswahl")
            listOf(
                Triple("free", "Lasso", "Frei um Striche und Bilder ziehen"),
                Triple("rect", "Rechteck", "Ein Rechteck aufziehen"),
                Triple("snip", "Ausschnitt", "Bereich aufziehen: wird in voller Auflösung kopiert (auch PDF), dann überall einfügen"),
                Triple("space", "Platz einfügen", "Nach unten ziehen: alles darunter rückt nach, die Seite wird länger (auch ein PDF teilt sich). Zur Seite: alles rechts davon rückt nach, die Seite wird breiter. Zurück nimmt Platz weg"),
            ).forEach { (m, t, sub) ->
                Item(when (m) { "snip" -> Glyph.CUT; "rect" -> Glyph.PAGE; "space" -> Glyph.SPACE; else -> Glyph.LASSO }, t, sub, trailing = if (Settings.selectMode == m) "✓" else null) {
                    selectDir = if (SELECT_MODES.indexOf(m) >= SELECT_MODES.indexOf(selectMode)) 1 else -1
                    selectMode = m; Settings.set("select_mode", m); tool = NotebookView.Tool.LASSO; applyTools(); menu = null; tick++
                }
            }
        }

        fun switchPalette(d: Int) {
            paletteDir = if (d > 0) 1 else -1
            Tools.choosePalette(Tools.palette + d)
            applyTools()
            tick++
            status = "Palette: ${Palettes.names[Tools.palette]}"
            N0te.main.removeCallbacks(clearStatus)
            N0te.main.postDelayed(clearStatus, 1200)
        }

        val clearStatus = Runnable { status = null }

        /** A horizontal swipe switches the colour palette (taps still reach the swatches). */
        fun Modifier.paletteSwipe(): Modifier = this.pointerInput(Unit) {
            var acc = 0f
            val step = 48.dp.toPx()
            detectHorizontalDragGestures(onDragStart = { acc = 0f }) { change, dx ->
                acc += dx
                if (kotlin.math.abs(acc) >= step) {
                    change.consume()
                    switchPalette(if (acc < 0) 1 else -1)
                    acc = 0f
                }
            }
        }

        /** "125 %" — tap: lock on/off (the mode used last), hold: zoom menu with both lock modes. */
        @Composable
        fun ZoomChip() {
            Row(
                Modifier.anchor("zoom").clip(RoundedCornerShape(12.dp)).background(if (zoomLocked) Ui.Chalk else Ui.Chip)
                    .toolTap(onLongClick = { menu = "zoom" }) { toggleZoomLock() }
                    .padding(horizontal = 10.dp, vertical = 5.dp),
                verticalAlignment = Alignment.CenterVertically
            ) {
                if (!inBoard) GlyphIcon(if (zoomLocked) Glyph.LOCK else Glyph.UNLOCK, if (zoomLocked) Ui.Black else Ui.Muted, 14.dp)
                Text("$zoom %", color = if (zoomLocked) Ui.Black else Ui.Muted, fontSize = 12.sp, fontFamily = Voice.metaFont, modifier = Modifier.padding(start = 4.dp))
                if (lock == NotebookView.Lock.PAGE) Text(Voice.cased("Seite"), color = Ui.Black, fontSize = 11.sp, fontFamily = Voice.metaFont, fontWeight = FontWeight.Bold, modifier = Modifier.padding(start = 6.dp))
            }
        }

        fun toggleZoomLock() {
            if (inBoard) { menu = "zoom"; return }
            applyLock(if (zoomLocked) NotebookView.Lock.NONE else if (Settings.lockMode == "page") NotebookView.Lock.PAGE else NotebookView.Lock.ZOOM)
        }

        fun applyLock(l: NotebookView.Lock) {
            lock = l
            view.lock = l
            if (l != NotebookView.Lock.NONE) Settings.set("lock_mode", if (l == NotebookView.Lock.PAGE) "page" else "zoom")
            flash(when (l) {
                NotebookView.Lock.ZOOM -> "Zoom gesperrt — hoch/runter scrollen, seitlich ziehen wechselt das Blatt"
                NotebookView.Lock.PAGE -> "Auf der Seite — zoomen geht, seitlich ziehen wechselt das Blatt"
                NotebookView.Lock.NONE -> "Frei"
            })
        }

        @Composable
        fun ZoomMenu() {
            // Its switches are settings (not state): read the counter they bump, so a tap redraws.
            @Suppress("UNUSED_VARIABLE") val ticked = tick
            Section("Zoom")
            if (!inBoard) {
                LockItem(NotebookView.Lock.ZOOM, "Zoom sperren", "Zoom bleibt; nur hoch/runter; seitlich ziehen wechselt das Blatt")
                LockItem(NotebookView.Lock.PAGE, "Auf der Seite bleiben", "Zoomen geht; seitlich nur innerhalb der Seite, ziehen wechselt das Blatt")
                if (zoomLocked) Item(Glyph.UNLOCK, "Entsperren", "Frei verschieben und zoomen") { menu = null; applyLock(NotebookView.Lock.NONE) }
                MenuDivider()
                Item(Glyph.PAGE, "Ganze Seite", "Seite passt auf den Bildschirm") { menu = null; view.currentSheet()?.let { view.fitSheet(it, whole = true) } }
                Item(Glyph.RIGHT, "Seitenbreite", "Seite füllt die Breite") { menu = null; view.currentSheet()?.let { view.fitSheet(it, whole = false) } }
                MenuDivider()
            }
            if (!inBoard) {
                Section("Standard beim Öffnen")
                Row(Modifier.padding(horizontal = 14.dp, vertical = 6.dp), horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                    val d = Settings.pageScale
                    listOf("width" to "Breite", "page" to "Ganz", "$zoom" to "jetzt $zoom %").forEach { (v, label) ->
                        val on = d == v || (v == "$zoom" && d.toIntOrNull() != null && d == "$zoom")
                        Text(label, color = if (on) Ui.Black else Ui.Chalk, fontSize = 12.sp, fontFamily = Voice.metaFont,
                            modifier = Modifier.clip(RoundedCornerShape(10.dp)).background(if (on) Ui.Chalk else Ui.Elevated)
                                .clickable { Settings.set("page_scale", v); tick++; flash("Standard: " + when (v) { "width" -> "Seitenbreite"; "page" -> "ganze Seite"; else -> "$v %" }) }
                                .padding(horizontal = 10.dp, vertical = 8.dp))
                    }
                    if (d.toIntOrNull() != null && d != "$zoom") Text("($d %)", color = Ui.Muted, fontSize = 12.sp, fontFamily = Voice.metaFont, modifier = Modifier.padding(start = 4.dp, top = 8.dp))
                }
                MenuDivider()
            }
            Row(Modifier.padding(horizontal = 14.dp, vertical = 6.dp), horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                listOf(50, 75, 100, 150, 200, 300).forEach { z ->
                    Text("$z", color = if (zoom == z) Ui.Black else Ui.Chalk, fontSize = 12.sp, fontFamily = Voice.metaFont,
                        modifier = Modifier.clip(RoundedCornerShape(10.dp)).background(if (zoom == z) Ui.Chalk else Ui.Elevated)
                            .clickable { menu = null; view.zoomTo(z) }.padding(horizontal = 10.dp, vertical = 8.dp))
                }
            }
        }

        @Composable
        fun LockItem(l: NotebookView.Lock, title: String, sub: String) {
            Row(Modifier.fillMaxWidth().clickable { menu = null; applyLock(if (lock == l) NotebookView.Lock.NONE else l) }.padding(horizontal = 16.dp, vertical = 10.dp), verticalAlignment = Alignment.CenterVertically) {
                Box(Modifier.size(34.dp).clip(RoundedCornerShape(10.dp)).background(if (lock == l) Ui.Chalk else Ui.Elevated), contentAlignment = Alignment.Center) {
                    GlyphIcon(Glyph.LOCK, if (lock == l) Ui.Black else Ui.Chalk, 20.dp)
                }
                Column(Modifier.weight(1f).padding(start = 12.dp)) {
                    Text(title, color = Ui.Chalk, fontSize = 14.sp, fontWeight = FontWeight.Medium)
                    Text(sub, color = Ui.Muted, fontSize = 11.sp)
                }
                if (lock == l) GlyphIcon(Glyph.CHECK, Ui.Red, 18.dp)
            }
        }

        /** A page's sheet and its side pages (left ones first). */
        fun rowIds(r: FfiOutlineRow.Sheet): List<String> = r.left.reversed().map { it.sheetId } + r.sheetId + r.right.map { it.sheetId }

        /** Pick pages to share: all pages, also those in folded groups; side pages each on their own. */
        @Composable
        fun PagePicker(rows: List<FfiOutlineRow>) {
            val sheets = remember(rows) { rows.filterIsInstance<FfiOutlineRow.Sheet>() }
            // Sheet ids: main pages and side pages each on their own (a single side page works too).
            val picked = remember(rows) {
                androidx.compose.runtime.mutableStateListOf<String>().also { l ->
                    view.currentSheet()?.let { cur -> sheets.firstOrNull { it.node == cur.node || it.left.any { s -> s.node == cur.node } || it.right.any { s -> s.node == cur.node } }?.let { l += rowIds(it) } }
                }
            }
            Dialog(onDismissRequest = { pagePicker = null }) {
                Column(Modifier.width(460.dp).panel().padding(vertical = 14.dp)) {
                    Row(Modifier.padding(horizontal = 16.dp), verticalAlignment = Alignment.CenterVertically) {
                        Caption("Seiten teilen", Modifier.weight(1f))
                        Text(Voice.cased("Alle"), color = Ui.Chalk, fontSize = 12.sp, fontFamily = Voice.labelFont,
                            modifier = Modifier.clip(RoundedCornerShape(8.dp)).clickable { picked.clear(); picked.addAll(sheets.flatMap { rowIds(it) }) }.padding(8.dp))
                        Text(Voice.cased("Keine"), color = Ui.Chalk, fontSize = 12.sp, fontFamily = Voice.labelFont,
                            modifier = Modifier.clip(RoundedCornerShape(8.dp)).clickable { picked.clear() }.padding(8.dp))
                    }
                    var number = 0
                    val numbered = rows.map { r -> if (r is FfiOutlineRow.Sheet) { number++; r to number } else r to 0 }
                    LazyColumn(Modifier.fillMaxWidth().height(420.dp).padding(vertical = 6.dp)) {
                        items(numbered, key = { (r, _) -> when (r) { is FfiOutlineRow.Group -> r.node; is FfiOutlineRow.Sheet -> r.node } }) { (r, n) ->
                            when (r) {
                                is FfiOutlineRow.Group -> {
                                    val inside = groupSheets(rows, r).flatMap { rowIds(it) }
                                    val all = inside.isNotEmpty() && inside.all { it in picked }
                                    Row(
                                        Modifier.fillMaxWidth().clickable { if (all) picked.removeAll(inside) else inside.forEach { if (it !in picked) picked += it } }
                                            .padding(start = (16 + r.depth.toInt() * 14).dp, end = 16.dp, top = 9.dp, bottom = 5.dp),
                                        verticalAlignment = Alignment.CenterVertically
                                    ) {
                                        CheckBox(all)
                                        Text(r.title, color = Ui.Chalk, fontSize = 13.sp, fontWeight = FontWeight.SemiBold, maxLines = 1, overflow = TextOverflow.Ellipsis,
                                            modifier = Modifier.weight(1f).padding(start = 10.dp))
                                        Text("${inside.size}", color = Ui.Muted, fontSize = 12.sp, fontFamily = Voice.metaFont)
                                    }
                                }
                                is FfiOutlineRow.Sheet -> Column {
                                    val on = r.sheetId in picked
                                    // The page with its side pages; each side page below on its own.
                                    Row(
                                        Modifier.fillMaxWidth().clickable { val ids = rowIds(r); if (on) picked.removeAll(ids) else ids.forEach { if (it !in picked) picked += it } }
                                            .padding(start = (16 + r.depth.toInt() * 14).dp, end = 16.dp, top = 7.dp, bottom = 7.dp),
                                        verticalAlignment = Alignment.CenterVertically
                                    ) {
                                        CheckBox(on)
                                        Text("%02d".format(n), color = if (on) Ui.Red else Ui.Muted, fontSize = 12.sp, fontFamily = Voice.metaFont, modifier = Modifier.padding(start = 10.dp).width(30.dp))
                                        Text(r.title.ifBlank { if (r.spec.width <= 0f) "Board" else if (r.spec.pdfBlob != null) "PDF-Seite ${r.spec.pdfPage + 1u}" else if (r.spec.background == FfiBackground.TEXT) "Textseite" else "Seite" },
                                            color = Ui.Chalk, fontSize = 13.sp, maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f))
                                    }
                                    val sides = r.left.mapIndexed { i, sd -> Triple(sd, "L${i + 1}", "links") }.reversed() + r.right.mapIndexed { i, sd -> Triple(sd, "R${i + 1}", "rechts") }
                                    for ((sd, label, where) in sides) {
                                        val sOn = sd.sheetId in picked
                                        Row(
                                            Modifier.fillMaxWidth().clickable { if (sOn) picked.remove(sd.sheetId) else picked += sd.sheetId }
                                                .padding(start = (46 + r.depth.toInt() * 14).dp, end = 16.dp, top = 5.dp, bottom = 5.dp),
                                            verticalAlignment = Alignment.CenterVertically
                                        ) {
                                            CheckBox(sOn)
                                            Text("%02d %s".format(n, label), color = if (sOn) Ui.Red else Ui.Muted, fontSize = 12.sp, fontFamily = Voice.metaFont, modifier = Modifier.padding(start = 10.dp).width(56.dp))
                                            Text("Seitenblatt $where" + when {
                                                sd.spec.width <= 0f -> " · Board"
                                                sd.spec.pdfBlob != null -> " · PDF-Seite ${sd.spec.pdfPage + 1u}"
                                                sd.spec.background == FfiBackground.TEXT -> " · Textseite"
                                                else -> ""
                                            }, color = Ui.Chalk.copy(alpha = 0.85f), fontSize = 12.5.sp, maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f))
                                        }
                                    }
                                }
                            }
                        }
                    }
                    val pickedSet = picked.toSet()
                    val total = pickedSet.size
                    Text(if (total == 0) "Keine Seite gewählt" else if (total == 1) "1 Seite" else "$total Seiten", color = Ui.Muted, fontSize = 12.sp,
                        modifier = Modifier.padding(horizontal = 16.dp, vertical = 6.dp))
                    Row(Modifier.padding(horizontal = 16.dp).fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        Btn("Abbrechen", BtnKind.QUIET) { pagePicker = null }
                        Spacer(Modifier.weight(1f))
                        Pill("An den Laptop") { exportPicked(sheets, pickedSet, toLaptop = true) }
                        Pill("Teilen …", primary = true) { exportPicked(sheets, pickedSet, toLaptop = false) }
                    }
                }
            }
        }

        /** The sheets inside group [g] (any depth), from the full outline. */
        fun groupSheets(rows: List<FfiOutlineRow>, g: FfiOutlineRow.Group): List<FfiOutlineRow.Sheet> {
            val start = rows.indexOf(g)
            val out = ArrayList<FfiOutlineRow.Sheet>()
            for (k in start + 1 until rows.size) {
                val r = rows[k]
                val depth = when (r) { is FfiOutlineRow.Group -> r.depth; is FfiOutlineRow.Sheet -> r.depth }
                if (depth <= g.depth) break
                if (r is FfiOutlineRow.Sheet) out += r
            }
            return out
        }

        @Composable
        fun CheckBox(on: Boolean) {
            Box(Modifier.size(20.dp).clip(RoundedCornerShape(6.dp)).background(if (on) Ui.Red else Color.Transparent).border(1.5.dp, if (on) Ui.Red else Ui.Muted, RoundedCornerShape(6.dp)),
                contentAlignment = Alignment.Center) { if (on) GlyphIcon(Glyph.CHECK, Ui.OnAccent, 14.dp) }
        }

        fun pickColor(i: Int, c: Int) {
            Tools.setInk(i, c)
            colorIdx = i
            if (tool == NotebookView.Tool.ERASER || tool == NotebookView.Tool.LASSO) tool = NotebookView.Tool.PEN
            applyTools()
            tick++
            menu = null
        }

        @Composable
        fun Toolbar(modifier: Modifier) {
            androidx.compose.foundation.layout.BoxWithConstraints(Modifier.fillMaxWidth()) {
                val wide = maxWidth >= 1000.dp
                Column(modifier.align(Alignment.TopCenter), horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(6.dp)) {
                    val texting = textEdit != null
                    if (readOnly) {
                        // A guest who may only read: no tools, a clear note instead.
                        Bar { NavGroup(); Divider(); ReadOnlyNote() }
                    } else if (wide) {
                        Bar { NavGroup(); Divider(); if (texting) TextTools() else ToolGroup(); Divider(); HistoryGroup() }
                    } else {
                        Bar { NavGroup(); Divider(); HistoryGroup() }
                        Bar { if (texting) TextTools() else ToolGroup() }
                    }
                }
            }
        }

        @Composable
        fun Bar(content: @Composable androidx.compose.foundation.layout.RowScope.() -> Unit) {
            Row(
                Modifier.bar(24f).padding(horizontal = 8.dp, vertical = 6.dp),
                verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(2.dp)
            ) { content() }
        }

        @Composable
        fun androidx.compose.foundation.layout.RowScope.NavGroup() {
            IconBtn(Glyph.BACK) { model?.flush(); finish() }
            IconBtn(Glyph.MENU, selected = sidebar) { sidebar = !sidebar; held = null; inlineRename = null; confirmDelete = null }
            // The title takes what room is left (the tools come first).
            Text(title, color = Ui.Chalk, fontSize = 13.sp, fontWeight = FontWeight.SemiBold, maxLines = 1, overflow = TextOverflow.Ellipsis,
                modifier = Modifier.weight(1f, fill = false).widthIn(min = 24.dp, max = 140.dp).padding(horizontal = 6.dp))
            IconBtn(Glyph.SETTINGS) { openSettings() }
            // ⧉: another notebook (or this one again) beside this one.
            IconBtn(Glyph.SPLIT, selected = menu == "split", key = "split") { menu = if (menu == "split") null else "split" }
        }

        @Composable
        fun ToolGroup() {
            @Suppress("UNUSED_VARIABLE") val t = tick
            if (onTextPage) ModeSwitch()
            // Pen: tap selects it (tap again or hold for the pen type).
            IconBtn(Glyph.PEN, selected = tool == NotebookView.Tool.PEN, key = "pen",
                onLongClick = { menu = "pen" }) { if (tool == NotebookView.Tool.PEN) menu = "pen" else { tool = NotebookView.Tool.PEN; applyTools() } }
            if (Settings.toolMarker) IconBtn(Glyph.MARKER, selected = tool == NotebookView.Tool.MARKER) { tool = NotebookView.Tool.MARKER; applyTools() }
            EraserBtn()
            if (Settings.toolLasso) SelectBtn()
            if (Settings.toolLaser) IconBtn(Glyph.LASER, selected = tool == NotebookView.Tool.LASER) { tool = NotebookView.Tool.LASER; applyTools() }
            IconBtn(Glyph.TEXT, selected = tool == NotebookView.Tool.TEXT) { tool = NotebookView.Tool.TEXT; applyTools(); flash("Text: tippen schreibt, ziehen verschiebt") }
            Divider()
            // Colours: tap picks, hold changes the colour, swipe sideways switches the palette (the
            // swatches slide over to the next palette).
            androidx.compose.animation.AnimatedContent(
                targetState = Tools.palette,
                transitionSpec = { slideDir(paletteDir) },
                label = "palette",
            ) { _ ->
            Row(Modifier.paletteSwipe().height(38.dp), verticalAlignment = Alignment.CenterVertically) {
                Tools.inks.forEachIndexed { i, c ->
                    Box(Modifier.anchor("color$i").size(30.dp).clip(CircleShape)
                        .toolTap(onLongClick = { colorIdx = i; applyTools(); menu = "color:$i" }) {
                            colorIdx = i; if (tool == NotebookView.Tool.ERASER || tool == NotebookView.Tool.LASSO) tool = NotebookView.Tool.PEN; applyTools()
                        }, contentAlignment = Alignment.Center) {
                        Box(Modifier.size(if (colorIdx == i) 24.dp else 18.dp).clip(CircleShape).background(Color(c))
                            .border(2.dp, if (colorIdx == i) Ui.Chalk else Color.Transparent, CircleShape))
                    }
                }
            }
            }
            Divider()
            // Widths: tap picks, hold opens the slider.
            Tools.widths.forEachIndexed { i, w ->
                // Selected size: white disc with a black dot (clear at a glance); others: grey dots.
                val on = widthIdx == i
                Box(Modifier.anchor("width$i").size(32.dp).clip(CircleShape).background(if (on) Ui.Chalk else Color.Transparent)
                    .border(1.dp, if (on) Ui.Chalk else Color.Transparent, CircleShape)
                    .toolTap(onLongClick = { widthIdx = i; applyTools(); menu = "width:$i" }) { widthIdx = i; applyTools() },
                    contentAlignment = Alignment.Center) {
                    Box(Modifier.size((2.5f + w * 1.9f).coerceAtMost(22f).dp).clip(CircleShape).background(if (on) Ui.Black else Ui.Muted))
                }
            }
        }

        @Composable
        fun HistoryGroup() {
            IconBtn(Glyph.UNDO, enabled = canUndo) { model?.undo() }
            IconBtn(Glyph.REDO, enabled = canRedo) { model?.redo() }
            Divider()
            // Pen-only (default) vs. finger drawing.
            IconBtn(Glyph.HAND, selected = !penOnly) { penOnly = !penOnly; applyTools() }
            if (Settings.toolRuler) IconBtn(Glyph.RULER, selected = rulerOn) { rulerOn = !rulerOn; applyTools() }
            if (Settings.toolLoupe && !inBoard) IconBtn(Glyph.LOUPE, selected = loupe) { toggleLoupe() }
            if (inBoard) IconBtn(Glyph.BOARD, selected = true) { view.exitBoard() }
            else if (currentIsBoard) IconBtn(Glyph.BOARD) { view.currentSheet()?.let { view.enterBoard(it) } }
            IconBtn(Glyph.PLUS, selected = menu == "add", key = "add") { menu = if (menu == "add") null else "add" }
            IconBtn(Glyph.SHARE, selected = menu == "export", key = "export") { menu = if (menu == "export") null else "export" }
        }

        /**
         * The outline. Tap: open the page / fold the group. Hold: quick actions (rename, delete,
         * more) and drag to move — up/down between rows, sideways to go into or out of a group,
         * onto a group to put it at its end.
         */
        @Composable
        fun Sidebar(modifier: Modifier) {
            val scroll = androidx.compose.foundation.rememberScrollState()
            val density = androidx.compose.ui.platform.LocalDensity.current
            val indent = with(density) { OUT_INDENT.dp.toPx() }
            val base = with(density) { OUT_BASE.dp.toPx() }
            val dragged = dragNode
            Column(
                modifier.width(312.dp).fillMaxHeight().bar(20f).padding(vertical = 10.dp)
            ) {
                // The notebook, how many pages and groups; as a list or as pictures.
                Row(Modifier.padding(start = 16.dp, end = 10.dp, top = 4.dp, bottom = 8.dp), verticalAlignment = Alignment.CenterVertically) {
                    Column(Modifier.weight(1f)) {
                        Caption("Gliederung")
                        Text(title.ifBlank { "Ohne Titel" }, color = Ui.Chalk, fontSize = 17.sp, fontWeight = FontWeight.SemiBold, maxLines = 1, overflow = TextOverflow.Ellipsis,
                            modifier = Modifier.padding(top = 2.dp))
                        val groups = rows.count { it is FfiOutlineRow.Group }
                        Text(listOfNotNull("$pageCount ${if (pageCount == 1) "Seite" else "Seiten"}", groups.takeIf { it > 0 }?.let { "$it ${if (it == 1) "Gruppe" else "Gruppen"}" },
                            marks.size.takeIf { it > 0 }?.let { "$it gemerkt" }).joinToString(" · "), color = Ui.Muted, fontSize = 11.5.sp, modifier = Modifier.padding(top = 1.dp))
                    }
                    // List or pictures, as one switch.
                    Row(Modifier.clip(RoundedCornerShape(10.dp)).background(Ui.Black).border(1.dp, Ui.Border, RoundedCornerShape(10.dp)).padding(2.dp)) {
                        ViewPick(Glyph.LIST, !outlinePics) { outlinePics = false; Settings.set("outline_view", "list") }
                        ViewPick(Glyph.GRID, outlinePics) { outlinePics = true; Settings.set("outline_view", "pics") }
                    }
                }
                SearchField()
                if (query.isNotBlank()) { SearchResults(Modifier.weight(1f)); return@Column }
                // All pages, or only the marked ones.
                if (marks.isNotEmpty()) Row(Modifier.padding(horizontal = 12.dp, vertical = 4.dp), horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                    SmallChoice("Alle", !onlyMarked) { onlyMarked = false }
                    SmallChoice("Gemerkt · ${marks.size}", onlyMarked) { onlyMarked = true }
                }
                Box(Modifier.weight(1f).fillMaxWidth()) {
                    when {
                        onlyMarked && marks.isNotEmpty() -> MarkedPages()
                        outlinePics -> PagePictures()
                        else -> {
                            val numbered = rows.map { r -> if (r is FfiOutlineRow.Sheet) r to (model?.numberOf(r.node) ?: 0) else r to 0 }
                            val subtree = remember(rows, dragged) { subtreeOf(dragged) }
                            Column(Modifier.fillMaxWidth().verticalScroll(scroll)) {
                                numbered.forEach { (r, n) ->
                                    androidx.compose.runtime.key(nodeOf(r)) { OutlineRow(r, n, dimmed = dragMoved && nodeOf(r) in subtree && nodeOf(r) != dragged) }
                                    // The page's headings, under it.
                                    // (The first one not again when it already names an untitled page.)
                                    if (r is FfiOutlineRow.Sheet && !(dragMoved && nodeOf(r) in subtree)) heads.filter { it.node == r.node }
                                        .let { hs -> if (r.title.isBlank() && hs.firstOrNull()?.text?.isNotBlank() == true) hs.drop(1) else hs }
                                        .forEach { h -> androidx.compose.runtime.key(h.id) { HeadingRow(h, depthOf(r)) } }
                                }
                                Spacer(Modifier.height(24.dp))
                            }
                            // Where the row lands.
                            (drop as? Drop.Gap)?.takeIf { dragged != null && dragMoved }?.let { g ->
                                Box(
                                    Modifier.offset { androidx.compose.ui.unit.IntOffset((base + g.depth * indent).toInt(), (g.lineY - scroll.value).toInt() - 2) }
                                        .padding(end = 12.dp).fillMaxWidth().height(3.dp).clip(RoundedCornerShape(2.dp)).background(Ui.Red)
                                )
                            }
                            // Opened: the page in view is in view here too.
                            androidx.compose.runtime.LaunchedEffect(Unit) {
                                kotlinx.coroutines.delay(60)
                                val cur = rows.filterIsInstance<FfiOutlineRow.Sheet>().firstOrNull { model?.numberOf(it.node) == currentPage } ?: return@LaunchedEffect
                                rowBounds[cur.node]?.let { (top, _) -> scroll.scrollTo((top - with(density) { 120.dp.toPx() }).toInt().coerceAtLeast(0)) }
                            }
                        }
                    }
                }
                // A page or a group more; all groups folded or unfolded.
                if (!readOnly) Box(Modifier.fillMaxWidth().padding(horizontal = 12.dp).height(1.dp).background(Ui.Border))
                if (!readOnly) Row(Modifier.fillMaxWidth().padding(start = 12.dp, end = 10.dp, top = 8.dp), verticalAlignment = Alignment.CenterVertically) {
                    FootBtn(Glyph.PLUS, "Seite") { addPage("below") }
                    FootBtn(Glyph.GROUP, "Gruppe") { addPage("group") }
                    Spacer(Modifier.weight(1f))
                    val groups = rows.filterIsInstance<FfiOutlineRow.Group>()
                    if (groups.isNotEmpty()) {
                        val anyOpen = groups.any { !it.collapsed }
                        FootBtn(if (anyOpen) Glyph.UP_ARROW else Glyph.DOWN_ARROW, if (anyOpen) "Zuklappen" else "Aufklappen") {
                            model?.setCollapsedAll(groups.filter { it.collapsed != anyOpen }.map { it.node }, anyOpen)
                        }
                    }
                }
            }
            // Scroll while a row is dragged near the top or bottom edge.
            androidx.compose.runtime.LaunchedEffect(dragged) {
                if (dragged == null) return@LaunchedEffect
                val edge = with(density) { 56.dp.toPx() }
                while (dragNode != null) {
                    val vy = dragY - scroll.value
                    val step = when {
                        vy < edge -> -(edge - vy) / 4f
                        vy > scroll.viewportSize - edge -> (vy - (scroll.viewportSize - edge)) / 4f
                        else -> 0f
                    }
                    if (step != 0f && dragMoved) {
                        val moved = scroll.scrollBy(step)
                        dragY += moved
                        drop = computeDrop(dragged, indent)
                    }
                    kotlinx.coroutines.delay(16)
                }
            }
        }

        @Composable
        fun ViewPick(g: Glyph, on: Boolean, onClick: () -> Unit) {
            Box(Modifier.size(width = 36.dp, height = 30.dp).clip(RoundedCornerShape(8.dp)).background(if (on) Ui.Elevated else Color.Transparent).clickable(onClick = onClick), contentAlignment = Alignment.Center) {
                GlyphIcon(g, if (on) Ui.Chalk else Ui.Muted, 17.dp)
            }
        }

        @Composable
        fun FootBtn(g: Glyph, label: String, onClick: () -> Unit) {
            Row(Modifier.padding(end = 6.dp).clip(RoundedCornerShape(10.dp)).border(1.dp, Ui.Border, RoundedCornerShape(10.dp)).clickable(onClick = onClick)
                .padding(horizontal = 10.dp, vertical = 7.dp), verticalAlignment = Alignment.CenterVertically) {
                GlyphIcon(g, Ui.Chalk, 15.dp)
                Text(label, color = Ui.Chalk, fontSize = 12.5.sp, modifier = Modifier.padding(start = 6.dp))
            }
        }

        /** A page's name: its own, else its first heading, a text page's first line, else what kind of page it is. */
        fun pageName(r: FfiOutlineRow.Sheet): String = r.title.ifBlank {
            heads.firstOrNull { it.node == r.node }?.text?.takeIf { it.isNotBlank() }
                ?: (if (r.spec.background == FfiBackground.TEXT) model?.sheets?.get(r.sheetId)?.texts?.firstOrNull()?.text?.lineSequence()
                    ?.map { it.trim().trimStart('#', '-', '*', '>', ' ') }?.firstOrNull { it.isNotBlank() }?.take(60) else null)
                ?: if (r.spec.width <= 0f) "Board" else if (r.spec.pdfBlob != null) "PDF-Seite ${r.spec.pdfPage + 1u}" else if (r.spec.background == FfiBackground.TEXT) "Textseite" else "Seite"
        }

        /** Small pictures of pages, drawn anew when their page changes. */
        val thumbs = HashMap<String, Pair<Int, androidx.compose.ui.graphics.ImageBitmap>>()

        /** A page as a little picture [w] wide (a board: its icon). */
        @Composable
        fun PageThumb(r: FfiOutlineRow.Sheet, w: androidx.compose.ui.unit.Dp, current: Boolean = false, box: Boolean = false, boxRatio: Float = 1.3f,
                      boxAlign: Alignment = Alignment.Center, overlay: @Composable androidx.compose.foundation.layout.BoxScope.() -> Unit = {}) {
            @Suppress("UNUSED_VARIABLE") val t = tick
            // Read so a page that loaded (or changed) since gets its picture: its version is no state.
            @Suppress("UNUSED_VARIABLE") val changes = modelTick
            val density = androidx.compose.ui.platform.LocalDensity.current
            val px = with(density) { w.roundToPx() }.coerceAtLeast(8)
            val board = r.spec.width <= 0f
            val aspect = if (board) 0.75f else (r.spec.height / r.spec.width).coerceIn(0.3f, 3f)
            val st = model?.sheets?.get(r.sheetId)
            val version = st?.version ?: -1
            val img = remember(r.sheetId, version, px, rows) {
                thumbs[r.sheetId]?.takeIf { it.first == version && it.second.width == px && version >= 0 }?.second
                    ?: if (board) null else view.pageThumb(r.sheetId, px)?.asImageBitmap()?.also { if (model?.sheets?.get(r.sheetId)?.handle != null) thumbs[r.sheetId] = version to it }
            }
            val shape = RoundedCornerShape(3.dp)
            // In a list: always the same upright box (rows keep one height), the page fitted in.
            val (bw, bh) = if (box) w to w * boxRatio else w to w * aspect
            Box(Modifier.size(bw, bh), contentAlignment = boxAlign) {
                val (iw, ih) = if (box) { if (aspect >= boxRatio) bh / aspect to bh else bw to bw * aspect } else bw to bh
                Box(Modifier.size(iw, ih).clip(shape).background(if (board) Ui.Elevated else Ui.Paper)
                    .border(if (current) 2.dp else 1.dp, if (current) Ui.Red else Ui.Border, shape), contentAlignment = Alignment.Center) {
                    if (img != null) androidx.compose.foundation.Image(img, null, Modifier.fillMaxSize(), contentScale = androidx.compose.ui.layout.ContentScale.FillBounds)
                    else if (board) GlyphIcon(Glyph.BOARD, Ui.Muted, w * 0.4f)
                    overlay()
                }
            }
        }

        /** The outline as pictures of its pages, two to a row; groups as headings that fold. */
        @OptIn(androidx.compose.foundation.ExperimentalFoundationApi::class)
        @Composable
        fun PagePictures() {
            val grid = androidx.compose.foundation.lazy.grid.rememberLazyGridState()
            androidx.compose.foundation.lazy.grid.LazyVerticalGrid(
                columns = androidx.compose.foundation.lazy.grid.GridCells.Fixed(2), state = grid,
                contentPadding = androidx.compose.foundation.layout.PaddingValues(start = 14.dp, end = 14.dp, top = 6.dp, bottom = 24.dp),
                horizontalArrangement = Arrangement.spacedBy(12.dp), verticalArrangement = Arrangement.spacedBy(12.dp), modifier = Modifier.fillMaxSize(),
            ) {
                // Groups open as a heading and close with a line (pages after a group are not in it).
                val open = ArrayList<Int>()
                rows.forEachIndexed { i, r ->
                    // (A group right after one at its level: its heading is line enough.)
                    val closed = OutlineDrop.closesBefore(rows, i, open)
                    if (closed >= 0) {
                        item(key = "end-$i", span = { androidx.compose.foundation.lazy.grid.GridItemSpan(maxLineSpan) }) {
                            Box(Modifier.fillMaxWidth().padding(start = (closed * 12).dp, top = 2.dp, bottom = 2.dp).height(1.dp).background(Ui.Border))
                        }
                    }
                    when (r) {
                        is FfiOutlineRow.Group -> item(key = r.node, span = { androidx.compose.foundation.lazy.grid.GridItemSpan(maxLineSpan) }) {
                            // A group: a heading across both columns that folds, a line under it.
                            Column(Modifier.fillMaxWidth().padding(start = (r.depth.toInt() * 12).dp, top = 4.dp)) {
                                Row(Modifier.fillMaxWidth().clip(RoundedCornerShape(8.dp)).clickable { model?.setCollapsed(r.node, !r.collapsed) }.padding(vertical = 6.dp),
                                    verticalAlignment = Alignment.CenterVertically) {
                                    val turn by androidx.compose.animation.core.animateFloatAsState(if (r.collapsed) 0f else 90f, label = "fold")
                                    Box(Modifier.width(16.dp).graphicsLayer { rotationZ = turn }, contentAlignment = Alignment.Center) { Chevron(Ui.Red, 15.dp) }
                                    Box(Modifier.padding(start = 6.dp, end = 8.dp)) { GlyphIcon(Glyph.FOLDER, Ui.Chalk.copy(alpha = 0.8f), 17.dp) }
                                    Text(r.title.ifBlank { "Gruppe" }, color = Ui.Chalk, fontSize = 14.sp, fontWeight = FontWeight.SemiBold, maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f))
                                    Text("${r.count}", color = Ui.Muted, fontSize = 11.sp, fontFamily = Voice.metaFont,
                                        modifier = Modifier.clip(RoundedCornerShape(8.dp)).background(Ui.Elevated).padding(horizontal = 7.dp, vertical = 2.dp))
                                }
                                Box(Modifier.fillMaxWidth().height(1.dp).background(Ui.Border))
                            }
                        }
                        is FfiOutlineRow.Sheet -> item(key = r.node) {
                            val n = model?.numberOf(r.node) ?: 0
                            val slop = androidx.compose.ui.platform.LocalViewConfiguration.current.touchSlop
                            val haptic = androidx.compose.ui.platform.LocalHapticFeedback.current
                            var root by remember { mutableStateOf(androidx.compose.ui.geometry.Offset.Zero) }
                            Box {
                                // Tap: show the page. Hold: its menu; hold and drag: the page beside (onto the other pane).
                                Column(Modifier.onGloballyPositioned { root = it.positionInRoot() }.graphicsLayer { if (ghost?.first?.node == r.node) alpha = 0.4f }.pointerInput(r) {
                                    awaitEachGesture {
                                        val down = awaitFirstDown(requireUnconsumed = false)
                                        val long = awaitLongPressOrCancellation(down.id)
                                        if (long == null) {
                                            val up = currentEvent.changes.firstOrNull { it.id == down.id }
                                            if (up != null && up.changedToUpIgnoreConsumed() && !up.isConsumed && (up.position - down.position).getDistance() < slop) view.scrollTo(r.sheetId)
                                            return@awaitEachGesture
                                        }
                                        if (readOnly) return@awaitEachGesture
                                        haptic.performHapticFeedback(androidx.compose.ui.hapticfeedback.HapticFeedbackType.LongPress)
                                        var moved = false
                                        val ok = drag(long.id) { ch ->
                                            ch.consume()
                                            val at = root + ch.position
                                            if (!moved && (ch.position - long.position).getDistance() > slop) { moved = true; dragNode = r.node; dragMoved = true }
                                            if (moved) { ghost = r to at; besideSheet = if (inBesideZone(at.x, at.y)) r.sheetId else null }
                                        }
                                        val beside = besideSheet
                                        ghost = null; besideSheet = null; dragNode = null; dragMoved = false
                                        if (!moved) picMenu = r.node
                                        else if (ok && beside != null) openBeside(this@Pane, nbId, beside)
                                    }
                                }) {
                                    // All pictures the same height (wide pages sit in the middle), the number on the picture.
                                    val here = n == currentPage
                                    PageThumb(r, 132.dp, current = here, box = true, boxAlign = Alignment.BottomCenter) {
                                        if (r.node in marks) Box(Modifier.align(Alignment.TopEnd).padding(4.dp)) { GlyphIcon(Glyph.MARK, Ui.Red, 18.dp) }
                                        Text("$n", color = if (here) Ui.OnAccent else Ui.Chalk, fontSize = 11.sp, fontFamily = Voice.metaFont, fontWeight = FontWeight.SemiBold,
                                            modifier = Modifier.align(Alignment.BottomStart).padding(5.dp).clip(RoundedCornerShape(7.dp))
                                                .background(if (here) Ui.Red else Ui.Bar.copy(alpha = 0.88f)).padding(horizontal = 7.dp, vertical = 2.dp))
                                        val sides = r.left.size + r.right.size
                                        if (sides > 0) Text("+$sides", color = Ui.Chalk, fontSize = 10.sp, fontFamily = Voice.metaFont,
                                            modifier = Modifier.align(Alignment.BottomEnd).padding(5.dp).clip(RoundedCornerShape(7.dp)).background(Ui.Bar.copy(alpha = 0.88f)).padding(horizontal = 6.dp, vertical = 2.dp))
                                    }
                                    Text(pageName(r).let { if (it == "Seite") "Seite $n" else it }, color = if (here) Ui.Chalk else Ui.Chalk.copy(alpha = 0.85f), fontSize = 12.5.sp,
                                        fontWeight = if (here) FontWeight.SemiBold else FontWeight.Normal, maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.padding(top = 6.dp, start = 2.dp))
                                }
                                if (picMenu == r.node) PageMenu(r) { picMenu = null }
                            }
                        }
                    }
                }
            }
            androidx.compose.runtime.LaunchedEffect(Unit) {
                val i = rows.indexOfFirst { it is FfiOutlineRow.Sheet && model?.numberOf(it.node) == currentPage }
                // The closing lines are items too: by row index alone it landed too early.
                if (i > 0) grid.scrollToItem(OutlineDrop.gridIndex(rows, i))
            }
        }

        /** A page's menu in the pictures (held): rename, mark, delete. */
        @Composable
        fun PageMenu(r: FfiOutlineRow.Sheet, close: () -> Unit) {
            var sure by remember { mutableStateOf(false) }
            androidx.compose.ui.window.Popup(alignment = Alignment.TopCenter, onDismissRequest = close, properties = androidx.compose.ui.window.PopupProperties(focusable = true)) {
                N0teType {
                    Column(Modifier.width(200.dp).panel().padding(vertical = 6.dp)) {
                        Item(Glyph.PEN, "Umbenennen") { close(); renaming = r.node to r.title }
                        val marked = r.node in marks
                        Item(Glyph.MARK, if (marked) "Nicht mehr merken" else "Merken") { close(); model?.setMarked(r.node, !marked) }
                        Item(Glyph.SPLIT, "Daneben öffnen") { close(); openBeside(this@Pane, nbId, r.sheetId) }
                        Item(Glyph.TRASH, if (sure) "Wirklich löschen?" else "Löschen") { if (sure) { close(); deleteWithUndo(r.node, "Seite") } else sure = true }
                    }
                }
            }
        }

        /** The marked pages, wherever they are: picture, number, name, its group; tap shows it. */
        @Composable
        fun MarkedPages() {
            val all = rows.filterIsInstance<FfiOutlineRow.Sheet>()
            val list = model?.let { m -> marks.mapNotNull { node -> all.firstOrNull { it.node == node } }.sortedBy { m.numberOf(it.node) } }.orEmpty()
            Column(Modifier.fillMaxWidth().verticalScroll(androidx.compose.foundation.rememberScrollState()).padding(vertical = 4.dp)) {
                if (list.isEmpty()) Text("Die gemerkten Seiten liegen in zugeklappten Gruppen.", color = Ui.Muted, fontSize = 12.sp, modifier = Modifier.padding(16.dp))
                list.forEach { r ->
                    val n = model?.numberOf(r.node) ?: 0
                    Row(Modifier.fillMaxWidth().clickable { view.scrollTo(r.sheetId) }.background(if (n == currentPage) Ui.Elevated else Color.Transparent)
                        .padding(horizontal = 14.dp, vertical = 7.dp), verticalAlignment = Alignment.CenterVertically) {
                        PageThumb(r, 30.dp, current = n == currentPage, box = true)
                        Text("%02d".format(n), color = if (n == currentPage) Ui.Red else Ui.Muted, fontSize = 12.sp, fontFamily = Voice.metaFont, modifier = Modifier.padding(start = 12.dp, end = 8.dp))
                        Text(pageName(r), color = Ui.Chalk, fontSize = 13.sp, maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f))
                        Box(Modifier.size(30.dp).clip(RoundedCornerShape(8.dp)).clickable { model?.setMarked(r.node, false) }, contentAlignment = Alignment.Center) { GlyphIcon(Glyph.MARK, Ui.Red, 16.dp) }
                    }
                }
            }
        }

        /** The outline's search field: typing searches the notebook (handwriting, text, headings, PDFs). */
        @Composable
        fun SearchField() {
            Row(Modifier.padding(horizontal = 12.dp, vertical = 4.dp).fillMaxWidth().clip(RoundedCornerShape(12.dp)).background(Ui.Black)
                .border(1.dp, Ui.Border, RoundedCornerShape(12.dp)).padding(horizontal = 10.dp, vertical = 9.dp), verticalAlignment = Alignment.CenterVertically) {
                GlyphIcon(Glyph.SEARCH, Ui.Muted, 18.dp)
                Spacer(Modifier.width(8.dp))
                Box(Modifier.weight(1f)) {
                    if (query.isEmpty()) Text("Suchen …", color = Ui.Muted, fontSize = 14.sp)
                    BasicTextField(query, { q -> search(q) }, singleLine = true, textStyle = TextStyle(color = Ui.Chalk, fontSize = 14.sp), cursorBrush = SolidColor(Ui.Red),
                        modifier = Modifier.fillMaxWidth())
                }
                if (query.isNotEmpty()) Text("✕", color = Ui.Muted, fontSize = 14.sp, modifier = Modifier.clickable { search("") }.padding(start = 8.dp))
            }
        }

        fun search(q: String) {
            query = q
            val seq = ++searchSeq
            if (q.isBlank()) { hits = null; return }
            // A moment after the last key: search (in the background).
            N0te.main.postDelayed({ if (seq == searchSeq) model?.search(q) { r -> if (seq == searchSeq) hits = r } }, 250)
        }

        /** What the search found, page by page; tapping one shows it on its page. */
        @Composable
        fun SearchResults(modifier: Modifier) {
            @Suppress("UNUSED_VARIABLE") val t = tick
            val found = hits
            Column(modifier.fillMaxWidth().verticalScroll(androidx.compose.foundation.rememberScrollState())) {
                if (Settings.handwritingSearch && !Handwriting.ready) {
                    val s = when (Handwriting.state) {
                        Handwriting.State.LOADING -> "Die Handschrift-Erkennung lädt (einmalig, über WLAN) …"
                        Handwriting.State.FAILED -> "Handschrift-Erkennung nicht verfügbar: ${Handwriting.error ?: "Fehler"}"
                        else -> "Handschrift wird erst gefunden, wenn die Erkennung geladen ist (Einstellungen → Handschrift durchsuchbar)"
                    }
                    Text(s, color = Ui.Muted, fontSize = 12.sp, modifier = Modifier.padding(horizontal = 16.dp, vertical = 6.dp))
                }
                when {
                    found == null -> Text("Sucht …", color = Ui.Muted, fontSize = 13.sp, modifier = Modifier.padding(16.dp))
                    found.isEmpty() -> Text("Nichts gefunden", color = Ui.Muted, fontSize = 13.sp, modifier = Modifier.padding(16.dp))
                    else -> {
                        Text(if (found.size == 1) "1 Treffer" else "${found.size} Treffer", color = Ui.Muted, fontSize = 12.sp, fontFamily = Voice.metaFont, modifier = Modifier.padding(horizontal = 16.dp, vertical = 6.dp))
                        var lastNode: String? = null
                        for (h in found) {
                            if (h.node != lastNode) {
                                lastNode = h.node
                                val main = rows.filterIsInstance<FfiOutlineRow.Sheet>().firstOrNull { r -> r.node == h.node || r.left.any { it.node == h.node } || r.right.any { it.node == h.node } }
                                // In a folded group the row isn't among the visible ones: numbered from the whole outline.
                                val n = main?.let { model?.numberOf(it.node) } ?: model?.numberOf(h.node) ?: 0
                                val name = main?.title?.ifBlank { null } ?: heads.firstOrNull { it.node == main?.node }?.text ?: "Seite"
                                Text("%02d  %s".format(n, name), color = Ui.Chalk, fontSize = 13.sp, fontWeight = FontWeight.SemiBold, maxLines = 1, overflow = TextOverflow.Ellipsis,
                                    modifier = Modifier.padding(start = 16.dp, end = 16.dp, top = 10.dp, bottom = 2.dp))
                            }
                            Row(Modifier.fillMaxWidth().clickable { showHit(h) }.padding(start = 26.dp, end = 14.dp, top = 6.dp, bottom = 6.dp),
                                verticalAlignment = Alignment.CenterVertically) {
                                val g = when (h.kind) { uniffi.n0te_ffi.FfiHitKind.HEADING -> Glyph.HEADING; uniffi.n0te_ffi.FfiHitKind.PDF -> Glyph.PDF; uniffi.n0te_ffi.FfiHitKind.TEXT -> Glyph.TEXT; uniffi.n0te_ffi.FfiHitKind.PLOT -> Glyph.PLOT; else -> Glyph.PEN }
                                GlyphIcon(g, Ui.Muted, 15.dp)
                                Spacer(Modifier.width(8.dp))
                                Text(highlighted(h.text, query), color = Ui.Chalk, fontSize = 13.sp, maxLines = 2, overflow = TextOverflow.Ellipsis)
                            }
                        }
                    }
                }
                Spacer(Modifier.height(48.dp))
            }
        }

        /** Goes to a search hit; one in a folded group unfolds it first (it did nothing). */
        fun showHit(h: uniffi.n0te_ffi.FfiSearchHit) {
            val spot = android.graphics.RectF(h.x0, h.y0, h.x1, h.y1)
            val m = model ?: return
            if (m.placed.any { it.sheetId == h.sheetId }) return view.showSpot(h.sheetId, spot)
            m.allRows { all ->
                val groups = OutlineDrop.groupsAround(all, h.node)
                if (groups.isEmpty()) view.showSpot(h.sheetId, spot)
                else m.setCollapsedAll(groups, false) { view.post { view.showSpot(h.sheetId, spot) } }
            }
        }

        /** [text] with the words of [q] in bold where they appear (ignoring case). */
        fun highlighted(text: String, q: String): androidx.compose.ui.text.AnnotatedString = androidx.compose.ui.text.buildAnnotatedString {
            append(text)
            for (w in q.split(' ').filter { it.length >= 2 }) {
                var i = text.indexOf(w, ignoreCase = true)
                while (i >= 0) {
                    addStyle(androidx.compose.ui.text.SpanStyle(fontWeight = FontWeight.Bold, color = Ui.Red), i, i + w.length)
                    i = text.indexOf(w, i + w.length, ignoreCase = true)
                }
            }
        }

        /** A heading under its page in the outline: tap shows it, hold to make it no heading again. */
        @Composable
        fun HeadingRow(h: uniffi.n0te_ffi.FfiPageHeading, depth: Int) {
            Column(Modifier.fillMaxWidth()) {
                // Under its page's name, a little in: what is marked as a heading on it.
                Row(Modifier.fillMaxWidth().treeGuides(depth)
                    .combinedClickable(onLongClick = { if (!readOnly) heldHeading = if (heldHeading == h.id) null else h.id }) {
                        heldHeading = null
                        view.showSpot(h.sheetId, android.graphics.RectF(h.x0, h.y0, h.x1, h.y1))
                    }
                    .padding(start = (OUT_BASE + depth * OUT_INDENT + 44).dp, end = 14.dp, top = 3.dp, bottom = 5.dp), verticalAlignment = Alignment.CenterVertically) {
                    Box(Modifier.padding(end = 8.dp).size(width = 8.dp, height = 1.5.dp).background(Ui.Muted))
                    Text(h.text.ifBlank { "Überschrift" }, color = Ui.Chalk.copy(alpha = 0.8f), fontSize = 12.5.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
                }
                if (heldHeading == h.id) Row(Modifier.padding(start = (OUT_BASE + depth * OUT_INDENT + 44).dp, bottom = 8.dp)) {
                    QuickBtn(Glyph.TRASH, "Keine Überschrift") { heldHeading = null; model?.removeHeading(h.sheetId, h.id) }
                }
            }
        }

        // --- editing a graph (GeoGebra-like) --------------------------------------------------

        /** The graph being edited, and what changes it. */
        var graph by mutableStateOf<GraphSession?>(null)
        var graphTick by mutableStateOf(0)
        var graphPanel by mutableStateOf(true)
        /** The graph fills the room beside the panel (else it stays as on the page). */
        var graphBig by mutableStateOf(Settings.str("graph_big", "1") == "1")
        /** Where the view stood before editing (it goes back there). */
        var graphHome: FloatArray? = null

        /** Shows the edited graph: big in the room beside the panel and bars, or as it was on the page. */
        fun fitGraph() {
            val g = graph ?: return
            if (graphBig) return
            val st = view.storedPlot(g.sheetId, g.draft.id) ?: return
            val box = android.graphics.RectF(st.x, st.y, st.x + st.w, st.y + st.h)
            val d = resources.displayMetrics.density
            val left = if (graphPanel) (GRAPH_PANEL_W + 30) * d else 24 * d
            if (graphBig) {
                val bottom = view.height - (if (g.keys.shown) GRAPH_KEYS_H.value + 76f else 84f) * d
                view.fitBox(g.sheetId, box, android.graphics.RectF(left, 104 * d, view.width - 24 * d, bottom))
            } else graphHome?.let { view.showAt(it, g.sheetId, box, left) }
        }

        /**
         * The graph keyboard came up: the view stays where it is unless the keyboard would cover the
         * whole graph — then the page moves up just so far that the graph's top shows above it.
         */
        fun keepGraphAboveKeys() {
            val g = graph ?: return
            if (graphBig) return
            val box = graphBox(g) ?: return
            val page = (model?.placed?.firstOrNull { it.sheetId == g.sheetId } ?: view.board?.takeIf { it.sheetId == g.sheetId })?.rect ?: return
            val d = resources.displayMetrics.density
            val keysTop = view.height - GRAPH_KEYS_H.value * d
            val c = view.camera()
            val top = (page.top + box.top) * c[0] + c[2]
            val room = keysTop - 56 * d
            if (top > room) view.setCamera(floatArrayOf(c[0], c[1], c[2] - (top - room)))
        }

        /** The graph's box on the page (page points), or null when it is gone. */
        fun graphBox(g: GraphSession): android.graphics.RectF? =
            view.storedPlot(g.sheetId, g.draft.id)?.let { android.graphics.RectF(it.x, it.y, it.x + it.w, it.y + it.h) }

        /** Groß on or off; off, the graph is edited in its box on the page again. */
        fun bigGraph(on: Boolean) {
            val g = graph
            if (!on && g != null) graphBox(g)?.let { g.toPage(it) }
            g?.big = on
            graphBig = on
            Settings.set("graph_big", if (on) "1" else "0")
        }

        /** Starts editing a coordinate system: its tools in the graph, the panel beside it. */
        fun startGraph(sheetId: String, plot: uniffi.n0te_ffi.FfiPlot, fresh: Boolean) {
            if (closed) return
            // Another graph open (a late reading from the laptop, the hold menu): it is kept first, not dropped.
            if (graph != null) finishGraph(true)
            view.clearSelection()
            val s = GraphSession(sheetId, plot, fresh) { graphTick++; graph?.let { view.plotDraft = it.shown() } }
            s.big = graphBig
            // Squared paper: the graph keeps to its squares (in "Groß" they are drawn behind it).
            s.squares = model?.placed?.firstOrNull { it.sheetId == sheetId }?.spec?.takeIf { it.background == uniffi.n0te_ffi.FfiBackground.GRID && it.spacing > 2f }?.spacing ?: 0f
            graph = s
            view.plotDraft = plot
            var downHit: String? = null
            view.graph = object : NotebookView.GraphTouch {
                override fun sheet() = sheetId
                override fun box(): android.graphics.RectF {
                    val d = s.draft
                    val st = view.storedPlot(sheetId, d.id)
                    return if (st != null) android.graphics.RectF(st.x, st.y, st.x + st.w, st.y + st.h) else android.graphics.RectF(d.x, d.y, d.x + d.w, d.y + d.h)
                }
                override fun down(x: Float, y: Float, tol: Float) {
                    downHit = uniffi.n0te_ffi.plotHit(s.draft, x, y, tol)
                    s.down(x, y, downHit, tol)
                }
                override fun move(x: Float, y: Float) = s.move(x, y)
                override fun up(x: Float, y: Float, tap: Boolean) {
                    s.up()
                    if (tap) s.tap(x, y, downHit)
                }
                override fun pinch(k: Float, cx: Float, cy: Float) = s.pinch(k, cx, cy)
                override fun end() = s.settle()
            }
            s.choose(GTool.MOVE)
            graphHome = view.camera()
        }

        /** Done: the edited graph is stored (one step to undo), in its box as it is now. */
        fun finishGraph(keep: Boolean) {
            val s = graph ?: return
            // Back into its box on the page (it may have been edited in the whole room).
            graphBox(s)?.let { s.toPage(it) }
            if (keep) {
                val stored = view.storedPlot(s.sheetId, s.draft.id)
                if (stored != null) {
                    val d = s.draft.copy(x = stored.x, y = stored.y, w = stored.w, h = stored.h)
                    if (d != stored) model?.setPlot(s.sheetId, d)
                }
            }
            s.keys.active = null
            graph = null
            view.graph = null
            view.plotDraft = null
            graphHome?.let { view.setCamera(it) }
            graphHome = null
        }

        /**
         * The selected handwriting read as a function (on the laptop, like LaTeX) and drawn: into a
         * coordinate system next to it, or a new one beside it.
         */
        fun graphFromSelection() {
            val sel = selection ?: return
            if (sel.multi) { flash("Graph: bitte nur auf einer Seite auswählen"); return }
            val bounds = android.graphics.RectF(sel.bounds)
            // A new row of the graph nearest to it (or a new graph beside it); false: it doesn't read as one.
            fun place(expr: String): Boolean {
                val near = nearestPlot(sel.sheetId, bounds)
                if (near != null) {
                    val withIt = runCatching { uniffi.n0te_ffi.plotAdd(near, expr) }.getOrElse { return false }
                    startGraph(sel.sheetId, near, false)
                    graph?.set(withIt)
                } else {
                    if (runCatching { uniffi.n0te_ffi.plotAdd(Plots.fresh(0f, 0f, 0f, 0f, uniffi.n0te_ffi.FfiBackground.BLANK, 0f), expr) }.isFailure) return false
                    view.insertPlotBeside(sel.sheetId, bounds, expr, Plots.nextColor(emptyList())) { id, p -> startGraph(id, p, false) }
                }
                flash("In den Graphen: $expr")
                return true
            }
            // The tablet's own reading of the lines it is in (holding one word of "y = 2x + 1" is enough).
            fun readHere(why: String?) {
                model?.readText(sel.sheetId, sel.ids) { text ->
                    val expr = uniffi.n0te_ffi.plotFromHandwriting(text)
                    when {
                        text.isBlank() -> flash(why ?: if (Handwriting.ready) "Graph: die Schrift ist noch nicht gelesen, gleich noch einmal" else "Graph: Handschrift lesen ist aus (Einstellungen › Handschrift)")
                        !place(expr) -> flash("Graph: „$text“ ist keine Funktion")
                    }
                }
            }
            // The laptop reads formulas best (LaTeX); without it, or when its reading doesn't fit, the tablet's.
            view.selectionPng { png ->
                if (closed) return@selectionPng
                val asked = png != null && SyncHub.requestLatex(png) { latex, _, error ->
                    if (closed) return@requestLatex
                    val expr = if (error == null) runCatching { uniffi.n0te_ffi.plotFromLatex(latex) }.getOrNull() else null
                    if (expr == null || !place(expr)) readHere(error?.let { "Graph: $it" })
                }
                if (asked) flash("Lese die Funktion … (auf dem Laptop)") else readHere(null)
            }
        }

        /** The coordinate system on [sheetId] nearest to [r] (page points), if one is close (beside it or around it). */
        fun nearestPlot(sheetId: String, r: android.graphics.RectF): uniffi.n0te_ffi.FfiPlot? {
            fun gap(p: uniffi.n0te_ffi.FfiPlot): Float {
                val dx = maxOf(p.x - r.right, r.left - (p.x + p.w), 0f)
                val dy = maxOf(p.y - r.bottom, r.top - (p.y + p.h), 0f)
                return kotlin.math.hypot(dx, dy)
            }
            return model?.sheets?.get(sheetId)?.images?.mapNotNull { it.plot }?.filter { gap(it) < 160f }?.minByOrNull { gap(it) }
        }

        /** Marking the selected strokes as a heading: its words (the recogniser's reading to start with). */
        @Composable
        fun HeadingDialog(sheetId: String, ids: Set<String>) {
            val focus = remember { androidx.compose.ui.focus.FocusRequester() }
            androidx.compose.runtime.LaunchedEffect(sheetId, ids) { focus.requestFocus() }
            val old = headingOld
            fun done() {
                if (old != null) model?.setHeadingText(sheetId, old, headingText.text.trim())
                else { model?.addHeading(sheetId, ids, headingText.text.trim()); flash("Überschrift: steht jetzt in der Gliederung") }
                headingFor = null
                view.clearSelection()
            }
            Dialog(onDismissRequest = { headingFor = null }) {
                Column(Modifier.width(420.dp).panel().padding(vertical = 14.dp)) {
                    Section("Überschrift")
                    Text("So steht sie in der Gliederung (und wird gefunden):", color = Ui.Muted, fontSize = 13.sp, modifier = Modifier.padding(horizontal = 16.dp, vertical = 4.dp))
                    Box(Modifier.padding(horizontal = 16.dp, vertical = 4.dp).fillMaxWidth().clip(RoundedCornerShape(12.dp)).background(Ui.Black).border(1.dp, Ui.Border, RoundedCornerShape(12.dp)).padding(12.dp)) {
                        if (headingText.text.isEmpty()) Text(if (Handwriting.ready) "Wird gelesen … oder selbst eintippen" else "Wie heißt sie?", color = Ui.Muted, fontSize = 15.sp)
                        BasicTextField(headingText, { headingText = it }, singleLine = true, textStyle = TextStyle(color = Ui.Chalk, fontSize = 15.sp), cursorBrush = SolidColor(Ui.Red),
                            keyboardOptions = androidx.compose.foundation.text.KeyboardOptions(imeAction = androidx.compose.ui.text.input.ImeAction.Done),
                            keyboardActions = androidx.compose.foundation.text.KeyboardActions(onDone = { done() }),
                            modifier = Modifier.fillMaxWidth().focusRequester(focus))
                    }
                    Row(Modifier.padding(horizontal = 16.dp, vertical = 8.dp).fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        if (old != null) Pill("Keine Überschrift") { model?.removeHeading(sheetId, old); headingFor = null; view.clearSelection() }
                        Spacer(Modifier.weight(1f))
                        Btn("Abbrechen", BtnKind.QUIET) { headingFor = null }
                        Pill(if (old != null) "OK" else "Als Überschrift", primary = true) { done() }
                    }
                }
            }
        }

        fun nodeOf(r: FfiOutlineRow) = OutlineDrop.nodeOf(r)
        fun depthOf(r: FfiOutlineRow) = OutlineDrop.depthOf(r)
        fun subtreeOf(node: String?): Set<String> = OutlineDrop.subtree(rows, node)

        fun computeDrop(node: String, indent: Float): Drop? =
            when (val d = OutlineDrop.compute(rows, rowBounds, node, dragX - dragStartX, dragY, indent * 1.8f)) {
                is OutlineDrop.Target.Gap -> Drop.Gap(d.parent, d.index, d.depth, d.lineY)
                is OutlineDrop.Target.Into -> Drop.Into(d.group)
                null -> null
            }

        fun finishDrag(commit: Boolean) {
            val node = dragNode ?: return
            val target = drop
            val moved = dragMoved
            val beside = besideSheet
            dragNode = null; dragMoved = false; drop = null; besideSheet = null; ghost = null
            if (!moved || !commit) return // held without moving: the quick actions stay
            held = null
            // Dropped on the other pane (or the right of the screen): opens there.
            if (beside != null) { openBeside(this, nbId, beside); return }
            val m = model ?: return
            when (target) {
                is Drop.Gap -> m.moveNode(node, target.parent, target.index)
                is Drop.Into -> {
                    m.moveNode(node, target.group, Int.MAX_VALUE)
                    val g = rows.firstOrNull { nodeOf(it) == target.group } as? FfiOutlineRow.Group
                    flash("In „${g?.title?.ifBlank { "Gruppe" } ?: "Gruppe"}“ verschoben")
                }
                null -> {}
            }
        }

        @Composable
        fun OutlineRow(r: FfiOutlineRow, n: Int, dimmed: Boolean) {
            val node = nodeOf(r)
            val depth = depthOf(r)
            val haptic = androidx.compose.ui.platform.LocalHapticFeedback.current
            val slop = androidx.compose.ui.platform.LocalViewConfiguration.current.touchSlop
            val isDragged = dragNode == node && dragMoved
            val isInto = (drop as? Drop.Into)?.group == node && dragNode != null && dragMoved
            val isHeld = held == node && !dragMoved
            val title = when (r) { is FfiOutlineRow.Group -> r.title; is FfiOutlineRow.Sheet -> r.title }
            Column(
                Modifier.fillMaxWidth()
                    .onGloballyPositioned { c -> val y = c.positionInParent().y; rowBounds[node] = y to (y + c.size.height); rowRoot[node] = c.positionInRoot() }
                    .pointerInput(r) {
                        awaitEachGesture {
                            val down = awaitFirstDown(requireUnconsumed = false)
                            val long = awaitLongPressOrCancellation(down.id)
                            if (long == null) {
                                // A plain tap: open the page / fold the group.
                                val up = currentEvent.changes.firstOrNull { it.id == down.id }
                                if (inlineRename != node && up != null && up.changedToUpIgnoreConsumed() && !up.isConsumed && (up.position - down.position).getDistance() < slop) {
                                    held = null
                                    confirmDelete = null
                                    when (r) {
                                        is FfiOutlineRow.Group -> model?.setCollapsed(r.node, !r.collapsed)
                                        is FfiOutlineRow.Sheet -> view.scrollTo(r.sheetId)
                                    }
                                }
                                return@awaitEachGesture
                            }
                            // Read-only guests: no quick actions, no moving pages.
                            if (readOnly) return@awaitEachGesture
                            haptic.performHapticFeedback(androidx.compose.ui.hapticfeedback.HapticFeedbackType.LongPress)
                            held = node; confirmDelete = null; inlineRename = null
                            dragNode = node; dragMoved = false
                            dragStartX = long.position.x; dragX = dragStartX
                            dragStartY = (rowBounds[node]?.first ?: 0f) + long.position.y; dragY = dragStartY
                            val ok = drag(long.id) { change ->
                                change.consume()
                                dragX = change.position.x
                                dragY = (rowBounds[node]?.first ?: 0f) + change.position.y
                                if (!dragMoved && (kotlin.math.abs(dragY - dragStartY) > slop || kotlin.math.abs(dragX - dragStartX) > slop * 2)) dragMoved = true
                                // A page pulled far to the right: open it beside instead of moving it.
                                val at = (rowRoot[node] ?: androidx.compose.ui.geometry.Offset.Zero) + change.position
                                besideSheet = if (dragMoved && r is FfiOutlineRow.Sheet && inBesideZone(at.x, at.y)) r.sheetId else null
                                ghost = if (besideSheet != null && r is FfiOutlineRow.Sheet) r to at else null
                                if (dragMoved) drop = if (besideSheet != null) null else computeDrop(node, OUT_INDENT.dp.toPx())
                            }
                            finishDrag(commit = ok)
                        }
                    }
                    .graphicsLayer {
                        if (isDragged) { translationY = dragY - dragStartY; shadowElevation = 12f; alpha = 0.95f }
                        else if (dimmed) alpha = 0.35f
                    }
                    .treeGuides(depth)
                    .padding(horizontal = 6.dp).clip(RoundedCornerShape(10.dp))
                    .background(
                        when {
                            isDragged -> Ui.Elevated
                            isInto -> Ui.Red.copy(alpha = 0.28f)
                            isHeld -> Ui.Elevated
                            r is FfiOutlineRow.Sheet && n == currentPage -> Ui.Elevated
                            else -> Color.Transparent
                        }
                    )
                    // The page in view: a red edge at the start of its row.
                    .drawBehind { if (r is FfiOutlineRow.Sheet && n == currentPage) drawRect(Ui.Red, size = androidx.compose.ui.geometry.Size(3.dp.toPx(), size.height)) }
            ) {
                val here = r is FfiOutlineRow.Sheet && n == currentPage
                Row(Modifier.fillMaxWidth()
                    .padding(start = (OUT_BASE - 6 + depth * OUT_INDENT).dp, end = 8.dp, top = if (r is FfiOutlineRow.Group) 9.dp else 6.dp, bottom = if (r is FfiOutlineRow.Group) 9.dp else 6.dp),
                    verticalAlignment = Alignment.CenterVertically) {
                    when (r) {
                        is FfiOutlineRow.Group -> {
                            // The arrow turns when the group folds/unfolds.
                            val turn by androidx.compose.animation.core.animateFloatAsState(if (r.collapsed) 0f else 90f, label = "fold")
                            Box(Modifier.width(16.dp).graphicsLayer { rotationZ = turn }, contentAlignment = Alignment.Center) { Chevron(Ui.Red, 15.dp) }
                            Box(Modifier.padding(start = 6.dp, end = 8.dp)) { GlyphIcon(Glyph.FOLDER, Ui.Chalk.copy(alpha = 0.8f), 17.dp) }
                        }
                        // Upright and wide pages alike fill a near-square box.
                        is FfiOutlineRow.Sheet -> Box(Modifier.padding(end = 10.dp)) { PageThumb(r, 34.dp, current = here, box = true, boxRatio = 1.1f) }
                    }
                    if (inlineRename == node) {
                        InlineTitle(node, title)
                    } else when (r) {
                        is FfiOutlineRow.Group -> {
                            Text(title.ifBlank { "Gruppe" }, color = Ui.Chalk, fontSize = 14.sp, fontWeight = FontWeight.SemiBold, maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f))
                            // How many pages, in a small pill.
                            Text("${r.count}", color = Ui.Muted, fontSize = 11.sp, fontFamily = Voice.metaFont,
                                modifier = Modifier.clip(RoundedCornerShape(8.dp)).background(Ui.Elevated).padding(horizontal = 7.dp, vertical = 2.dp))
                        }
                        is FfiOutlineRow.Sheet -> {
                            // Its name (else "Seite 3"), under it the number and what else it has.
                            val own = pageName(r)
                            val generic = own == "Seite"
                            Column(Modifier.weight(1f)) {
                                Text(if (generic) "Seite $n" else own, color = Ui.Chalk, fontSize = 13.5.sp, fontWeight = if (here) FontWeight.SemiBold else FontWeight.Normal,
                                    maxLines = 1, overflow = TextOverflow.Ellipsis)
                                val meta = listOfNotNull(if (generic) null else "Seite $n",
                                    r.left.size.takeIf { it > 0 }?.let { "$it links" }, r.right.size.takeIf { it > 0 }?.let { "$it rechts" })
                                if (meta.isNotEmpty()) Text(meta.joinToString(" · "), color = if (here) Ui.Red else Ui.Muted, fontSize = 11.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
                            }
                            if (r.node in marks) Box(Modifier.padding(start = 6.dp)) { GlyphIcon(Glyph.MARK, Ui.Red, 15.dp) }
                        }
                    }
                }
                if (isHeld && inlineRename != node) QuickActions(r, title)
            }
        }

        /** Rename / delete / more for the held outline row. */
        @Composable
        fun QuickActions(r: FfiOutlineRow, title: String) {
            val node = nodeOf(r)
            val group = r as? FfiOutlineRow.Group
            @OptIn(androidx.compose.foundation.layout.ExperimentalLayoutApi::class)
            androidx.compose.foundation.layout.FlowRow(Modifier.padding(start = (OUT_BASE - 6 + depthOf(r) * OUT_INDENT).dp, end = 10.dp, bottom = 10.dp), horizontalArrangement = Arrangement.spacedBy(6.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
                QuickBtn(Glyph.PEN, "Umbenennen") { inlineRename = node }
                if (r is FfiOutlineRow.Sheet) {
                    val marked = node in marks
                    QuickBtn(Glyph.MARK, if (marked) "Gemerkt" else "Merken") { held = null; model?.setMarked(node, !marked) }
                    // Also: drag the page to the right (onto the other pane).
                    QuickBtn(Glyph.SPLIT, "Daneben") { held = null; openBeside(this@Pane, nbId, r.sheetId) }
                }
                val sure = confirmDelete == node
                QuickBtn(Glyph.TRASH, if (sure) (if (group != null) (if (group.count.toInt() == 1) "1 Seite löschen?" else "${group.count} Seiten löschen?") else "Sicher?") else "Löschen", danger = sure) {
                    if (sure) { confirmDelete = null; held = null; deleteWithUndo(node, "Seite") } else confirmDelete = node
                }
                QuickBtn(Glyph.MENU, "Mehr") { held = null; renaming = node to title }
            }
        }

        @Composable
        fun QuickBtn(g: Glyph, label: String, danger: Boolean = false, onClick: () -> Unit) {
            Row(
                Modifier.clip(RoundedCornerShape(10.dp)).background(if (danger) Ui.Red else Ui.Black).border(1.dp, if (danger) Ui.Red else Ui.Border, RoundedCornerShape(10.dp))
                    .clickable(onClick = onClick).padding(horizontal = 8.dp, vertical = 6.dp),
                verticalAlignment = Alignment.CenterVertically
            ) {
                GlyphIcon(g, if (danger) Ui.OnAccent else Ui.Chalk, 14.dp)
                Text(label, color = if (danger) Ui.OnAccent else Ui.Chalk, fontSize = 11.sp, modifier = Modifier.padding(start = 4.dp))
            }
        }

        /** The title as a text field (Enter or ✓ saves). */
        @Composable
        fun androidx.compose.foundation.layout.RowScope.InlineTitle(node: String, title: String) {
            var tf by remember(node) { mutableStateOf(androidx.compose.ui.text.input.TextFieldValue(title, androidx.compose.ui.text.TextRange(0, title.length))) }
            val focus = remember { androidx.compose.ui.focus.FocusRequester() }
            val save = { val t = tf.text.trim(); if (t != title) model?.rename(node, t); inlineRename = null; held = null }
            BasicTextField(
                tf, { tf = it }, singleLine = true, textStyle = TextStyle(color = Ui.Chalk, fontSize = 14.sp), cursorBrush = SolidColor(Ui.Red),
                keyboardOptions = androidx.compose.foundation.text.KeyboardOptions(imeAction = androidx.compose.ui.text.input.ImeAction.Done),
                keyboardActions = androidx.compose.foundation.text.KeyboardActions(onDone = { save() }),
                modifier = Modifier.weight(1f).clip(RoundedCornerShape(8.dp)).background(Ui.Black).border(1.dp, Ui.Red, RoundedCornerShape(8.dp))
                    .padding(horizontal = 8.dp, vertical = 6.dp).focusRequester(focus)
            )
            Box(Modifier.padding(start = 6.dp).size(30.dp).clip(CircleShape).background(Ui.Red).clickable { save() }, contentAlignment = Alignment.Center) { GlyphIcon(Glyph.CHECK, Ui.OnAccent, 16.dp) }
            androidx.compose.runtime.LaunchedEffect(node) { focus.requestFocus() }
        }

        @Composable
        fun SelectionBar(modifier: Modifier) {
            @Suppress("UNUSED_VARIABLE") val t = tick
            Row(
                modifier.bar(24f).padding(horizontal = 12.dp, vertical = 8.dp),
                verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)
            ) {
                // A coordinate system: edit it, or away with it.
                view.selectedPlot()?.let { (sheetId, plot) ->
                    LabeledBtn(Glyph.PLOT, "Bearbeiten") { startGraph(sheetId, plot, false) }
                    LabeledBtn(Glyph.LAPTOP, "Laptop") { sendSelectionToLaptop() }
                    LabeledBtn(Glyph.TRASH, "Löschen") { view.deleteSelection() }
                    Divider()
                    IconBtn(Glyph.CHECK) { view.clearSelection() }
                    return@Row
                }
                Tools.inks.forEach { c -> Box(Modifier.size(22.dp).clip(CircleShape).background(Color(c)).clickable { view.recolorSelection(c) }) }
                Divider()
                // Copied: the bar closes; pasting is a finger held on the spot (hold menu).
                LabeledBtn(Glyph.COPY, "Kopieren") { view.copySelection(cut = false) { canPaste = true; view.clearSelection(); flash("Kopiert · zum Einfügen den Finger auf die Stelle halten") } }
                LabeledBtn(Glyph.CUT, "Ausschneiden") { view.copySelection(cut = true) { canPaste = true; flash("Ausgeschnitten · zum Einfügen den Finger auf die Stelle halten") } }
                LabeledBtn(Glyph.TRASH, "Löschen") { view.deleteSelection() }
                LabeledBtn(Glyph.LAPTOP, "Laptop") { sendSelectionToLaptop() }
                LabeledBtn(Glyph.TEXT, "LaTeX") { recognizeSelection() }
                // Handwriting next to a graph (f(x) = x², y = 2x + 1) goes into it as a new row.
                val intoGraph = selection?.let { s -> !s.multi && nearestPlot(s.sheetId, android.graphics.RectF(s.bounds)) != null } == true
                LabeledBtn(Glyph.PLOT, if (intoGraph) "In den Graphen" else "Graph") { graphFromSelection() }
                selection?.takeIf { !it.multi }?.let { sel ->
                    LabeledBtn(Glyph.HEADING, "Überschrift") {
                        headingText = androidx.compose.ui.text.input.TextFieldValue("")
                        headingOld = null
                        headingFor = sel.sheetId to sel.ids
                        // Already a heading: its words; else the recogniser's reading. Selected: typing replaces it.
                        fun offer(t: String) { if (headingText.text.isEmpty()) headingText = androidx.compose.ui.text.input.TextFieldValue(t, androidx.compose.ui.text.TextRange(0, t.length)) }
                        model?.headingOf(sel.sheetId, sel.ids) { old ->
                            if (old != null) { headingOld = old.id; offer(old.text) }
                            else model?.readText(sel.sheetId, sel.ids) { t -> offer(t) }
                        }
                    }
                }
                Divider()
                IconBtn(Glyph.CHECK) { view.clearSelection() }
            }
        }

        /** Menu of a group, opened from its title in the page column. */
        @Composable
        fun GroupMenu() {
            val h = groupMenu ?: return
            Section("Gruppe · ${h.title.ifBlank { "Ohne Titel" }}")
            Item(Glyph.PEN, "Umbenennen …", "Titel, Reihenfolge, in/aus Gruppen") { menu = null; renaming = h.node to h.title }
            Item(if (h.collapsed) Glyph.DOWN else Glyph.UP_ARROW, if (h.collapsed) "Aufklappen" else "Einklappen", "${h.count} Seiten") {
                menu = null; model?.setCollapsed(h.node, !h.collapsed)
            }
            MenuDivider()
            Item(Glyph.PAGE, "Neue Seite in der Gruppe", "Am Ende der Gruppe") { menu = null; model?.addSheetIn(h.node, newSpec()) { view.showPage(it) } }
            Item(Glyph.GROUP, "Neue Untergruppe", "Am Ende der Gruppe") { menu = null; model?.addGroupIn(h.node, "Neue Gruppe") { node -> renaming = node to "Neue Gruppe" } }
            MenuDivider()
            Row(
                Modifier.fillMaxWidth().clickable {
                    if (confirmGroupDelete) { menu = null; confirmGroupDelete = false; deleteWithUndo(h.node, "Gruppe") } else confirmGroupDelete = true
                }.padding(horizontal = 16.dp, vertical = 10.dp),
                verticalAlignment = Alignment.CenterVertically
            ) {
                Box(Modifier.size(34.dp).clip(RoundedCornerShape(10.dp)).background(if (confirmGroupDelete) Ui.Red else Ui.Elevated), contentAlignment = Alignment.Center) { GlyphIcon(Glyph.TRASH, if (confirmGroupDelete) Ui.OnAccent else Ui.Chalk, 20.dp) }
                Column(Modifier.weight(1f).padding(start = 12.dp)) {
                    Text(if (confirmGroupDelete) "Wirklich löschen?" else "Gruppe löschen", color = if (confirmGroupDelete) Ui.Red else Ui.Chalk, fontSize = 14.sp, fontWeight = FontWeight.Medium)
                    Text(if (confirmGroupDelete) "Nochmal tippen: Gruppe und ${h.count} Seiten weg" else "Mit allen Seiten darin", color = Ui.Muted, fontSize = 11.sp)
                }
            }
        }

        /** "3 / 12": tap for the page menu (paper, rename, move). */
        /** Icon with a small label underneath (selection actions). */
        @Composable
        fun LabeledBtn(g: Glyph, label: String, onClick: () -> Unit) {
            Column(Modifier.clip(RoundedCornerShape(12.dp)).clickable(onClick = onClick).padding(horizontal = 6.dp, vertical = 2.dp), horizontalAlignment = Alignment.CenterHorizontally) {
                GlyphIcon(g, Ui.Chalk, 20.dp)
                Text(label, color = Ui.Muted, fontSize = 9.sp)
            }
        }

        /** Eraser: whole strokes or only what it touches; its own size, or automatic. */
        @Composable
        fun EraserMenu() {
            var area by this@NotebookActivity::eraserArea
            var auto by remember { mutableStateOf(Settings.eraserAuto) }
            var size by remember { mutableStateOf(Settings.eraserSize) }
            Section("Radierer")
            @Composable fun Choice(on: Boolean, title: String, sub: String, click: () -> Unit) {
                Row(Modifier.fillMaxWidth().clickable(onClick = click).padding(horizontal = 16.dp, vertical = 9.dp), verticalAlignment = Alignment.CenterVertically) {
                    Column(Modifier.weight(1f)) {
                        Text(title, color = Ui.Chalk, fontSize = 14.sp, fontWeight = FontWeight.Medium)
                        Text(sub, color = Ui.Muted, fontSize = 11.sp)
                    }
                    if (on) GlyphIcon(Glyph.CHECK, Ui.Red, 18.dp)
                }
            }
            Choice(!area, "Ganze Striche", "Ein Strich verschwindet, sobald du ihn berührst") { area = false; Settings.set("eraser_mode", "stroke") }
            Choice(area, "Nur Bereich", "Nur was unter dem Radierer liegt, wie ein echter Radiergummi") { area = true; Settings.set("eraser_mode", "area") }
            MenuDivider()
            Choice(auto, "Automatische Größe", "So breit wie der Stift, wächst mit Tempo und Fläche") { auto = !auto; Settings.set("eraser_auto", auto) }
            Row(Modifier.padding(horizontal = 16.dp).alpha(if (auto) 0.4f else 1f), verticalAlignment = Alignment.CenterVertically) {
                Box(Modifier.size(44.dp), contentAlignment = Alignment.Center) {
                    Box(Modifier.size((size * 1.6f).coerceIn(4f, 44f).dp).clip(CircleShape).border(1.5.dp, Ui.Chalk, CircleShape))
                }
                N0Slider(size, { size = it; Settings.set("eraser_size", it) }, 2f..60f, Modifier.weight(1f).padding(start = 8.dp), enabled = !auto)
                Text("%.1f mm".format(size * 0.3528f), color = Ui.Chalk, fontSize = 12.sp, fontFamily = Voice.metaFont, modifier = Modifier.width(64.dp).padding(start = 8.dp))
            }
            Note("Gilt auch für die Taste am Stift.")
        }

        @Composable
        fun PageChip(modifier: Modifier) {
            if (pageCount == 0) return
            Text("$currentPage / $pageCount", color = if (menu == "page") Ui.Black else Ui.Muted, fontSize = 12.sp, fontFamily = Voice.metaFont,
                modifier = modifier.anchor("page").clip(RoundedCornerShape(12.dp)).background(if (menu == "page") Ui.Chalk else Ui.Chip)
                    .clickable { pageFor = null; menu = if (menu == "page") null else "page" }.padding(horizontal = 10.dp, vertical = 5.dp))
        }

        /** The sheet in focus (main page, side sheet or board): its paper, and rename/move for pages. */
        @Composable
        fun PageMenu() {
            val p = pageFor?.let { n -> model?.placed?.firstOrNull { it.node == n } } ?: view.focusedSheet() ?: return
            Section(when {
                p.isBoard -> "Board"
                p.side < 0 -> "Seite ${p.label} · links"
                p.side > 0 -> "Seite ${p.label} · rechts"
                else -> "Seite ${p.number}"
            })
            if (readOnly) {
                // A guest who may only read changes nothing here.
                Note((if (p.isBoard) "" else Formats.describe(p.spec.width, p.spec.height) + " · ") + "Nur lesen: Papier und Format ändert, wer dich eingeladen hat.")
                return
            }
            PaperEditor(p.node, p.spec)
            if (p.side == 0) {
                MenuDivider()
                Item(Glyph.MENU, "Umbenennen, verschieben …", "Titel, Reihenfolge, Gruppe") {
                    menu = null
                    val t = rows.filterIsInstance<FfiOutlineRow.Sheet>().firstOrNull { it.node == p.node }?.title ?: ""
                    renaming = p.node to t
                }
            }
            MenuDivider()
            val placed = model?.placed.orEmpty()
            val what = when { p.isBoard -> "Board"; p.side != 0 -> "Seitenblatt"; else -> "Seite" }
            if (p.side == 0 && placed.count { it.side == 0 } <= 1) {
                Note("Die letzte Seite eines Notizbuchs bleibt.")
            } else {
                var sure by remember(p.node) { mutableStateOf(false) }
                val sides = if (p.side == 0) placed.count { it.mainNode == p.node && it.side != 0 } else 0
                Item(Glyph.TRASH, if (sure) "Wirklich löschen?" else "$what löschen",
                    if (sure) "Nochmal tippen" + (if (sides == 1) " · mit 1 Seitenblatt" else if (sides > 1) " · mit $sides Seitenblättern" else "")
                    else "Danach kurz rückgängig zu machen") {
                    if (!sure) sure = true else { menu = null; deleteWithUndo(p.node, what) }
                }
            }
        }

        /** Paper and format of one sheet, changed right away; "für alle" applies both to every plain page. */
        @Composable
        fun PaperEditor(node: String, spec: uniffi.n0te_ffi.FfiSheetSpec) {
            when {
                spec.pdfBlob != null -> { Note("PDF-Seite: Hintergrund und Format (${Formats.describe(spec.width, spec.height)}) kommen aus der PDF."); return }
                spec.width <= 0f || spec.height <= 0f -> { Note("Ein Board ist unendlich groß, mit Punktraster."); return }
            }
            val text = spec.background == FfiBackground.TEXT
            var bg by remember(node) { mutableStateOf(spec.background) }
            var spacing by remember(node) { mutableStateOf(spec.spacing.takeIf { it >= 4f } ?: Settings.spacingFor(spec.background)) }
            var size by remember(node) { mutableStateOf(spec.width to spec.height) }
            if (!text) {
                Section("Papier")
                Row(Modifier.padding(horizontal = 14.dp, vertical = 4.dp), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    PAPERS.forEach { (b, label) ->
                        PaperChoice(b, label, bg == b) {
                            if (b != bg) { bg = b; spacing = Settings.spacingFor(b); model?.setPaper(listOf(node), b, spacing) }
                        }
                    }
                }
                if (bg != FfiBackground.BLANK) {
                    Row(Modifier.padding(horizontal = 16.dp), verticalAlignment = Alignment.CenterVertically) {
                        Text("Abstand", color = Ui.Chalk, fontSize = 13.sp, modifier = Modifier.width(70.dp))
                        N0Slider(spacing, { spacing = it }, 8f..40f, Modifier.weight(1f), onDone = { model?.setPaper(listOf(node), bg, spacing) })
                        Text(mmText(spacing), color = Ui.Chalk, fontSize = 12.sp, fontFamily = Voice.metaFont, modifier = Modifier.width(64.dp).padding(start = 10.dp))
                    }
                }
            }
            Section("Format")
            FormatPicker(size.first, size.second, Modifier.padding(start = 16.dp, end = 16.dp, top = 2.dp, bottom = 6.dp)) { w, h ->
                resize(listOf(node), w, h) { size = w to h }
            }
            Item(Glyph.PAGE, "Für alle Seiten übernehmen", if (text) "Dieses Format für jede Seite (keine PDF-Seiten)" else "Papier und Format für jede Seite (keine PDF-Seiten)") {
                menu = null
                model?.let { m ->
                    m.allPlainSheetNodes { nodes ->
                        // A text page passes on its format only: its "paper" would turn every page into text.
                        if (!text) m.setPaper(nodes, bg, spacing)
                        resize(nodes, size.first, size.second) { flash("${if (text) "Format" else "Papier und Format"} für ${nodes.size} Seiten übernommen") }
                    }
                }
            }
        }

        /** A line width as pens are sold: "0,35 mm", "1,2 mm". */
        fun penMm(pt: Float): String { val mm = pt * 0.3528f; return if (mm < 1f) "%.2f mm".format(mm) else "%.1f mm".format(mm) }

        /** A size change waiting for "Trotzdem ändern": what would stick out, and the change itself. */
        var sizeWarning by mutableStateOf<Pair<uniffi.n0te_ffi.FfiOutside, () -> Unit>?>(null)

        /** Turns or resizes [nodes]; asks first when strokes, images or text would end up past the edge. */
        fun resize(nodes: List<String>, w: Float, h: Float, done: () -> Unit = {}) {
            val m = model ?: return
            m.outside(nodes, w, h) { o ->
                val apply = { m.setSize(nodes, w, h) { done() } }
                if (o == null || o.pages == 0u) apply() else sizeWarning = o to apply
            }
        }

        @Composable
        fun SizeWarning(o: uniffi.n0te_ffi.FfiOutside, apply: () -> Unit) {
            androidx.compose.ui.window.Dialog(onDismissRequest = { sizeWarning = null }) {
                N0teType {
                    Column(Modifier.width(480.dp).panel().padding(24.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                        Heading("Nicht alles passt auf die Seite", 20.sp)
                        val where = if (o.pages > 1u) " (auf ${o.pages} Seiten)" else ""
                        Text("${o.text} ${if (o.strokes + o.images + o.texts == 1u) "ragt" else "ragen"} dann über den Rand$where und " +
                            "${if (o.strokes + o.images + o.texts == 1u) "wird" else "werden"} abgeschnitten, auch im PDF.",
                            color = Ui.Chalk, fontSize = 15.sp, lineHeight = 21.sp)
                        Text("Gelöscht wird nichts: wird die Seite wieder größer, ist alles wieder da.", color = Ui.Muted, fontSize = 13.sp)
                        Row(Modifier.fillMaxWidth().padding(top = 4.dp), horizontalArrangement = Arrangement.End) {
                            Btn("Abbrechen", BtnKind.QUIET) { sizeWarning = null }
                            Spacer(Modifier.width(8.dp))
                            Btn("Trotzdem ändern", BtnKind.PRIMARY) { sizeWarning = null; apply() }
                        }
                    }
                }
            }
        }

        /** A paper spacing as you'd measure it: "5,0 mm" (1 pt = 0.3528 mm). */
        fun mmText(pt: Float) = "%.1f mm".format(pt * 0.3528f)

        @Composable
        fun Note(text: String) {
            Text(text, color = Ui.Muted, fontSize = 12.sp, modifier = Modifier.padding(horizontal = 18.dp, vertical = 8.dp))
        }

        // --- guests ---------------------------------------------------------------------------

        /** As a guest with read-only rights: nothing here changes the pages. */
        var readOnly by mutableStateOf(false)
        /** Which page each guest looks at (from their presence). */
        var guestPages by mutableStateOf<Map<String, String>>(emptyMap())
        var guestsOpen by mutableStateOf(false)

        fun applyReadOnly() {
            val id = nbId
            readOnly = SyncHub.joinedRole(id)?.role == uniffi.n0te_ffi.FfiRole.READ
            view.readOnly = readOnly
            if (readOnly && loupe) { view.closeLoupe(); loupe = false }
        }

        @Composable
        fun ReadOnlyNote() {
            val id = nbId
            val j = SyncHub.joinedRole(id)
            Row(Modifier.padding(horizontal = 12.dp), verticalAlignment = Alignment.CenterVertically) {
                GlyphIcon(Glyph.LOCK, Ui.Muted, 18.dp)
                Text("Nur lesen · geteilt von ${j?.ownerName ?: "jemandem"}", color = Ui.Chalk, fontSize = 14.sp, modifier = Modifier.padding(start = 8.dp))
            }
        }

        /** Owner: "● 2 Gäste" (who's in this notebook); guest: whose notebook it is. Tap: the overview. */
        @Composable
        fun GuestChip() {
            // Our rights can arrive after opening (the engine's status) or change: follow them.
            androidx.compose.runtime.LaunchedEffect(SyncHub.joined) { applyReadOnly() }
            val id = nbId
            val here = SyncHub.guests.filter { it.notebook == id }
            val host = SyncHub.joinedRole(id)
            // Someone at the door: the chip says so in the signal colour.
            val knocking = SyncHub.waiting.filter { it.notebook == id }
            if (knocking.isNotEmpty()) {
                Text(Voice.cased(if (knocking.size == 1) "● ${knocking[0].name} wartet" else "● ${knocking.size} warten"), color = Ui.OnAccent, fontSize = 12.sp, fontFamily = Voice.metaFont, fontWeight = FontWeight.Bold,
                    modifier = Modifier.anchor("guests").clip(RoundedCornerShape(12.dp)).background(Ui.Red).clickable { menu = if (menu == "guests") null else "guests" }.padding(horizontal = 10.dp, vertical = 5.dp))
                return
            }
            val text = when {
                host != null -> (if (host.online) "● " else "○ ") + "Gast bei ${host.ownerName}"
                here.size == 1 -> "● ${here[0].name}"
                here.isNotEmpty() -> "● ${here.size} Gäste"
                else -> return
            }
            val open = menu == "guests"
            Text(Voice.cased(text), color = if (open) Ui.Black else if (host == null || host.online) Ui.Chalk else Ui.Muted, fontSize = 12.sp, fontFamily = Voice.metaFont,
                modifier = Modifier.anchor("guests").clip(RoundedCornerShape(12.dp)).background(if (open) Ui.Chalk else Ui.Chip).clickable { menu = if (open) null else "guests" }.padding(horizontal = 10.dp, vertical = 5.dp))
        }

        /** Last known view of every device that showed one (jump to a guest / the owner). */
        val lastViews = HashMap<String, Pair<String, android.graphics.RectF>>()

        /** Runs an engine call in the background; failures show as a short message. */
        fun guestCall(fail: String, then: () -> Unit = {}, call: (uniffi.n0te_ffi.SyncEngine) -> Unit) {
            val e = SyncHub.engine ?: return flash("Sync startet noch …")
            N0te.background({ call(e) }) { r ->
                r.onFailure { flash("$fail: ${it.message}") }
                SyncHub.refreshGuests()
                then()
            }
        }

        /** Brings one guest (or all, [device] null) to where I look. */
        fun summon(device: String?) {
            val id = nbId
            val (sheet, rect) = view.currentView() ?: return
            menu = null
            guestCall("Holen fehlgeschlagen") { it.summon(id, device, sheet, rect.toList()) }
            flash(if (device == null) "Alle Gäste kommen zu dir" else "${SyncHub.nameOf(device)} kommt zu dir")
        }

        /** One row of the quick menu: title, small line, and an optional button on the right. */
        @Composable
        fun MenuRow(title: String, sub: String?, onClick: (() -> Unit)?, trailing: @Composable (() -> Unit)? = null) {
            Row(
                Modifier.fillMaxWidth().then(if (onClick != null) Modifier.clickable(onClick = onClick) else Modifier).padding(start = 18.dp, end = 12.dp, top = 8.dp, bottom = 8.dp),
                verticalAlignment = Alignment.CenterVertically
            ) {
                Column(Modifier.weight(1f)) {
                    Text(title, color = Ui.Chalk, fontSize = 14.sp, fontWeight = FontWeight.Medium)
                    if (sub != null) Text(sub, color = Ui.Muted, fontSize = 11.sp)
                }
                trailing?.invoke()
            }
        }

        /**
         * The guests chip's menu — everyday things only. Owner: who waits (let in), who is here
         * (tap: look where they are; Holen: bring them to me), all to me, then the window for
         * invites. Guest: jump to the owner, follow the owner, show the owner's frame.
         */
        @Composable
        fun GuestsMenu() {
            // Its switches are settings (not state): read the counter they bump, so a tap redraws.
            @Suppress("UNUSED_VARIABLE") val ticked = tick
            val id = nbId
            val host = SyncHub.joinedRole(id)
            if (host != null) {
                val name = host.ownerName
                Section("Gast bei $name · " + if (host.role == uniffi.n0te_ffi.FfiRole.READ_WRITE) "schreiben" else "nur lesen")
                MenuRow("Zu $name springen", "Einmal dorthin, wo $name gerade schaut", {
                    menu = null
                    val last = lastViews[host.owner]
                    if (last == null || !view.lookAt(last.first, last.second)) flash(if (host.online) "$name zeigt die Ansicht gerade nicht" else "$name ist gerade nicht verbunden")
                })
                val follows = following == host.owner
                MenuRow("$name folgen", "Deine Ansicht geht mit · selbst verschieben beendet es", {
                    menu = null
                    if (follows) { following = null; view.followDevice = null }
                    else {
                        following = host.owner; view.followDevice = host.owner
                        lastViews[host.owner]?.let { view.lookAt(it.first, it.second) }
                    }
                }) { dev.n0bridge.n0te.Switch(follows) }
                MenuRow("Rahmen von $name zeigen", null, {
                    Settings.set("guest_frames", !Settings.guestFrames)
                    if (!Settings.guestFrames) view.hideOthers()
                    tick++
                }) { dev.n0bridge.n0te.Switch(Settings.guestFrames) }
                return
            }
            val knocking = SyncHub.waiting.filter { it.notebook == id }
            val here = SyncHub.guests.filter { it.notebook == id }
            knocking.forEach { g ->
                MenuRow(g.name, "möchte herein", null) {
                    Btn("Nein", BtnKind.QUIET) { guestCall("Ablehnen fehlgeschlagen") { it.admit(g.device, false) } }
                    Spacer(Modifier.width(4.dp))
                    Btn("Herein", BtnKind.PRIMARY) { guestCall("Hereinlassen fehlgeschlagen") { it.admit(g.device, true) } }
                }
            }
            if (here.isNotEmpty()) Section(if (here.size == 1) "1 Gast da" else "${here.size} Gäste da")
            here.forEach { g ->
                val page = guestPages[g.device]?.let { sid -> model?.placed?.firstOrNull { it.sheetId == sid }?.let { p -> "Seite ${p.number}" } }
                MenuRow(g.name, listOfNotNull(if (g.write) "schreibt" else "liest", page).joinToString(" · "), {
                    menu = null
                    lastViews[g.device]?.let { view.lookAt(it.first, it.second) } ?: flash("${g.name} zeigt die Ansicht gerade nicht")
                }) { Btn("Holen", BtnKind.QUIET) { summon(g.device) } }
            }
            if (here.size > 1) MenuRow("Alle zu mir holen", "Alle sehen, was du gerade siehst", { summon(null) })
            MenuDivider()
            MenuRow("Einladen und verwalten …", null, { menu = null; guestsOpen = true })
        }

        /**
         * The guests window: who is in (remove one), invites as short lines — each opens to its
         * settings (rights, validity, approval, one device, PIN, removed devices, revoke) — and the
         * two view switches. Everyday actions are in the chip's menu.
         */
        @Composable
        fun GuestsDialog() {
            // Its switches are settings (not state): read the counter they bump, so a tap redraws.
            @Suppress("UNUSED_VARIABLE") val ticked = tick
            val id = nbId
            val engine = SyncHub.engine
            var invites by remember { mutableStateOf<List<uniffi.n0te_ffi.FfiInvite>>(emptyList()) }
            var removing by remember { mutableStateOf<String?>(null) }
            var expanded by remember { mutableStateOf<String?>(null) }
            var inviting by remember { mutableStateOf(false) }
            var tickLocal by remember { mutableIntStateOf(0) }
            androidx.compose.runtime.LaunchedEffect(tickLocal, SyncHub.guests, SyncHub.waiting) {
                val e = engine ?: return@LaunchedEffect
                invites = kotlinx.coroutines.withContext(kotlinx.coroutines.Dispatchers.IO) { runCatching { e.listInvites() }.getOrDefault(emptyList()) }.filter { it.notebook == id && !it.revoked }
            }
            fun act(fail: String, call: (uniffi.n0te_ffi.SyncEngine) -> Unit) = guestCall(fail, { tickLocal++ }, call)
            val host = SyncHub.joinedRole(id)
            androidx.compose.ui.window.Dialog(onDismissRequest = { guestsOpen = false }) {
                N0teType {
                    Column(Modifier.width(560.dp).panel().padding(24.dp).verticalScroll(androidx.compose.foundation.rememberScrollState()), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                        Row(verticalAlignment = Alignment.CenterVertically) {
                            Heading("Gäste", 22.sp)
                            Spacer(Modifier.weight(1f))
                            Btn("Fertig", BtnKind.QUIET) { guestsOpen = false }
                        }
                        if (host != null) {
                            Text("Du bist Gast bei ${host.ownerName} · " + if (host.role == uniffi.n0te_ffi.FfiRole.READ_WRITE) "du darfst mitschreiben" else "nur lesen", color = Ui.Chalk, fontSize = 15.sp)
                            Text("Springen, folgen und den Rahmen findest du unten rechts im Chip.", color = Ui.Muted, fontSize = 13.sp)
                            return@Column
                        }
                        // Who is in, compact; removing asks once more.
                        val knocking = SyncHub.waiting.filter { it.notebook == id }
                        knocking.forEach { g ->
                            Row(verticalAlignment = Alignment.CenterVertically) {
                                Text("${g.name} möchte herein", color = Ui.Chalk, fontSize = 15.sp, modifier = Modifier.weight(1f))
                                Btn("Nein", BtnKind.QUIET) { act("Ablehnen fehlgeschlagen") { it.admit(g.device, false) } }
                                Spacer(Modifier.width(6.dp))
                                Btn("Herein", BtnKind.PRIMARY) { act("Hereinlassen fehlgeschlagen") { it.admit(g.device, true) } }
                            }
                        }
                        val here = SyncHub.guests.filter { it.notebook == id }
                        if (here.isEmpty() && knocking.isEmpty()) Text("Gerade ist niemand da.", color = Ui.Muted, fontSize = 13.sp)
                        here.forEach { g ->
                            Row(verticalAlignment = Alignment.CenterVertically) {
                                Box(Modifier.size(8.dp).clip(androidx.compose.foundation.shape.CircleShape).background(Ui.Red))
                                Text(g.name, color = Ui.Chalk, fontSize = 15.sp, modifier = Modifier.padding(start = 10.dp))
                                Text(if (g.write) "  schreibt" else "  liest", color = Ui.Muted, fontSize = 13.sp, modifier = Modifier.weight(1f))
                                val sure = removing == g.device
                                Btn(if (sure) "Wirklich entfernen?" else "Entfernen", if (sure) BtnKind.PRIMARY else BtnKind.QUIET) {
                                    if (!sure) removing = g.device
                                    else { removing = null; act("Entfernen fehlgeschlagen") { it.removeGuest(g.device) }; flash("${g.name} ist entfernt") }
                                }
                            }
                        }
                        Box(Modifier.fillMaxWidth().height(1.dp).background(Ui.Border))
                        // Invites: one line each; tap opens the settings.
                        invites.forEach { inv ->
                            val rw = inv.role == uniffi.n0te_ffi.FfiRole.READ_WRITE
                            val open = expanded == inv.id
                            val until = when {
                                inv.expires == 0uL -> "ohne Ablauf"
                                inv.expires.toLong() < System.currentTimeMillis() / 1000 -> "abgelaufen"
                                else -> "bis " + java.text.SimpleDateFormat("d. MMM", java.util.Locale.GERMAN).format(java.util.Date(inv.expires.toLong() * 1000))
                            }
                            val extras = listOfNotNull(if (inv.approve) "bestätigen" else null, if (inv.single) "ein Gerät" else null, inv.pin?.let { "PIN $it" })
                            Column(Modifier.fillMaxWidth().group()) {
                                Row(Modifier.fillMaxWidth().clickable { expanded = if (open) null else inv.id }.padding(horizontal = 14.dp, vertical = 10.dp), verticalAlignment = Alignment.CenterVertically) {
                                    Column(Modifier.weight(1f)) {
                                        Text("Link · " + if (rw) "lesen & schreiben" else "nur lesen", color = Ui.Chalk, fontSize = 14.sp, fontWeight = FontWeight.Medium)
                                        Text((listOf(until) + extras).joinToString(" · "), color = Ui.Muted, fontSize = 12.sp)
                                    }
                                    Btn("Teilen", BtnKind.QUIET) { startActivity(Intent.createChooser(Intent(Intent.ACTION_SEND).setType("text/plain").putExtra(Intent.EXTRA_TEXT, inv.link), "Einladung teilen")) }
                                    GlyphIcon(if (open) Glyph.UP_ARROW else Glyph.DOWN, Ui.Muted, 18.dp)
                                }
                                if (open) Column(Modifier.padding(start = 14.dp, end = 14.dp, bottom = 12.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                                    Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                                        Choice("Nur lesen", !rw) { if (rw) setRole(inv.id, false) { tickLocal++ } }
                                        Choice("Lesen & schreiben", rw) { if (!rw) setRole(inv.id, true) { tickLocal++ } }
                                    }
                                    Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                                        listOf(1 to "1 Tag", 7 to "7 Tage", 30 to "30 Tage", 0 to "Unbegrenzt").forEach { (days, label) ->
                                            Choice(label, days == 0 && inv.expires == 0uL) {
                                                act("Ablauf ändern fehlgeschlagen") { it.setExpiry(inv.id, if (days == 0) 0uL else (System.currentTimeMillis() / 1000 + days * 86_400L).toULong()) }
                                            }
                                        }
                                    }
                                    @Composable fun Option(title: String, on: Boolean, flip: () -> Unit) {
                                        Row(Modifier.fillMaxWidth().clickable(onClick = flip).padding(vertical = 2.dp), verticalAlignment = Alignment.CenterVertically) {
                                            Text(title, color = Ui.Chalk, fontSize = 14.sp, modifier = Modifier.weight(1f))
                                            dev.n0bridge.n0te.Switch(on)
                                        }
                                    }
                                    Option("Neue Gäste erst bestätigen", inv.approve) { act("Ändern fehlgeschlagen") { it.setInviteOptions(inv.id, !inv.approve, inv.single) } }
                                    Option("Nur für ein Gerät", inv.single) { act("Ändern fehlgeschlagen") { it.setInviteOptions(inv.id, inv.approve, !inv.single) } }
                                    Option(inv.pin?.let { "PIN $it · getrennt vom Link sagen" } ?: "PIN verlangen", inv.pin != null) {
                                        act("PIN ändern fehlgeschlagen") { it.setPin(inv.id, if (inv.pin != null) null else (java.security.SecureRandom().nextInt(9000) + 1000).toString()) }
                                    }
                                    inv.banned.forEach { b ->
                                        Row(verticalAlignment = Alignment.CenterVertically) {
                                            Text("Entfernt: ${b.name}", color = Ui.Muted, fontSize = 13.sp, modifier = Modifier.weight(1f))
                                            Btn("Wieder zulassen", BtnKind.QUIET) { act("Zulassen fehlgeschlagen") { it.allowAgain(inv.id, b.device) } }
                                        }
                                    }
                                    Row {
                                        Spacer(Modifier.weight(1f))
                                        Btn("Link widerrufen", BtnKind.QUIET) { act("Widerrufen fehlgeschlagen") { it.revoke(inv.id) } }
                                    }
                                }
                            }
                        }
                        // New link: first what guests may do.
                        if (!inviting) Btn("+ Einladen") { inviting = true }
                        else Row(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
                            Text("Gäste dürfen", color = Ui.Muted, fontSize = 13.sp)
                            Btn("Nur lesen") { inviting = false; invite(rw = false) { tickLocal++ } }
                            Btn("Lesen & schreiben") { inviting = false; invite(rw = true) { tickLocal++ } }
                        }
                        Box(Modifier.fillMaxWidth().height(1.dp).background(Ui.Border))
                        @Composable fun ViewSwitch(title: String, on: Boolean, flip: () -> Unit) {
                            Row(Modifier.fillMaxWidth().clickable(onClick = flip).padding(vertical = 2.dp), verticalAlignment = Alignment.CenterVertically) {
                                Text(title, color = Ui.Chalk, fontSize = 14.sp, modifier = Modifier.weight(1f))
                                dev.n0bridge.n0te.Switch(on)
                            }
                        }
                        ViewSwitch("Zeigen, wo Gäste hinschauen", Settings.guestFrames) { Settings.set("guest_frames", !Settings.guestFrames); if (!Settings.guestFrames) view.hideOthers(); tick++ }
                        ViewSwitch("Gäste sehen, wo ich hinschaue", Settings.shareView) { SyncHub.setShareView(!Settings.shareView); tick++ }
                    }
                }
            }
        }

        fun setRole(invite: String, write: Boolean, then: () -> Unit) {
            val e = SyncHub.engine ?: return
            N0te.background({ e.setRole(invite, if (write) uniffi.n0te_ffi.FfiRole.READ_WRITE else uniffi.n0te_ffi.FfiRole.READ) }) { r ->
                r.onFailure { flash("Rechte ändern fehlgeschlagen: ${it.message}") }
                    .onSuccess { flash(if (write) "Gäste dürfen jetzt mitschreiben" else "Gäste dürfen jetzt nur lesen") }
                then()
            }
        }

        /** What was just deleted ("Seite gelöscht"), with Rückgängig for a few seconds. */
        var undoLabel by mutableStateOf<String?>(null)
        val clearUndo = Runnable { undoLabel = null }

        /** The page that was deleted last (shown again after Rückgängig). */
        var undoSheet: String? = null

        fun deleteWithUndo(node: String, what: String) {
            val m = model ?: return
            // The whole outline first (folded groups too): the last page stays, from every menu
            // (only the page menu checked it), and a group says "Gruppe gelöscht".
            m.allRows { rows ->
                if (!canDelete(rows, node)) return@allRows flash("Die letzte Seite eines Notizbuchs bleibt.")
                deleteNow(m, node, if (rows.any { it is FfiOutlineRow.Group && it.node == node }) "Gruppe" else what)
            }
        }

        private fun deleteNow(m: NotebookModel, node: String, what: String) {
            // Afterwards: a side page's main page, else the page before (or after) — not the empty spot.
            val gone = m.placed.firstOrNull { it.node == node }
            val mains = m.placed.filter { it.side == 0 }
            val next = when {
                gone == null -> null
                gone.side != 0 -> mains.firstOrNull { it.node == gone.mainNode }
                else -> mains.indexOfFirst { it.node == node }.let { i -> mains.getOrNull(i - 1) ?: mains.getOrNull(i + 1) }
            }
            undoSheet = gone?.sheetId
            m.deleteNode(node) { next?.let { view.showPage(it.sheetId) } }
            undoLabel = "$what gelöscht"
            N0te.main.removeCallbacks(clearUndo)
            N0te.main.postDelayed(clearUndo, 8000)
        }

        fun flash(text: String) {
            status = text
            N0te.main.removeCallbacks(clearStatus)
            N0te.main.postDelayed(clearStatus, flashMs(text))
        }

        @Composable
        fun RenameDialog(node: String, old: String) {
            var text by remember(node) { mutableStateOf(androidx.compose.ui.text.input.TextFieldValue(old, androidx.compose.ui.text.TextRange(0, old.length))) }
            var sure by remember(node) { mutableStateOf(false) }
            val focus = remember { androidx.compose.ui.focus.FocusRequester() }
            androidx.compose.runtime.LaunchedEffect(node) { focus.requestFocus() }
            Dialog(onDismissRequest = { renaming = null }) {
                Column(Modifier.width(400.dp).panel().padding(vertical = 14.dp)) {
                    Section("Titel")
                    Box(Modifier.padding(horizontal = 16.dp, vertical = 4.dp).fillMaxWidth().clip(RoundedCornerShape(12.dp)).background(Ui.Black).border(1.dp, Ui.Border, RoundedCornerShape(12.dp)).padding(12.dp)) {
                        if (text.text.isEmpty()) Text("Ohne Titel", color = Ui.Muted, fontSize = 15.sp)
                        BasicTextField(text, { text = it }, singleLine = true, textStyle = TextStyle(color = Ui.Chalk, fontSize = 15.sp), cursorBrush = SolidColor(Ui.Red),
                            keyboardOptions = androidx.compose.foundation.text.KeyboardOptions(imeAction = androidx.compose.ui.text.input.ImeAction.Done),
                            keyboardActions = androidx.compose.foundation.text.KeyboardActions(onDone = { if (text.text.trim() != old) model?.rename(node, text.text.trim()); renaming = null }),
                            modifier = Modifier.fillMaxWidth().focusRequester(focus))
                    }
                    // The middle scrolls: with the keyboard up (it opens for the title) OK and Löschen
                    // were below the screen's edge on a tablet held sideways.
                    Column(Modifier.weight(1f, fill = false).verticalScroll(androidx.compose.foundation.rememberScrollState())) {
                        rows.filterIsInstance<FfiOutlineRow.Sheet>().firstOrNull { it.node == node }?.let { sheet ->
                            MenuDivider()
                            PaperEditor(node, sheet.spec)
                        }
                        MenuDivider()
                        Section("Verschieben")
                        Item(Glyph.UP_ARROW, "Nach oben") { model?.move(node, "up"); renaming = null }
                        Item(Glyph.DOWN_ARROW, "Nach unten") { model?.move(node, "down"); renaming = null }
                        Item(Glyph.GROUP, "In die Gruppe darüber", "Ans Ende der Gruppe direkt über diesem Eintrag") { model?.move(node, "into"); renaming = null }
                        Item(Glyph.LEFT, "Aus der Gruppe heraus", "Hinter die Gruppe") { model?.move(node, "out"); renaming = null }
                    }
                    MenuDivider()
                    Row(Modifier.padding(horizontal = 16.dp).fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        Pill(if (sure) "Wirklich löschen" else "Löschen", primary = sure) { if (sure) { deleteWithUndo(node, "Seite"); renaming = null } else sure = true }
                        Spacer(Modifier.weight(1f))
                        Btn("Abbrechen", BtnKind.QUIET) { renaming = null }
                        Pill("OK", primary = !sure) { if (text.text.trim() != old) model?.rename(node, text.text.trim()); renaming = null }
                    }
                }
            }
        }

        @Composable
        fun IconBtn(g: Glyph, selected: Boolean = false, enabled: Boolean = true, key: String? = null, onLongClick: (() -> Unit)? = null, onClick: () -> Unit) {
            Box(
                Modifier.let { if (key != null) it.anchor(key) else it }.size(38.dp).clip(CircleShape).background(if (selected) Ui.Chalk else Color.Transparent)
                    .toolTap(enabled = enabled, onLongClick = onLongClick, onClick = onClick),
                contentAlignment = Alignment.Center
            ) {
                GlyphIcon(g, tint = when { selected -> Ui.Black; enabled -> Ui.Chalk; else -> Ui.Muted.copy(alpha = 0.5f) })
            }
        }

        @Composable
        fun Divider() {
            Box(Modifier.padding(horizontal = 6.dp).width(1.dp).height(22.dp).background(Ui.Border))
        }
    }
}

/** The outline's rows: where depth 0 starts and how far each level goes in (dp). */
private const val OUT_BASE = 12
private const val OUT_INDENT = 18

/** The outline's tree: a thin line down from each group the row is in (under the group's arrow). */
private fun Modifier.treeGuides(depth: Int): Modifier = if (depth <= 0) this else drawBehind {
    for (d in 0 until depth) {
        val x = (OUT_BASE + d * OUT_INDENT + 8).dp.toPx()
        drawLine(Ui.Border, androidx.compose.ui.geometry.Offset(x, 0f), androidx.compose.ui.geometry.Offset(x, size.height), 1.dp.toPx())
    }
}

/** A small ">" (turned down when its group is open). */
@Composable
private fun Chevron(tint: Color, size: androidx.compose.ui.unit.Dp) {
    androidx.compose.foundation.Canvas(Modifier.size(size)) {
        val k = this.size.minDimension / 24f
        val p = androidx.compose.ui.graphics.Path().apply { moveTo(9f * k, 5f * k); lineTo(16f * k, 12f * k); lineTo(9f * k, 19f * k) }
        drawPath(p, tint, style = androidx.compose.ui.graphics.drawscope.Stroke(2.4f * k, cap = androidx.compose.ui.graphics.StrokeCap.Round, join = androidx.compose.ui.graphics.StrokeJoin.Round))
    }
}
