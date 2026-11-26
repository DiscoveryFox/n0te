package dev.n0bridge.n0te

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
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
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import uniffi.n0te_ffi.FfiBackground

/** SETTINGS: switch n0te's features on or off and tune input, view and paper. */
@OptIn(androidx.compose.foundation.layout.ExperimentalLayoutApi::class)
class SettingsActivity : ComponentActivity() {

    private var tick by mutableIntStateOf(0)
    private val sectionState = mutableStateOf(lastSection)
    private var section: String
        get() = sectionState.value
        set(v) { sectionState.value = v; lastSection = v }
    private val themeId = Ui.theme.id
    private var pencilVersion by mutableIntStateOf(0)

    companion object {
        /** The open section survives a design switch (the activity is re-created). */
        private var lastSection = "design"
        /** Open at this section ("news": the changelog). */
        const val EXTRA_SECTION = "section"
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        Themes.applyWindow(this)
        // Only when opened (not again when rebuilt for a new design); a section that exists.
        if (savedInstanceState == null) intent.getStringExtra(EXTRA_SECTION)?.takeIf { s -> sections.any { it.first == s } }?.let { section = it }
        Handwriting.onChange(handwritingListener)
        if (Settings.handwritingSearch) Handwriting.check(fetch = false)
        setContent { Screen() }
    }

    private val handwritingListener: () -> Unit = { tick++ }

    override fun onDestroy() {
        // Else every settings screen opened stayed in Handwriting's listeners for good.
        Handwriting.offChange(handwritingListener)
        super.onDestroy()
    }

    private val sections = listOf(
        "design" to "Design", "laptop" to "Laptop", "input" to "Stift & Eingabe", "pages" to "Seiten & Ansicht",
        "paper" to "Papier", "text" to "Text & Formeln", "toolbar" to "Werkzeugleiste", "updates" to "Updates", "diag" to "Diagnose", "news" to "Neuigkeiten", "about" to "Über n0te",
    )

    @Composable
    private fun Screen() = N0teType {
        @Suppress("UNUSED_VARIABLE") val t = tick
        Column(Modifier.fillMaxSize().background(Ui.Black).statusBarsPadding()) {
            Row(Modifier.fillMaxWidth().padding(horizontal = 20.dp, vertical = 14.dp), verticalAlignment = Alignment.CenterVertically) {
                Box(Modifier.size(44.dp).clip(CircleShape).clickable { finish() }, contentAlignment = Alignment.Center) { GlyphIcon(Glyph.BACK, Ui.Chalk) }
                Heading("Einstellungen", 26.sp, Modifier.padding(start = 10.dp))
            }
            Row(Modifier.fillMaxSize()) {
                // Sections, like a sidebar.
                Column(Modifier.width(250.dp).fillMaxHeight().padding(start = 16.dp, end = 8.dp, top = 6.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                    sections.forEach { (id, label) -> NavItem(label, section == id) { section = id } }
                }
                Column(
                    Modifier.weight(1f).fillMaxHeight().verticalScroll(rememberScrollState()).padding(start = 20.dp, end = 32.dp, top = 6.dp, bottom = 32.dp),
                    verticalArrangement = Arrangement.spacedBy(22.dp)
                ) {
                    when (section) {
                        // The designs are tiles themselves: no box around them.
                        "design" -> Card("Design", "Farben, Formen und Schrift. Exportierte PDFs bleiben weiß.", boxed = false) {
                            androidx.compose.foundation.layout.FlowRow(horizontalArrangement = Arrangement.spacedBy(12.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                                Themes.all.forEach { t -> StyleCard(t) }
                            }
                        }
                        "laptop" -> Card("Laptop", "Sync direkt zwischen Tablet und Laptop, ohne Server und ohne n0bridge.") { SyncCard() }
                        "input" -> {
                            Card("Eingabe") {
                                Toggle("pen_only", "Nur Stift (beim Öffnen)", "Finger verschieben und zoomen, zeichnen nie", Settings.penOnlyDefault)
                                Toggle("side_button_erases", "Stifttaste radiert", "Gedrückt halten zum Radieren", Settings.sideButtonErases)
                                Toggle("double_tap", "Doppeltippen passt die Seite ein", "Zweimal: Seitenbreite", Settings.doubleTapFit)
                                Toggle("shapes", "Formen erkennen", "Am Strichende still halten: Linie, Kreis, Rechteck …", Settings.shapes)
                                Toggle("arrows", "Pfeile erkennen", "Spitze an ein Linienende zeichnen und still halten: wird eine saubere Pfeilspitze", Settings.arrows)
                                Toggle("handwriting_search", "Handschrift durchsuchbar", handwritingNote(), Settings.handwritingSearch)
                                Toggle("scribble_erase", "Wegkritzeln löscht", "Schnelles Zickzack über Striche löscht sie (nicht mit dem Textmarker)", Settings.scribbleErase)
                                Toggle("write_flow", "Mitwandern beim Schreiben", "Hineingezoomt: den Stift im markierten Streifen rechts absetzen schiebt die Seite weiter; am Seitenrand eine Zeile tiefer (auf Linien und Kästchen)", Settings.writeFlow)
                                SliderRow("palm_mm", "Handballen ab", Settings.palmMm, 15f, 50f, { "%.0f mm".format(it) }, "Größere Berührungen werden ignoriert")
                            }
                            Card("Bleistift") {
                                PenSample(PenType.PENCIL, 0xFF111111.toInt(), Tools.widths[1], Modifier.padding(horizontal = 16.dp, vertical = 6.dp).fillMaxWidth().height(84.dp),
                                    alpha = PenType.PENCIL.inkAlpha(), version = pencilVersion)
                                SliderRow("pencil_grain", "Körnung", Settings.pencilGrain, 0f, 1f, PERCENT, "Links glatt, rechts rau: viel Papier sichtbar") { Pencil.refresh(); pencilVersion++ }
                                SliderRow("pencil_softness", "Weiche Kante", Settings.pencilSoftness, 0f, 1f, PERCENT, "Links gleichmäßig, rechts weicher Rand mit dunklem Kern bei Druck") { Pencil.refresh(); pencilVersion++ }
                                SliderRow("pencil_opacity", "Deckkraft", Settings.pencilOpacity, 0.3f, 1f, PERCENT, "Gilt für neue Striche") { pencilVersion++ }
                            }
                        }
                        "pages" -> {
                            Card("Zoom beim Öffnen", "Wie groß eine Seite erscheint, wenn du ein Notizbuch öffnest (auch im Zoom-Menü)") {
                                androidx.compose.foundation.layout.FlowRow(Modifier.padding(horizontal = 16.dp, vertical = 8.dp), horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                                    val d = Settings.pageScale
                                    listOf("width" to "Seitenbreite", "page" to "Ganze Seite", "75" to "75 %", "100" to "100 %", "150" to "150 %").forEach { (v, label) ->
                                        Choice(label, d == v) { Settings.set("page_scale", v); tick++ }
                                    }
                                }
                            }
                            Card("Seiten") {
                                Toggle("side_pages", "Seitenblätter links und rechts", "Neben jede Seite weitere Blätter legen", Settings.sidePages)
                                Toggle("handles", "Plus-Knöpfe an der Seite", "Neue Seite links, rechts und unten", Settings.handles)
                                if (Settings.handles) Column(Modifier.padding(horizontal = 16.dp, vertical = 6.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                                    Text("Plus-Knopf bestätigen", color = Ui.Chalk, fontSize = 15.sp)
                                    Text("Halten, bis sich der Ring um das Plus schließt: verhindert versehentliche Seiten. Sofort: Antippen genügt.", color = Ui.Muted, fontSize = 12.sp)
                                    androidx.compose.foundation.layout.FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                                        listOf(0f to "Sofort", 300f to "0,3 s", 500f to "0,5 s", 800f to "0,8 s", 1200f to "1,2 s").forEach { (v, l) ->
                                            Choice(l, kotlin.math.abs(Settings.handleHoldMs - v) < 1f, keepCase = true) { Settings.set("handle_hold_ms", v); tick++ }
                                        }
                                    }
                                }
                                Toggle("boards", "Boards (unendliche Fläche)", "Im Plus-Menü anbieten", Settings.boards)
                                Toggle("groups", "Gruppen", "Seiten zu einklappbaren Gruppen zusammenfassen", Settings.groups)
                            }
                            Card("Ansicht") {
                                Toggle("main_column", "Hauptspalte hervorheben", "Ein sanftes Band hinter den Hauptseiten, wenn es Seitenblätter gibt", Settings.mainColumn)
                                Toggle("lock_momentum", "Schwung bei Seitensperre", "Gesperrt und hineingezoomt: Wischen gleitet innerhalb der Seite weiter", Settings.lockMomentum)
                                Toggle("show_zoom", "Zoomstufe anzeigen", "Unten rechts, antippen zum Sperren", Settings.showZoom)
                                Toggle("page_numbers", "Seitenzahlen", "Neben jeder Seite", Settings.pageNumbers)
                                Toggle("minimap", "Minikarte im Board", "Beim Verschieben und Zoomen", Settings.minimap)
                                Toggle("hover_cursor", "Stift-Vorschau", "Kreis unter dem schwebenden Stift", Settings.hoverCursor)
                            }
                            Card("Abstände") {
                                SliderRow("page_gap_v", "Zwischen Seiten (unten)", Settings.pageGapV, 8f, 120f, MM, null)
                                SliderRow("page_gap_h", "Zu Seitenblättern (seitlich)", Settings.pageGapH, 8f, 120f, MM, null)
                            }
                        }
                        "paper" -> {
                            Card("Papier", "Für neue Notizbücher; ein Notizbuch behält sein Papier für neue Seiten") {
                                androidx.compose.foundation.layout.FlowRow(Modifier.padding(horizontal = 16.dp, vertical = 8.dp), horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                                    listOf(FfiBackground.BLANK to "Leer", FfiBackground.LINES to "Linien", FfiBackground.GRID to "Kästchen", FfiBackground.DOTS to "Punkte", FfiBackground.CORNELL to "Cornell")
                                        .forEach { (b, label) -> Choice(label, Settings.defaultPaper == b) { Settings.set("paper", b.name); tick++ } }
                                }
                                // Only the spacing of the paper chosen above.
                                when (Settings.defaultPaper) {
                                    FfiBackground.GRID -> SliderRow("grid_size", "Kästchengröße", Settings.gridSize, 8f, 40f, MM1, null)
                                    FfiBackground.DOTS -> SliderRow("dot_spacing", "Punktabstand", Settings.dotSpacing, 8f, 40f, MM1, null)
                                    FfiBackground.LINES, FfiBackground.CORNELL -> SliderRow("line_spacing", "Linienabstand", Settings.lineSpacing, 12f, 40f, MM1, null)
                                    else -> {}
                                }
                            }
                            Card("Format", "Für neue Notizbücher; jede Seite lässt sich später drehen oder ändern: unten rechts auf die Seitenzahl tippen") {
                                val (w, h) = Settings.newSize
                                FormatPicker(w, h, Modifier.padding(horizontal = 16.dp, vertical = 8.dp)) { nw, nh -> Settings.setNewSize(nw, nh); tick++ }
                            }
                        }
                        "text" -> Card("Text & Formeln") {
                            Toggle("formula_center", "Formeln zentrieren", "Eine Zeile nur mit \$\$…\$\$ steht in der Mitte", Settings.formulaCenter)
                        }
                        "toolbar" -> Card("Werkzeugleiste", "Welche Werkzeuge oben angeboten werden") {
                            Toggle("tool_marker", "Textmarker", null, Settings.toolMarker)
                            Toggle("tool_lasso", "Lasso", null, Settings.toolLasso)
                            Toggle("tool_ruler", "Lineal", null, Settings.toolRuler)
                            Toggle("tool_loupe", "Schreiblupe", "Groß schreiben, klein auf der Seite: ein Schreibfeld zeigt einen Rahmen der Seite vergrößert", Settings.toolLoupe)
                            Toggle("tool_laser", "Laserpointer", "Leuchtende Spur, verschwindet von selbst (wird nicht gespeichert)", Settings.toolLaser)
                            if (Settings.toolLaser) Column(Modifier.padding(horizontal = 16.dp, vertical = 6.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                                Text("Laserspur", color = Ui.Chalk, fontSize = 15.sp)
                                androidx.compose.foundation.layout.FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                                    listOf("short" to "Kurz", "long" to "Lang (4 s)", "keep" to "Bleibt beim Zeichnen").forEach { (v, l) ->
                                        Choice(l, Settings.laserTrail == v) { Settings.set("laser_trail", v); tick++ }
                                    }
                                }
                            }
                        }
                        "diag" -> {
                            // Write, hover, press the button: what the pen reports, drawn.
                            Card("Stift-Test", "Schreiben, schweben, die Taste drücken: die Linie wird so dick, wie fest du drückst.") {
                                var penTest by remember { mutableStateOf<PenTestView?>(null) }
                                androidx.compose.ui.viewinterop.AndroidView(
                                    factory = { PenTestView(it).also { v -> penTest = v } },
                                    modifier = Modifier.padding(horizontal = 16.dp, vertical = 8.dp).fillMaxWidth().height(340.dp).clip(RoundedCornerShape(minOf(Ui.theme.style.card, 12f).dp))
                                )
                                Row(Modifier.padding(horizontal = 16.dp, vertical = 6.dp)) {
                                    Btn("Leeren", BtnKind.QUIET) { penTest?.clear() }
                                }
                            }
                            Card("Messungen", "Für Fehlersuche: wie schnell dieses Tablet schreibt und rechnet.") {
                                Row(Modifier.padding(horizontal = 16.dp, vertical = 10.dp), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                                    Btn("Schreib-Verzögerung testen") { startActivity(android.content.Intent(this@SettingsActivity, InkSpikeActivity::class.java)) }
                                    Btn("Rechentempo messen", BtnKind.QUIET) { startActivity(android.content.Intent(this@SettingsActivity, MainActivity::class.java)) }
                                }
                                Text("${android.os.Build.MANUFACTURER} ${android.os.Build.MODEL} · Android ${android.os.Build.VERSION.RELEASE}", color = Ui.Muted, fontSize = 12.sp, fontFamily = Type.Mono,
                                    modifier = Modifier.padding(horizontal = 16.dp, vertical = 4.dp))
                            }
                        }
                        "updates" -> UpdateSettings()
                        "news" -> Changelog.all.forEachIndexed { i, e ->
                            Card("Version ${e.version} · ${e.date}" + if (i == 0) " · neueste" else "") {
                                Column(Modifier.padding(horizontal = 16.dp, vertical = 8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                                    Heading(e.title, 19.sp)
                                    e.items.forEach { ChangeItem(it) }
                                }
                            }
                        }
                        "about" -> Card("Über n0te") {
                            val version = runCatching { packageManager.getPackageInfo(packageName, 0).versionName }.getOrNull() ?: "?"
                            Row(Modifier.padding(horizontal = 16.dp, vertical = 10.dp), verticalAlignment = Alignment.CenterVertically) {
                                Wordmark(cell = 3.dp)
                                Text("Version $version", color = Ui.Muted, fontSize = 13.sp, fontFamily = Voice.metaFont, modifier = Modifier.padding(start = 16.dp))
                            }
                            Text("Schriften: Space Grotesk, Space Mono, Doto (SIL Open Font License).", color = Ui.Muted, fontSize = 12.sp, modifier = Modifier.padding(horizontal = 16.dp, vertical = 4.dp))
                        }
                    }
                }
            }
        }
    }

    /**
     * New versions from a GitHub repo's releases: which repo (a token for a private one), when
     * to look, a look now; a downloaded update is installed from here or the start screen.
     */
    @Composable
    private fun UpdateSettings() {
        var repo by remember { mutableStateOf(Settings.str("update_repo", Updater.DEFAULT_REPO)) }
        var token by remember { mutableStateOf(Settings.str("update_token", "")) }
        var mode by remember { mutableStateOf(Updater.mode) }
        var pre by remember { mutableStateOf(Updater.prerelease) }
        Card("Updates von GitHub", "n0te sucht neue Versionen in den Releases eines GitHub-Repos, lädt sie herunter und installiert sie, wenn du tippst.") {
            Column(Modifier.padding(horizontal = 16.dp, vertical = 10.dp), verticalArrangement = Arrangement.spacedBy(14.dp)) {
                Field(repo, "besitzer/repo", mono = true, label = "GitHub-Repo") { repo = it; Settings.set("update_repo", it.trim()) }
                Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
                    Caption("Zugangs-Token (nur für private Repos)")
                    SecretField(token, "github_pat_…") { token = it; Settings.set("update_token", it.trim()) }
                    Text("Bleibt nur auf diesem Tablet. Lesen der Releases reicht (Contents: read).", color = Ui.Muted, fontSize = 12.sp)
                }
                Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
                    Caption("Nachsehen")
                    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        listOf("open" to "Beim Öffnen", "periodic" to "Beim Öffnen und alle 6 Stunden", "manual" to "Nur von Hand").forEach { (v, l) ->
                            Choice(l, mode == v) { mode = v; Settings.set("update_mode", v) }
                        }
                    }
                }
                Row(Modifier.fillMaxWidth().clickable { pre = !pre; Settings.set("update_pre", if (pre) "1" else "0") }, verticalAlignment = Alignment.CenterVertically) {
                    Text("Auch Vorabversionen (Pre-Releases)", color = Ui.Chalk, fontSize = 15.sp, modifier = Modifier.weight(1f))
                    Switch(pre)
                }
                Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                    Btn("Jetzt nach Updates suchen", enabled = Updater.state !is Updater.State.Checking && Updater.state !is Updater.State.Downloading) { Updater.check(this@SettingsActivity, manual = true) }
                    (Updater.state as? Updater.State.Ready)?.let { r -> Btn("Version ${r.release.version} installieren", BtnKind.PRIMARY) { Updater.install(this@SettingsActivity) } }
                }
                val status = when (val st = Updater.state) {
                    is Updater.State.Idle -> "Installiert: ${Updater.current(this@SettingsActivity)}"
                    is Updater.State.Checking -> "Sucht …"
                    is Updater.State.UpToDate -> "Aktuell: ${st.version} ist die neueste Version"
                    is Updater.State.Downloading -> "Lädt ${st.release.version} …" + if (st.total > 0) " ${st.done * 100 / st.total} %" else ""
                    is Updater.State.Ready -> "${st.release.version} ist heruntergeladen und bereit"
                    is Updater.State.Installing -> "Wird installiert …"
                    is Updater.State.Failed -> st.message
                }
                Text(status, color = if (Updater.state is Updater.State.Failed) Ui.Red else Ui.Muted, fontSize = 13.sp, fontFamily = Voice.metaFont)
                Text("Die APK im Release muss mit demselben Schlüssel signiert sein wie diese App, sonst lehnt Android das Update ab.", color = Ui.Muted, fontSize = 12.sp)
            }
        }
    }

    /** A field whose text shows as dots until the eye is tapped. */
    @Composable
    private fun SecretField(value: String, hint: String, onChange: (String) -> Unit) {
        var show by remember { mutableStateOf(false) }
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Box(Modifier.weight(1f).clip(RoundedCornerShape(10.dp)).background(Ui.Black).border(1.dp, Ui.Border, RoundedCornerShape(10.dp)).padding(horizontal = 14.dp, vertical = 12.dp)) {
                if (value.isEmpty()) Text(hint, color = Ui.Muted, fontSize = 15.sp, fontFamily = Type.Mono)
                androidx.compose.foundation.text.BasicTextField(value, onChange, singleLine = true,
                    textStyle = androidx.compose.ui.text.TextStyle(color = Ui.Chalk, fontSize = 15.sp, fontFamily = Type.Mono),
                    cursorBrush = androidx.compose.ui.graphics.SolidColor(Ui.Red),
                    visualTransformation = if (show) androidx.compose.ui.text.input.VisualTransformation.None else androidx.compose.ui.text.input.PasswordVisualTransformation(),
                    keyboardOptions = androidx.compose.foundation.text.KeyboardOptions(autoCorrectEnabled = false, keyboardType = androidx.compose.ui.text.input.KeyboardType.Password),
                    modifier = Modifier.fillMaxWidth())
            }
            Btn(if (show) "Verbergen" else "Zeigen", BtnKind.QUIET) { show = !show }
        }
    }

    @Composable
    private fun NavItem(label: String, on: Boolean, onClick: () -> Unit) {
        val st = Ui.theme.style
        val shape = RoundedCornerShape(minOf(st.button, 12f).dp)
        val bg = when {
            !on -> Color.Transparent
            st.fill == Fill.TONAL -> Voice.tonal
            st.fill == Fill.RULE -> Ui.Chalk
            else -> Ui.Surface
        }
        Row(
            Modifier.fillMaxWidth().clip(shape).background(bg)
                .then(if (on && st.fill == Fill.OUTLINE) Modifier.border(1.dp, Ui.Border, shape) else Modifier)
                .clickable(onClick = onClick).padding(horizontal = 14.dp, vertical = 12.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            if (on && st.fill != Fill.RULE) Box(Modifier.size(6.dp).clip(CircleShape).background(Ui.Red))
            Text(Voice.cased(label), color = if (on) (if (st.fill == Fill.RULE) Ui.Black else Ui.Chalk) else Ui.Muted,
                fontSize = if (Voice.mono) 12.sp else 15.sp, fontFamily = if (Voice.mono) Type.Mono else Voice.titleFont,
                fontWeight = if (on) FontWeight.Bold else FontWeight.Normal, letterSpacing = Voice.labelSpacing,
                modifier = Modifier.padding(start = if (on && st.fill != Fill.RULE) 10.dp else 16.dp))
        }
    }

    private var pairText by mutableStateOf("")
    private var pairStatus by mutableStateOf<String?>(null)

    /** Paired devices and pairing with the laptop (code from the clipboard or typed). */
    @Composable
    private fun SyncCard() {
        @Suppress("UNUSED_VARIABLE") val t = tick
        val engine = SyncHub.engine
        Column(Modifier.padding(horizontal = 16.dp, vertical = 4.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            if (engine == null) {
                val err = SyncHub.startError
                Text(if (err == null) "Sync startet …" else "Sync startet nicht: $err", color = if (err == null) Ui.Muted else Ui.Red, fontSize = 13.sp)
                if (err != null) Btn("Neu versuchen") { SyncHub.retryStart() }
                return@Column
            }
            // Network state, refreshed while the settings are open (what to check first when
            // the laptop doesn't show up).
            var net by androidx.compose.runtime.remember { mutableStateOf<String?>(null) }
            androidx.compose.runtime.LaunchedEffect(engine) {
                while (true) {
                    val s = kotlinx.coroutines.withContext(kotlinx.coroutines.Dispatchers.IO) { runCatching { engine.status() }.getOrNull() }
                    net = s?.let {
                        (if (it.relay != null) "Relay verbunden" else "kein Relay — nur im selben WLAN") +
                            " · ${it.directAddrs} direkte Adr. · " + (if (it.systemDns) "System-DNS" else "Ersatz-DNS") + " · Gerät ${it.nodeId.take(8)}"
                    }
                    kotlinx.coroutines.delay(3000)
                }
            }
            net?.let { Text(it, color = Ui.Muted, fontSize = 12.sp, fontFamily = Type.Mono) }
            // The name the laptop and guests see (applies when the app starts again).
            var name by remember { mutableStateOf(Settings.displayName) }
            Text("Name für Laptop und Gäste", color = Ui.Chalk, fontSize = 14.sp)
            Field(name, "z. B. Alex’ Tablet") { name = it; Settings.set("display_name", it.trim()) }
            if (name.trim() != SyncHub.startedName) Text("Gilt nach dem nächsten Start der App.", color = Ui.Muted, fontSize = 12.sp)
            if (SyncHub.devices.isEmpty()) Text("Noch kein Laptop gekoppelt. Am Laptop: n0te → Einstellungen → Geräte → Tablet koppeln, dann hier QR scannen (oder den Code einfügen).", color = Ui.Muted, fontSize = 13.sp)
            SyncHub.devices.forEach { d ->
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Text(if (d.online) "●" else "○", color = if (d.online) Ui.Red else Ui.Muted, fontSize = 14.sp, modifier = Modifier.width(22.dp))
                    Text(d.name, color = Ui.Chalk, fontSize = 14.sp)
                    Text(if (d.online) "  verbunden" else "  nicht erreichbar", color = Ui.Muted, fontSize = 12.sp)
                }
            }
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
                // The laptop shows the code as a QR code (n0te → Einstellungen → TABLET KOPPELN).
                Btn("QR scannen", BtnKind.PRIMARY) {
                    scanQr { code ->
                        if (!code.startsWith("n0te-pair:")) { pairStatus = "Das ist kein Koppel-Code von n0te"; return@scanQr }
                        pairStatus = "Kopple …"
                        N0te.aside("n0te-pair", { engine.acceptPair(code) }) { r ->
                            pairStatus = r.fold({ "Gekoppelt mit ${it.name}" }, { "Koppeln fehlgeschlagen: ${it.message}" })
                            SyncHub.refreshDevices()
                            tick++
                        }
                    }
                }
                Btn("Code einfügen") {
                    val clip = (getSystemService(android.content.Context.CLIPBOARD_SERVICE) as android.content.ClipboardManager).primaryClip?.getItemAt(0)?.text?.toString().orEmpty()
                    val code = if (clip.trim().startsWith("n0te-pair:")) clip.trim() else pairText.trim()
                    if (!code.startsWith("n0te-pair:")) { pairStatus = "Kein Koppel-Code in der Zwischenablage — unten einfügen"; return@Btn }
                    pairStatus = "Kopple …"
                    N0te.aside("n0te-pair", { engine.acceptPair(code) }) { r ->
                        pairStatus = r.fold({ "Gekoppelt mit ${it.name}" }, { "Koppeln fehlgeschlagen: ${it.message}" })
                        SyncHub.refreshDevices()
                        tick++
                    }
                }
            }
            Field(pairText, "oder Code hier einfügen (n0te-pair:…)", mono = true) { pairText = it }
            pairStatus?.let { Text(it, color = Ui.Chalk, fontSize = 12.sp) }
        }
    }

    /** Scans a QR code with Google's code scanner (no camera permission; Play services). */
    private fun scanQr(done: (String) -> Unit) {
        runCatching {
            com.google.mlkit.vision.codescanner.GmsBarcodeScanning.getClient(this).startScan()
                .addOnSuccessListener { b -> b.rawValue?.trim()?.let(done) }
                .addOnFailureListener { e -> pairStatus = "QR-Scanner: ${e.message} — Code stattdessen unten einfügen" }
        }.onFailure { pairStatus = "QR-Scanner nicht verfügbar — Code unten einfügen" }
    }

    /** A design as a card: a miniature (its chrome, heading face, button shapes, paper), name, one line. */
    @Composable
    private fun StyleCard(t: Theme) {
        val on = Ui.theme.id == t.id
        val st = t.style
        Column(Modifier.width(210.dp).option(on).clickable {
            if (!on) { Settings.set("theme", t.id); Ui.theme = t; recreate() }
        }.padding(10.dp)) {
            val mini = RoundedCornerShape(minOf(st.card, 10f).dp)
            Row(
                Modifier.fillMaxWidth().height(86.dp).clip(mini).background(Color(t.bg)).border(1.dp, Color(t.border), mini).padding(12.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    val face = when (st.heading) { HeadingFace.DOT -> Type.Dot; HeadingFace.MONO -> Type.Mono; HeadingFace.SANS -> Type.Sans; HeadingFace.SERIF -> Type.Serif }
                    Text(if (st.heading == HeadingFace.DOT) "aa" else "Aa", color = Color(t.fg), fontFamily = face,
                        fontWeight = if (st.heading == HeadingFace.DOT) FontWeight.Black else FontWeight.Bold, fontSize = if (st.heading == HeadingFace.DOT) 28.sp else 24.sp)
                    Row(horizontalArrangement = Arrangement.spacedBy(5.dp)) {
                        val r = RoundedCornerShape(minOf(st.button, 7f).dp)
                        Box(Modifier.size(width = 34.dp, height = 12.dp).clip(r).background(Color(t.accent)))
                        val (bg, line) = when (st.fill) {
                            Fill.OUTLINE -> Color.Transparent to Color(t.border)
                            Fill.TONAL -> (if (t.dark) Color(t.accent).copy(alpha = 0.22f) else Color(t.elevated)) to Color.Transparent
                            Fill.RULE -> Color.Transparent to Color(t.fg).copy(alpha = 0.6f)
                        }
                        Box(Modifier.size(width = 28.dp, height = 12.dp).clip(r).background(bg).then(if (line.alpha > 0f) Modifier.border(1.dp, line, r) else Modifier))
                    }
                }
                Box(Modifier.size(width = 34.dp, height = 46.dp).clip(RoundedCornerShape(minOf(st.card, 3f).dp)).background(Color(t.paper)))
            }
            Text(t.name, color = Ui.Chalk, fontSize = 15.sp, fontFamily = Voice.titleFont, fontWeight = FontWeight.Medium, modifier = Modifier.padding(top = 8.dp))
            Text(st.desc, color = Ui.Muted, fontSize = 12.sp, lineHeight = 15.sp)
        }
    }

    /** A titled group of settings. */
    @Composable
    private fun Card(title: String, sub: String? = null, boxed: Boolean = true, content: @Composable () -> Unit) {
        Column(Modifier.widthIn(max = 820.dp).fillMaxWidth()) {
            Caption(title, Modifier.padding(start = 4.dp, bottom = 4.dp))
            if (sub != null) Text(sub, color = Ui.Muted, fontSize = 12.sp, modifier = Modifier.padding(start = 4.dp, bottom = 8.dp))
            if (boxed) Column(Modifier.fillMaxWidth().group().padding(vertical = 8.dp)) { content() } else content()
        }
    }

    /** What the handwriting search's recogniser is doing (its model is loaded once, over Wi-Fi). */
    private fun handwritingNote(): String = when (Handwriting.state) {
        Handwriting.State.READY -> "Die Erkennung läuft auf dem Tablet, offline. Gefunden wird es in der Gliederung (Suchen) und auf dem Laptop"
        Handwriting.State.LOADING -> "Die Erkennung lädt (etwa 20 MB, einmalig, über WLAN) …"
        Handwriting.State.FAILED -> "Laden fehlgeschlagen: ${Handwriting.error ?: "unbekannt"} · zum erneuten Versuch aus- und einschalten"
        else -> "Lädt einmal die Erkennung (etwa 20 MB, über WLAN), dann offline auf dem Tablet"
    }

    @Composable
    private fun Toggle(key: String, title: String, sub: String?, value: Boolean) {
        Row(
            Modifier.fillMaxWidth().clickable {
                Settings.set(key, !value); tick++
                if (key == "handwriting_search" && !value) Handwriting.check(fetch = true)
            }.padding(horizontal = 16.dp, vertical = 11.dp),
            verticalAlignment = Alignment.CenterVertically
        ) {
            Column(Modifier.weight(1f)) {
                Text(title, color = Ui.Chalk, fontSize = 15.sp)
                if (sub != null) Text(sub, color = Ui.Muted, fontSize = 12.sp)
            }
            Switch(value)
        }
    }

    @Composable
    private fun SliderRow(key: String, title: String, value: Float, min: Float, max: Float, show: (Float) -> String, sub: String?, onChange: () -> Unit = {}) {
        var v by remember(key) { mutableStateOf(value) }
        Column(Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 6.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text(title, color = Ui.Chalk, fontSize = 15.sp, modifier = Modifier.weight(1f))
                Text(show(v), color = Ui.Chalk, fontSize = 13.sp, fontFamily = Voice.metaFont)
            }
            N0Slider(v, { v = it; Settings.set(key, it); onChange() }, min..max)
            if (sub != null) Text(sub, color = Ui.Muted, fontSize = 12.sp)
        }
    }

    /** Values as people read them: a share in percent, page points in millimetres. */
    private val PERCENT: (Float) -> String = { "%.0f %%".format(it * 100f) }
    private val MM: (Float) -> String = { "%.0f mm".format(it * 0.3528f) }
    private val MM1: (Float) -> String = { "%.1f mm".format(it * 0.3528f) }
}
