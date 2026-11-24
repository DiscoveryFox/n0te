package dev.n0bridge.n0te

import androidx.compose.foundation.layout.statusBarsPadding

import androidx.compose.ui.draw.drawBehind

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.TextRange
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.TextFieldValue
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import uniffi.n0te_ffi.FfiObjInfo
import uniffi.n0te_ffi.FfiStyle

/** Width of the graph editor's panel. */
const val GRAPH_PANEL_W = 380

/** GeoGebra's commands (German), each with the template it puts into the input. */
private val COMMANDS = listOf(
    "Punkt" to "Punkt(f, 1)", "Gerade" to "Gerade(A, B)", "Strecke" to "Strecke(A, B)", "Strahl" to "Strahl(A, B)", "Vektor" to "Vektor(A, B)",
    "Kreis" to "Kreis(M, 2)", "Vieleck" to "Vieleck(A, B, C)", "Mittelpunkt" to "Mittelpunkt(A, B)", "Schneide" to "Schneide(f, g)",
    "Nullstelle" to "Nullstelle(f)", "Extremum" to "Extremum(f)", "Wendepunkt" to "Wendepunkt(f)", "Ableitung" to "Ableitung(f)",
    "Integral" to "Integral(f, 0, 1)", "IntegralZwischen" to "IntegralZwischen(f, g, 0, 1)", "Tangente" to "Tangente(A, f)",
    "Senkrechte" to "Senkrechte(A, g)", "Parallele" to "Gerade(A, g)", "Mittelsenkrechte" to "Mittelsenkrechte(A, B)",
    "Winkelhalbierende" to "Winkelhalbierende(A, B, C)", "Abstand" to "Abstand(A, B)", "Länge" to "Länge(s)", "Steigung" to "Steigung(g)",
    "Winkel" to "Winkel(A, B, C)", "Fläche" to "Fläche(vieleck1)", "Funktion" to "Funktion(f, 0, 2)", "Wenn" to "Wenn(x < 0, -x, x^2)",
    "Kurve" to "Kurve(cos(t), sin(t), t, 0, 2pi)", "Text" to "Text(\"\")", "Schieberegler" to "Schieberegler(-5, 5, 0.1)",
    "Spiegle" to "Spiegle(A, g)", "Verschiebe" to "Verschiebe(A, u)", "Drehe" to "Drehe(A, 90°, M)",
)

/** First things to type into an empty graph (a tap adds them). */
private val EXAMPLES = listOf("f(x) = x^2", "A = (1, 2)", "y = 2x + 1", "Kreis((0, 0), 3)", "a = 2")

/** The colours an object can take: the plot palette, then the pen's inks, then greys. */
private val COLOURS = listOf(0xFF1565C0, 0xFFC62828, 0xFF2E7D32, 0xFF6A1B9A, 0xFFEF6C00, 0xFF00838F, 0xFF8D6E63, 0xFFAD1457, 0xFF424242, 0xFF20242A, 0xFF9E9E9E).map { it.toInt() }

private val LINEAR = listOf("Funktion", "Gerade", "Geraden", "Strecke", "Strahl", "Vektor", "Kreis", "Vieleck", "Kurve", "Winkel")
private val POINTY = listOf("Punkt", "Punkte")
private val LABELS = listOf("Aus", "Name", "Name & Wert", "Wert", "Eigener Text")
private val LINES = listOf("———", "– – –", "·······")
private val POINTS = listOf("●", "×", "○")

// --- top bar ---------------------------------------------------------------------------------

/**
 * The editor's top bar: Bewegen and the tools used last (each with its name), all tools,
 * undo and redo, a menu (panel, discard), and Fertig.
 */
@Composable
fun GraphToolbar(s: GraphSession, tick: Int, panel: Boolean, big: Boolean, onPanel: () -> Unit, onBig: () -> Unit, showTools: () -> Unit, onDone: () -> Unit, onDiscard: () -> Unit, modifier: Modifier = Modifier, docked: Boolean = false) {
    @Suppress("UNUSED_VARIABLE") val t = tick
    var menu by remember { mutableStateOf(false) }
    var confirm by remember { mutableStateOf(false) }
    Column(modifier, horizontalAlignment = Alignment.End) {
        // Docked (Groß): a flat bar across the whole top (like GeoGebra Classic), tools left, the rest right.
        val bar = if (docked) Modifier.fillMaxWidth().background(Ui.Bar).drawBehind { drawLine(Ui.Border, Offset(0f, size.height - 0.5f), Offset(size.width, size.height - 0.5f), 1.dp.toPx()) }
            .statusBarsPadding().padding(horizontal = 10.dp, vertical = 5.dp)
            else Modifier.bar(24f).padding(horizontal = 8.dp, vertical = 5.dp)
        Row(bar, verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(2.dp)) {
            val tools = listOf(GTool.MOVE) + GraphPrefs.recent + listOfNotNull(s.tool.takeIf { it != GTool.MOVE && it !in GraphPrefs.recent })
            // The tools scroll when the room is narrow (a pane of a split screen): Groß, undo and Fertig always show.
            Row(Modifier.weight(1f, fill = docked).horizontalScroll(rememberScrollState()), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(2.dp)) {
                tools.forEach { tool -> BarTool(tool.short, s.tool == tool, { ToolIcon(tool, Ui.Chalk, Ui.Red, 24.dp) }) { s.choose(tool) } }
                BarTool("Alle", false, { GridIcon(Ui.Chalk, 24.dp) }, onClick = showTools)
            }
            if (!docked) BarDivider()
            // Big: the graph fills the room beside the panel; off: as on the page, the rest in view.
            BarTool("Groß", big, { ExpandIcon(Ui.Chalk, 24.dp, big) }, onClick = onBig)
            GIconBtn(Glyph.UNDO, enabled = s.canUndo) { s.undo() }
            GIconBtn(Glyph.REDO, enabled = s.canRedo) { s.redo() }
            GIconBtn(Glyph.MENU, selected = menu) { menu = !menu; confirm = false }
            Spacer(Modifier.width(4.dp))
            Row(Modifier.clip(RoundedCornerShape(18.dp)).background(Ui.Red).clickable(onClick = onDone).padding(horizontal = 14.dp, vertical = 9.dp),
                verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                GlyphIcon(Glyph.CHECK, Ui.OnAccent, 16.dp)
                Text("Fertig", color = Ui.OnAccent, fontSize = 14.sp, fontWeight = FontWeight.SemiBold)
            }
        }
        if (menu) Column(Modifier.padding(top = 6.dp, end = if (docked) 10.dp else 0.dp).width(300.dp).panel().padding(vertical = 6.dp)) {
            if (!confirm) {
                MenuRow(if (panel) "Leiste links ausblenden" else "Leiste links zeigen") { menu = false; onPanel() }
                MenuRow("Änderungen verwerfen", danger = true) { if (s.changed) confirm = true else { menu = false; onDiscard() } }
            } else {
                Text("Alle Änderungen an diesem Graphen verwerfen?", color = Ui.Chalk, fontSize = 14.sp, modifier = Modifier.padding(horizontal = 14.dp, vertical = 8.dp))
                Row(Modifier.padding(horizontal = 8.dp), horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                    Btn("Verwerfen", BtnKind.DANGER) { menu = false; confirm = false; onDiscard() }
                    Btn("Behalten", BtnKind.QUIET) { menu = false; confirm = false }
                }
            }
        }
    }
}

@Composable
private fun BarTool(label: String, on: Boolean, icon: @Composable () -> Unit, onClick: () -> Unit) {
    val shape = RoundedCornerShape(12.dp)
    Column(Modifier.width(58.dp).clip(shape).background(if (on) Ui.Elevated else Color.Transparent)
        .border(if (on) 1.5.dp else 0.dp, if (on) Ui.Red else Color.Transparent, shape)
        .clickable(onClick = onClick).padding(top = 5.dp, bottom = 3.dp), horizontalAlignment = Alignment.CenterHorizontally) {
        icon()
        Text(label, color = if (on) Ui.Chalk else Ui.Muted, fontSize = 10.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
    }
}

/** Two arrows to the corners (big) or to the middle (as on the page). */
@Composable
private fun ExpandIcon(ink: Color, size: Dp, big: Boolean) {
    Canvas(Modifier.size(size)) {
        val k = this.size.minDimension / 24f
        val st = androidx.compose.ui.graphics.drawscope.Stroke(1.7f * k, cap = androidx.compose.ui.graphics.StrokeCap.Round, join = androidx.compose.ui.graphics.StrokeJoin.Round)
        fun arrow(x0: Float, y0: Float, x1: Float, y1: Float) {
            val p = androidx.compose.ui.graphics.Path()
            p.moveTo(x0 * k, y0 * k); p.lineTo(x1 * k, y1 * k)
            val dx = Math.signum(x0 - x1) * 5f; val dy = Math.signum(y0 - y1) * 5f
            p.moveTo((x1 + dx) * k, y1 * k); p.lineTo(x1 * k, y1 * k); p.lineTo(x1 * k, (y1 + dy) * k)
            drawPath(p, ink, style = st)
        }
        if (big) { arrow(10f, 10f, 4f, 4f); arrow(14f, 14f, 20f, 20f) } else { arrow(4f, 4f, 10f, 10f); arrow(20f, 20f, 14f, 14f) }
        drawRoundRect(ink.copy(alpha = 0.35f), Offset(2f * k, 2f * k), Size(20f * k, 20f * k), CornerRadius(3f * k), style = androidx.compose.ui.graphics.drawscope.Stroke(1.1f * k))
    }
}

/** Four squares: all tools. */
@Composable
private fun GridIcon(ink: Color, size: Dp) {
    Canvas(Modifier.size(size)) {
        val k = this.size.minDimension / 24f
        for (x in listOf(4f, 13.5f)) for (y in listOf(4f, 13.5f))
            drawRoundRect(ink, Offset(x * k, y * k), Size(6.5f * k, 6.5f * k), CornerRadius(1.6f * k), style = androidx.compose.ui.graphics.drawscope.Stroke(1.6f * k))
    }
}

@Composable
private fun BarDivider() = Box(Modifier.padding(horizontal = 5.dp).width(1.dp).height(26.dp).background(Ui.Border))

@Composable
private fun GIconBtn(g: Glyph, selected: Boolean = false, enabled: Boolean = true, size: Dp = 40.dp, onClick: () -> Unit) {
    Box(Modifier.size(size).clip(CircleShape).background(if (selected) Ui.Chalk else Color.Transparent).clickable(enabled = enabled, onClick = onClick), contentAlignment = Alignment.Center) {
        GlyphIcon(g, when { selected -> Ui.Black; enabled -> Ui.Chalk; else -> Ui.Muted.copy(alpha = 0.5f) }, 20.dp)
    }
}

@Composable
private fun MenuRow(text: String, danger: Boolean = false, onClick: () -> Unit) {
    Text(text, color = if (danger) Ui.Red else Ui.Chalk, fontSize = 14.sp, modifier = Modifier.fillMaxWidth().clickable(onClick = onClick).padding(horizontal = 14.dp, vertical = 11.dp))
}

// --- bottom: steps, style, notices -----------------------------------------------------------

/**
 * Under the graph: what the running tool wants next (step by step); with Bewegen, the selected
 * object's quick style (like GeoGebra's style bar); what just happened, with Rückgängig.
 */
@Composable
fun GraphHint(s: GraphSession, tick: Int, onSettings: (String) -> Unit, modifier: Modifier = Modifier) {
    @Suppress("UNUSED_VARIABLE") val t = tick
    val ask = s.asking
    if (ask != null) {
        NumberAsk(s, tick, ask, modifier)
        return
    }
    Column(modifier.imePadding(), horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(8.dp)) {
        s.notice?.let { n ->
            LaunchedEffect(n) { kotlinx.coroutines.delay(9000); if (s.notice == n) { s.notice = null; s.refresh() } }
            Row(Modifier.bar(18f).padding(start = 16.dp, end = 6.dp, top = 4.dp, bottom = 4.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                Text(n, color = Ui.Chalk, fontSize = 13.sp, maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.widthIn(max = 420.dp))
                Row(Modifier.clip(RoundedCornerShape(10.dp)).clickable { s.undo() }.padding(horizontal = 10.dp, vertical = 8.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                    GlyphIcon(Glyph.UNDO, Ui.Red, 16.dp)
                    Text("Rückgängig", color = Ui.Red, fontSize = 13.sp, fontWeight = FontWeight.SemiBold)
                }
            }
        }
        val info = s.infoOf(s.selected)
        when {
            s.tool != GTool.MOVE -> StepBar(s, tick)
            info != null -> StyleBar(s, tick, info, onSettings)
            s.error != null || s.message != null -> Row(Modifier.bar(18f).padding(start = 16.dp, end = 4.dp, top = 4.dp, bottom = 4.dp), verticalAlignment = Alignment.CenterVertically) {
                Text(s.error ?: s.message ?: "", color = if (s.error != null) Ui.Red else Ui.Chalk, fontSize = 13.sp, maxLines = 2, modifier = Modifier.widthIn(max = 520.dp))
                GIconBtn(Glyph.CLOSE, size = 34.dp) { s.error = null; s.message = null; s.refresh() }
            }
        }
    }
}

/** The running tool: its picture and name, the step, what is picked so far; ✕ back to Bewegen. */
@Composable
private fun StepBar(s: GraphSession, tick: Int) {
    @Suppress("UNUSED_VARIABLE") val t = tick
    Row(Modifier.bar(18f).padding(start = 12.dp, end = 4.dp, top = 6.dp, bottom = 6.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
        ToolIcon(s.tool, Ui.Chalk, Ui.Red, 28.dp)
        Column(Modifier.widthIn(max = 460.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                Text(s.tool.label, color = Ui.Chalk, fontSize = 13.sp, fontWeight = FontWeight.SemiBold)
                s.stepOf?.let { (i, n) -> Text("Schritt $i von $n", color = Ui.Muted, fontSize = 12.sp) }
            }
            Text(s.error ?: s.step, color = if (s.error != null) Ui.Red else Ui.Chalk, fontSize = 14.sp, maxLines = 2, overflow = TextOverflow.Ellipsis)
        }
        val picked = s.picks.filter { it.isNotEmpty() }
        if (picked.isNotEmpty()) Row(horizontalArrangement = Arrangement.spacedBy(4.dp)) {
            picked.forEach { Text(it, color = Ui.Black, fontSize = 12.sp, fontFamily = Type.Mono, modifier = Modifier.clip(RoundedCornerShape(6.dp)).background(Ui.Chalk).padding(horizontal = 6.dp, vertical = 2.dp)) }
        }
        GIconBtn(Glyph.CLOSE, size = 36.dp) { s.choose(GTool.MOVE) }
    }
}

/** The selected object at a glance: colour, line or point, label; its settings; delete. */
@Composable
private fun StyleBar(s: GraphSession, tick: Int, info: FfiObjInfo, onSettings: (String) -> Unit) {
    @Suppress("UNUSED_VARIABLE") val t = tick
    val o = s.draft.objs.firstOrNull { it.id == info.id } ?: return
    val st = o.style
    fun set(change: (FfiStyle) -> FfiStyle) = s.style(o.id, change)
    Row(Modifier.bar(18f).padding(start = 14.dp, end = 4.dp, top = 4.dp, bottom = 4.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
        Text(info.name, color = Ui.Chalk, fontSize = 15.sp, fontWeight = FontWeight.SemiBold, fontFamily = Type.Mono, maxLines = 1, modifier = Modifier.widthIn(max = 90.dp))
        BarDivider()
        COLOURS.take(6).forEach { c -> Swatch(c, st.color.toInt() == c, 24.dp) { set { it.copy(color = c.toUInt()) } } }
        BarDivider()
        if (info.kind in LINEAR) Cycle(LINES[st.line.toInt().coerceIn(0, 2)]) { set { it.copy(line = ((it.line.toInt() + 1) % 3).toUByte()) } }
        if (info.kind in POINTY) Cycle(POINTS[st.point.toInt().coerceIn(0, 2)]) { set { it.copy(point = ((it.point.toInt() + 1) % 3).toUByte()) } }
        // The label: off, name, name and value, value (the own caption stays in the settings).
        Cycle("Aa · " + LABELS[st.label.toInt().coerceIn(0, 4)]) { set { it.copy(label = ((it.label.toInt() + 1) % 4).toUByte()) } }
        BarDivider()
        GIconBtn(Glyph.SETTINGS, size = 38.dp) { onSettings(o.id) }
        GIconBtn(Glyph.TRASH, size = 38.dp) { s.remove(o.id) }
        GIconBtn(Glyph.CLOSE, size = 38.dp) { s.select(null) }
    }
}

@Composable
private fun Swatch(c: Int, on: Boolean, size: Dp, onClick: () -> Unit) {
    Box(Modifier.size(size).clip(CircleShape).background(Color(c)).border(if (on) 3.dp else 1.dp, if (on) Ui.Chalk else Ui.Muted.copy(alpha = 0.4f), CircleShape).clickable(onClick = onClick))
}

/** A small button that steps through choices (its text: the current one). */
@Composable
private fun Cycle(text: String, onClick: () -> Unit) {
    Text(text, color = Ui.Chalk, fontSize = 12.sp, maxLines = 1, modifier = Modifier.clip(RoundedCornerShape(8.dp)).background(Ui.Elevated).clickable(onClick = onClick).padding(horizontal = 9.dp, vertical = 7.dp))
}

/** A tool waits for a number (radius, angle). */
@Composable
private fun NumberAsk(s: GraphSession, tick: Int, ask: String, modifier: Modifier) {
    @Suppress("UNUSED_VARIABLE") val t = tick
    val field = remember(ask) { MathField() }
    var err by remember(ask) { mutableStateOf<String?>(null) }
    field.onEnter = { err = s.answer(field.text); if (err == null) s.keys.active = null }
    // The keyboard comes at once: the tool waits for this number.
    LaunchedEffect(ask) { s.keys.tab = 0; s.keys.active = field }
    androidx.compose.runtime.DisposableEffect(field) { onDispose { if (s.keys.active === field) s.keys.active = null } }
    Row(modifier.bar(18f).padding(horizontal = 14.dp, vertical = 8.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
        ToolIcon(s.tool, Ui.Chalk, Ui.Red, 26.dp)
        Text("$ask:", color = Ui.Chalk, fontSize = 14.sp)
        MathInputBox(field, s.keys, "", err != null, Modifier.width(140.dp))
        Btn("OK", BtnKind.PRIMARY) { field.onEnter() }
        GIconBtn(Glyph.CLOSE, size = 34.dp) { s.keys.active = null; s.clearPicks(); s.choose(s.tool) }
        err?.let { Text(it, color = Ui.Red, fontSize = 12.sp) }
    }
}

// --- the panel -------------------------------------------------------------------------------

/**
 * The editor's panel, like GeoGebra's: the algebra view (objects, the input), the tools by
 * group, the graphics settings; an object's settings open in its place.
 */
@Composable
fun GraphPanel(s: GraphSession, tick: Int, modifier: Modifier = Modifier, docked: Boolean = false) {
    @Suppress("UNUSED_VARIABLE") val t = tick
    // Docked (Groß): flush with the edge under the bar, a line towards the graph.
    val look = if (docked) Modifier.width(GRAPH_PANEL_W.dp).background(Ui.Bar).drawBehind { drawLine(Ui.Border, Offset(size.width - 0.5f, 0f), Offset(size.width - 0.5f, size.height), 1.dp.toPx()) }
        else Modifier.width(GRAPH_PANEL_W.dp).imePadding().panel()
    Column(modifier.then(look)) {
        val o = s.draft.objs.firstOrNull { it.id == s.settings }
        val info = s.infoOf(o?.id)
        if (o != null && info != null) {
            Row(Modifier.fillMaxWidth().padding(start = 4.dp, end = 12.dp, top = 6.dp, bottom = 2.dp), verticalAlignment = Alignment.CenterVertically) {
                GIconBtn(Glyph.BACK) { s.openSettings(null) }
                Text(info.name, color = Ui.Chalk, fontSize = 17.sp, fontWeight = FontWeight.SemiBold, fontFamily = Type.Mono)
                Spacer(Modifier.width(8.dp))
                Text(info.kind, color = Ui.Muted, fontSize = 13.sp)
            }
            ObjectSettings(s, tick, o, info)
            return@Column
        }
        // The three pages as one switch.
        Row(Modifier.fillMaxWidth().padding(horizontal = 10.dp, vertical = 10.dp).clip(RoundedCornerShape(12.dp)).background(Ui.Elevated.copy(alpha = 0.55f)).padding(3.dp)) {
            listOf("Algebra", "Werkzeuge", "Grafik").forEachIndexed { i, l ->
                val on = s.tab == i
                Text(l, color = if (on) Ui.Black else Ui.Chalk, fontSize = 14.sp, fontWeight = if (on) FontWeight.SemiBold else FontWeight.Normal, textAlign = TextAlign.Center, maxLines = 1,
                    modifier = Modifier.weight(1f).clip(RoundedCornerShape(9.dp)).background(if (on) Ui.Chalk else Color.Transparent).clickable { s.tab = i }.padding(vertical = 8.dp))
            }
        }
        Box(Modifier.weight(1f, fill = false)) {
            when (s.tab) {
                0 -> AlgebraView(s, tick)
                1 -> ToolsView(s, tick)
                else -> GraphicsProps(s, tick)
            }
        }
    }
}

// --- algebra ---------------------------------------------------------------------------------

@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun AlgebraView(s: GraphSession, tick: Int) {
    @Suppress("UNUSED_VARIABLE") val t = tick
    val field = remember { MathField() }
    var error by remember { mutableStateOf<String?>(null) }
    var commands by remember { mutableStateOf(false) }
    val focused = s.keys.active === field
    val input = field.value
    var seen by remember { mutableStateOf(input.text) }
    if (input.text != seen) { seen = input.text; error = null }
    fun template(word: Int, tmpl: String) {
        val v = field.value
        val head = v.text.substring(0, word); val tail = v.text.substring(v.selection.max)
        val open = tmpl.indexOf('(')
        field.value = TextFieldValue(head + tmpl + tail, if (open >= 0) TextRange(head.length + open + 1, head.length + tmpl.length - 1) else TextRange(head.length + tmpl.length))
        s.keys.active = field
    }
    fun submit() {
        error = s.input(field.text)
        if (error == null) field.set("")
    }
    field.onEnter = { submit() }
    // Another page of the panel: the keyboard has nothing to type into here any more.
    androidx.compose.runtime.DisposableEffect(field) { onDispose { if (s.keys.active === field) s.keys.active = null } }
    Column {
        // The objects (helpers last, like GeoGebra).
        Column(Modifier.weight(1f, fill = false).verticalScroll(rememberScrollState()).padding(horizontal = 8.dp)) {
            val objs = s.draft.objs
            if (objs.isEmpty()) {
                Text("Unten eine Funktion, einen Punkt oder einen Befehl eintippen, oder oben ein Werkzeug nehmen und in den Graphen tippen.",
                    color = Ui.Muted, fontSize = 13.sp, modifier = Modifier.padding(horizontal = 6.dp, vertical = 6.dp))
                FlowRow(Modifier.padding(horizontal = 6.dp, vertical = 4.dp), horizontalArrangement = Arrangement.spacedBy(6.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
                    EXAMPLES.forEach { ex -> Text(ex, color = Ui.Chalk, fontSize = 13.sp, fontFamily = Type.Mono, modifier = Modifier.clip(RoundedCornerShape(8.dp)).background(Ui.Elevated).clickable { error = s.input(ex) }.padding(horizontal = 9.dp, vertical = 6.dp)) }
                }
            }
            val main = objs.filter { !it.style.aux }
            val aux = objs.filter { it.style.aux }
            main.forEachIndexed { i, o -> s.infoOf(o.id)?.let { info -> key(o.id) { if (i > 0) RowRule(); AlgebraRow(s, tick, o, info) } } }
            if (aux.isNotEmpty()) {
                Caption("Hilfsobjekte", Modifier.padding(start = 10.dp, top = 12.dp, bottom = 2.dp))
                aux.forEachIndexed { i, o -> s.infoOf(o.id)?.let { info -> key(o.id) { if (i > 0) RowRule(); AlgebraRow(s, tick, o, info) } } }
            }
        }
        // The input, always at hand (like GeoGebra's input bar).
        Column(Modifier.padding(start = 10.dp, end = 10.dp, top = 6.dp, bottom = 10.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                MathInputBox(field, s.keys, "Eingabe …", error != null, Modifier.weight(1f))
                if (input.text.isNotBlank()) Box(Modifier.size(38.dp).clip(CircleShape).background(Ui.Red).clickable { submit() }, contentAlignment = Alignment.Center) { GlyphIcon(Glyph.RETURN, Ui.OnAccent, 18.dp) }
            }
            error?.let { Text(it, color = Ui.Red, fontSize = 12.sp, modifier = Modifier.padding(start = 4.dp, top = 4.dp)) }
            // What it will be, set as a formula while typing (also typed LaTeX).
            val tex = remember(input.text, s.draft) { s.preview(input.text) }
            if (tex.isNotEmpty() && error == null) Box(Modifier.padding(top = 6.dp).fillMaxWidth().clip(RoundedCornerShape(10.dp)).background(Ui.Elevated.copy(alpha = 0.45f)).padding(horizontal = 12.dp, vertical = 8.dp)) {
                MathText(tex, "", Ui.Chalk, 17.sp)
            }
            // Commands that start like the word being typed (GeoGebra's suggestions).
            val wordAt = run { var a = input.selection.min; while (a > 0 && input.text[a - 1].isLetter()) a--; a }
            val word = input.text.substring(wordAt, input.selection.min)
            val hits = if (word.length >= 2) COMMANDS.filter { it.first.startsWith(word, ignoreCase = true) }.take(5) else emptyList()
            if (hits.isNotEmpty()) Column(Modifier.padding(top = 6.dp).fillMaxWidth().clip(RoundedCornerShape(10.dp)).background(Ui.Elevated)) {
                hits.forEach { (name, tmpl) ->
                    Row(Modifier.fillMaxWidth().clickable { template(wordAt, tmpl) }.padding(horizontal = 10.dp, vertical = 8.dp), verticalAlignment = Alignment.CenterVertically) {
                        Text(name, color = Ui.Chalk, fontSize = 14.sp, fontWeight = FontWeight.Medium, modifier = Modifier.weight(1f))
                        Text(tmpl, color = Ui.Muted, fontSize = 12.sp, fontFamily = Type.Mono, maxLines = 1)
                    }
                }
            }
            // All commands, while typing.
            if (focused || commands) Text(if (commands) "Befehle ausblenden" else "Alle Befehle …", color = Ui.Muted, fontSize = 12.sp,
                modifier = Modifier.padding(top = 6.dp).clip(RoundedCornerShape(8.dp)).clickable { commands = !commands }.padding(horizontal = 6.dp, vertical = 4.dp))
            if (commands) Column(Modifier.padding(top = 6.dp).fillMaxWidth().heightIn(max = 260.dp).clip(RoundedCornerShape(10.dp)).background(Ui.Elevated.copy(alpha = 0.5f)).verticalScroll(rememberScrollState()).padding(6.dp)) {
                COMMANDS.sortedBy { it.first }.chunked(2).forEach { pair ->
                    Row(horizontalArrangement = Arrangement.spacedBy(4.dp)) {
                        pair.forEach { (name, tmpl) ->
                            Column(Modifier.weight(1f).clip(RoundedCornerShape(8.dp)).clickable { field.set(""); field.template(tmpl); commands = false; s.keys.active = field }.padding(6.dp)) {
                                Text(name, color = Ui.Chalk, fontSize = 13.sp, fontWeight = FontWeight.Medium)
                                Text(tmpl, color = Ui.Muted, fontSize = 11.sp, fontFamily = Type.Mono, maxLines = 1, overflow = TextOverflow.Ellipsis)
                            }
                        }
                        if (pair.size == 1) Spacer(Modifier.weight(1f))
                    }
                }
            }
        }
    }
}

/** A fine line between two objects of the list. */
@Composable
private fun RowRule() = Box(Modifier.fillMaxWidth().padding(start = 44.dp, end = 8.dp).height(1.dp).background(Ui.Border.copy(alpha = 0.6f)))

/**
 * One object: its dot (shown or hidden), its value and, for objects made from others, how.
 * A tap selects it (the graph shows it, the style bar comes); selected, its definition can be
 * changed right there.
 */
@Composable
private fun AlgebraRow(s: GraphSession, tick: Int, o: uniffi.n0te_ffi.FfiObj, info: FfiObjInfo) {
    @Suppress("UNUSED_VARIABLE") val t = tick
    val selected = s.selected == o.id
    Column(Modifier.fillMaxWidth().padding(vertical = 1.dp).clip(RoundedCornerShape(10.dp)).background(if (selected) Ui.Elevated else Color.Transparent)) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Box(Modifier.size(40.dp).clickable { s.style(o.id) { it.copy(visible = !it.visible) } }, contentAlignment = Alignment.Center) {
                Box(Modifier.size(20.dp).border(1.dp, Ui.Muted.copy(alpha = 0.45f), CircleShape), contentAlignment = Alignment.Center) {
                    Box(Modifier.size(15.dp).clip(CircleShape).background(if (o.style.visible) Color(o.style.color.toInt()) else Color.Transparent).border(2.dp, Color(o.style.color.toInt()), CircleShape))
                }
            }
            Column(Modifier.weight(1f).clickable(enabled = !selected) { s.select(o.id) }.padding(top = 7.dp, bottom = 7.dp, end = 4.dp)) {
                if (!selected) {
                    // Set like GeoGebra's algebra list (a formula), how it came about under it.
                    if (info.error == null) MathText(info.latex, info.shown, if (o.style.visible) Ui.Chalk else Ui.Muted, 17.sp)
                    else Text(o.def, color = Ui.Red, fontSize = 14.sp, fontFamily = Type.Mono, maxLines = 2, overflow = TextOverflow.Ellipsis)
                    if (info.error != null) Text(info.error!!, color = Ui.Red, fontSize = 11.sp)
                    else if (info.definition.isNotEmpty()) Text(info.definition, color = Ui.Muted, fontSize = 11.sp, fontFamily = Type.Mono, maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.padding(top = 3.dp))
                } else {
                    val field = remember(o.id, o.def) { MathField(o.def) }
                    var err by remember(o.id) { mutableStateOf<String?>(null) }
                    field.onEnter = { err = s.setDef(o.id, field.text); if (err == null) s.keys.active = null }
                    androidx.compose.runtime.DisposableEffect(field) { onDispose { if (s.keys.active === field) s.keys.active = null } }
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        MathInputBox(field, s.keys, "", err != null, Modifier.weight(1f), 14.sp)
                        if (field.text != o.def) GIconBtn(Glyph.CHECK, size = 36.dp) { field.onEnter() }
                    }
                    val note = err ?: info.error
                    if (note != null) Text(note, color = Ui.Red, fontSize = 11.sp, modifier = Modifier.padding(top = 2.dp))
                    else if (info.error == null) MathText(info.latex, info.shown, Ui.Muted, 13.sp, Modifier.padding(top = 4.dp))
                }
            }
            if (selected) GIconBtn(Glyph.TRASH, size = 38.dp) { s.remove(o.id) }
        }
        // A slider right in the list.
        val sl = o.style.slider
        val v = info.sliderValue
        if (sl != null && v != null) {
            Row(Modifier.padding(start = 40.dp, end = 12.dp, bottom = 4.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                Text(Plots.format(sl.min), color = Ui.Muted, fontSize = 11.sp)
                N0Slider(v.toFloat(), { nv -> uniffi.n0te_ffi.plotSetSlider(s.draft, o.id, nv.toDouble())?.let { s.set(it, quiet = true) } }, sl.min.toFloat()..sl.max.toFloat(), Modifier.weight(1f))
                Text(Plots.format(sl.max), color = Ui.Muted, fontSize = 11.sp)
            }
        }
    }
}

// --- tools -----------------------------------------------------------------------------------

/** The tools as tiles with their names: the basic ones, all of them by group on request. */
@Composable
private fun ToolsView(s: GraphSession, tick: Int) {
    @Suppress("UNUSED_VARIABLE") val t = tick
    var all by remember { mutableStateOf(GraphPrefs.allTools) }
    Column(Modifier.verticalScroll(rememberScrollState()).padding(start = 12.dp, end = 12.dp, bottom = 14.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
        Text("${s.tool.label}: ${s.tool.hint}", color = Ui.Muted, fontSize = 12.sp, maxLines = 2)
        ToolGrid(s, tick, GTool.basic)
        if (!all) Btn("Alle Werkzeuge", BtnKind.SECONDARY, Modifier.fillMaxWidth()) { all = true; GraphPrefs.allTools = true }
        else {
            GTool.categories.forEach { (title, tools) ->
                Caption(title, Modifier.padding(top = 6.dp))
                ToolGrid(s, tick, tools)
            }
            Btn("Weniger Werkzeuge", BtnKind.QUIET, Modifier.fillMaxWidth()) { all = false; GraphPrefs.allTools = false }
        }
    }
}

@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun ToolGrid(s: GraphSession, tick: Int, tools: List<GTool>) {
    @Suppress("UNUSED_VARIABLE") val t = tick
    FlowRow(maxItemsInEachRow = 4, horizontalArrangement = Arrangement.spacedBy(6.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
        tools.forEach { tool ->
            val on = s.tool == tool
            val shape = RoundedCornerShape(12.dp)
            Column(Modifier.width(83.dp).height(80.dp).clip(shape).background(if (on) Ui.Elevated else Color.Transparent)
                .border(if (on) 1.5.dp else 1.dp, if (on) Ui.Red else Ui.Border, shape).clickable { s.choose(tool) }.padding(horizontal = 4.dp, vertical = 8.dp),
                horizontalAlignment = Alignment.CenterHorizontally) {
                ToolIcon(tool, Ui.Chalk, Ui.Red, 30.dp)
                Spacer(Modifier.height(4.dp))
                // Long words break where a hyphen belongs (Mittel-senkrechte), not anywhere.
                Text(tool.label, color = if (on) Ui.Chalk else Ui.Chalk.copy(alpha = 0.85f), fontSize = 11.sp, lineHeight = 13.sp, textAlign = TextAlign.Center, maxLines = 2, overflow = TextOverflow.Ellipsis,
                    style = TextStyle(hyphens = androidx.compose.ui.text.style.Hyphens.Auto, lineBreak = androidx.compose.ui.text.style.LineBreak.Paragraph))
            }
        }
    }
}

// --- an object's settings ----------------------------------------------------------------------

@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun ObjectSettings(s: GraphSession, tick: Int, o: uniffi.n0te_ffi.FfiObj, info: FfiObjInfo) {
    @Suppress("UNUSED_VARIABLE") val t = tick
    val st = o.style
    fun set(change: (FfiStyle) -> FfiStyle) = s.style(o.id, change)
    val fillable = info.kind in listOf("Vieleck", "Kreis", "Winkel", "Integral")
    Column(Modifier.verticalScroll(rememberScrollState()).padding(start = 14.dp, end = 12.dp, bottom = 14.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
        // A plain text: its words; anything else: its definition.
        val literal = Regex("^\\s*\\w+\\s*=\\s*Text\\(\"(.*)\"\\)\\s*$").find(o.def)?.groupValues?.get(1)
        var err by remember(o.id) { mutableStateOf<String?>(null) }
        if (literal != null) {
            Caption("Text")
            var words by remember(o.id) { mutableStateOf(literal) }
            Field(words, "Text", onChange = { w -> words = w; err = s.setDef(o.id, "Text(\"${w.replace("\"", "'")}\")") })
        } else {
            Caption("Definition")
            val field = remember(o.id, o.def) { MathField(o.def) }
            field.onEnter = { err = s.setDef(o.id, field.text); if (err == null) s.keys.active = null }
            androidx.compose.runtime.DisposableEffect(field) { onDispose { if (s.keys.active === field) s.keys.active = null } }
            Row(verticalAlignment = Alignment.CenterVertically) {
                MathInputBox(field, s.keys, "", err != null, Modifier.weight(1f))
                if (field.text != o.def) GIconBtn(Glyph.CHECK) { field.onEnter() }
            }
            if (info.error == null) MathText(info.latex, info.shown, Ui.Muted, 15.sp)
        }
        (err ?: info.error)?.let { Text(it, color = Ui.Red, fontSize = 12.sp) }

        Caption("Farbe", Modifier.padding(top = 6.dp))
        FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            COLOURS.forEach { c -> Swatch(c, st.color.toInt() == c, 30.dp) { set { it.copy(color = c.toUInt()) } } }
        }
        if (info.kind in LINEAR) {
            Caption("Linie", Modifier.padding(top = 6.dp))
            Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) { LINES.forEachIndexed { i, l -> SmallChoice(l, st.line.toInt() == i, keepCase = true) { set { it.copy(line = i.toUByte()) } } } }
            LabeledSlider("Stärke", if (st.width > 0f) st.width else s.draft.width, 0.4f..5f) { w -> set { it.copy(width = w) } }
        }
        if (info.kind in POINTY) {
            Caption("Punkt", Modifier.padding(top = 6.dp))
            Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) { POINTS.forEachIndexed { i, l -> SmallChoice(l, st.point.toInt() == i, keepCase = true) { set { it.copy(point = i.toUByte()) } } } }
            LabeledSlider("Größe", if (st.size > 0f) st.size else 2.4f, 1f..6f) { v -> set { it.copy(size = v) } }
        }
        if (fillable) LabeledSlider("Füllung", st.fill, 0f..1f) { v -> set { it.copy(fill = v) } }

        Caption("Beschriftung", Modifier.padding(top = 6.dp))
        FlowRow(horizontalArrangement = Arrangement.spacedBy(6.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
            LABELS.forEachIndexed { i, l -> SmallChoice(l, st.label.toInt() == i) { set { it.copy(label = i.toUByte()) } } }
        }
        if (st.label.toInt() == 4) Field(st.caption, "Eigener Text", onChange = { c -> set { it.copy(caption = c) } })

        if (info.kind == "Funktion") {
            Caption("Punkte daran zeigen", Modifier.padding(top = 6.dp))
            FlowRow(horizontalArrangement = Arrangement.spacedBy(6.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
                for ((bit, l) in listOf(1 to "Nullstellen", 2 to "Hoch- und Tiefpunkte", 8 to "Wendepunkte", 4 to "Schnittpunkte")) {
                    SmallChoice(l, st.special.toInt() and bit != 0) { set { it.copy(special = (it.special.toInt() xor bit).toUByte()) } }
                }
            }
            if (st.special.toInt() != 0) ToggleRow("Mit Koordinaten", st.coords) { set { it.copy(coords = !it.coords) } }
        }
        st.slider?.let { sl ->
            Caption("Schieberegler", Modifier.padding(top = 6.dp))
            Row(horizontalArrangement = Arrangement.spacedBy(6.dp), verticalAlignment = Alignment.CenterVertically) {
                Text("von", color = Ui.Muted, fontSize = 13.sp); NumField(sl.min) { v -> set { it.copy(slider = sl.copy(min = v)) } }
                Text("bis", color = Ui.Muted, fontSize = 13.sp); NumField(sl.max) { v -> set { it.copy(slider = sl.copy(max = v)) } }
                Text("Schritt", color = Ui.Muted, fontSize = 13.sp); NumField(sl.step) { v -> set { it.copy(slider = sl.copy(step = v)) } }
            }
        }

        Caption("Verhalten", Modifier.padding(top = 6.dp))
        ToggleRow("Anzeigen", st.visible) { set { it.copy(visible = !it.visible) } }
        if (info.draggable || st.fixed || st.slider != null) ToggleRow("Fixieren (nicht ziehen)", st.fixed) { set { it.copy(fixed = !it.fixed) } }
        ToggleRow("Hilfsobjekt (in der Liste unten)", st.aux) { set { it.copy(aux = !it.aux) } }
        val deps = remember(s.draft, o.id) { uniffi.n0te_ffi.plotDependents(s.draft, o.id) }
        Spacer(Modifier.height(4.dp))
        // Names keep their case (c the circle, C the point).
        Btn(if (deps.isEmpty()) "Löschen" else "Löschen, mit ${deps.joinToString(", ")}", BtnKind.DANGER, keepCase = true) { s.remove(o.id) }
    }
}

@Composable
private fun LabeledSlider(label: String, value: Float, range: ClosedFloatingPointRange<Float>, onChange: (Float) -> Unit) {
    Row(verticalAlignment = Alignment.CenterVertically) {
        Text(label, color = Ui.Muted, fontSize = 13.sp, modifier = Modifier.width(64.dp))
        N0Slider(value, onChange, range, Modifier.weight(1f))
    }
}

@Composable
private fun NumField(v: Double, onSet: (Double) -> Unit) {
    var text by remember(v) { mutableStateOf(Plots.format(v)) }
    Field(text, "", number = true, modifier = Modifier.width(64.dp), onChange = { t -> text = t; Plots.number(t)?.let(onSet) })
}

@Composable
private fun ToggleRow(label: String, on: Boolean, flip: () -> Unit) {
    Row(Modifier.fillMaxWidth().clickable(onClick = flip).padding(vertical = 3.dp), verticalAlignment = Alignment.CenterVertically) {
        Text(label, color = Ui.Chalk, fontSize = 14.sp, modifier = Modifier.weight(1f)); Switch(on)
    }
}

// --- graphics --------------------------------------------------------------------------------

@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun GraphicsProps(s: GraphSession, tick: Int) {
    @Suppress("UNUSED_VARIABLE") val t = tick
    val p = s.draft
    Column(Modifier.verticalScroll(rememberScrollState()).padding(start = 14.dp, end = 12.dp, bottom = 14.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
        Caption("Ausschnitt")
        Row(horizontalArrangement = Arrangement.spacedBy(6.dp), verticalAlignment = Alignment.CenterVertically) {
            Text("x von", color = Ui.Muted, fontSize = 13.sp, modifier = Modifier.width(44.dp))
            NumField(p.xMin) { v -> if (v < p.xMax) s.view { it.copy(xMin = v) } }
            Text("bis", color = Ui.Muted, fontSize = 13.sp)
            NumField(p.xMax) { v -> if (v > p.xMin) s.view { it.copy(xMax = v) } }
        }
        Row(horizontalArrangement = Arrangement.spacedBy(6.dp), verticalAlignment = Alignment.CenterVertically) {
            Text("y von", color = Ui.Muted, fontSize = 13.sp, modifier = Modifier.width(44.dp))
            NumField(p.yMin) { v -> if (v < p.yMax) s.view { it.copy(yMin = v) } }
            Text("bis", color = Ui.Muted, fontSize = 13.sp)
            NumField(p.yMax) { v -> if (v > p.yMin) s.view { it.copy(yMax = v) } }
        }
        FlowRow(horizontalArrangement = Arrangement.spacedBy(6.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
            SmallChoice("Zurücksetzen", false) { s.resetView() }
            SmallChoice("y passend", false) { uniffi.n0te_ffi.plotFitY(p).takeIf { it.size == 2 }?.let { r -> s.view { it.copy(yMin = r[0], yMax = r[1]) } } }
            SmallChoice("Gleiche Einheiten", false) { uniffi.n0te_ffi.plotEqualUnits(p).let { r -> s.view { it.copy(yMin = r[0], yMax = r[1]) } } }
            SmallChoice("x in π", p.pi) {
                if (!p.pi) uniffi.n0te_ffi.plotPiRange(p).let { r -> s.view { it.copy(pi = true, xMin = r[0], xMax = r[1]) } } else s.view { it.copy(pi = false) }
            }
        }
        Text("Im Graphen: ins Leere ziehen verschiebt, zwei Finger zoomen.", color = Ui.Muted, fontSize = 11.sp)

        Caption("Achsen und Raster", Modifier.padding(top = 6.dp))
        ToggleRow("Achsen", p.axes) { s.view { it.copy(axes = !it.axes) } }
        if (p.axes) Row(horizontalArrangement = Arrangement.spacedBy(6.dp), verticalAlignment = Alignment.CenterVertically) {
            Text("Namen", color = Ui.Muted, fontSize = 13.sp, modifier = Modifier.width(52.dp))
            Field(p.xName, "x", modifier = Modifier.width(80.dp), onChange = { n -> s.view { it.copy(xName = n) } })
            Field(p.yName, "y", modifier = Modifier.width(80.dp), onChange = { n -> s.view { it.copy(yName = n) } })
        }
        ToggleRow("Raster", p.grid) { s.view { it.copy(grid = !it.grid) } }
        if (p.grid) ToggleRow("Feines Raster dazwischen", p.minor) { s.view { it.copy(minor = !it.minor) } }
        ToggleRow("Am Karopapier ausrichten", p.align) { s.view { it.copy(align = !it.align) } }

        Caption("Punktfang", Modifier.padding(top = 6.dp))
        Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            listOf("Automatisch", "Am Raster", "Aus").forEachIndexed { i, l -> SmallChoice(l, p.snap.toInt() == i) { s.view { it.copy(snap = i.toUByte()) } } }
        }
        Text(when (p.snap.toInt()) {
            1 -> "Neue und gezogene Punkte liegen immer auf dem Raster."
            2 -> "Punkte liegen genau dort, wo du tippst."
            else -> "Punkte rasten nah an einem Rasterpunkt ein, wenn das Raster an ist oder der Graph auf dem Karopapier sitzt."
        }, color = Ui.Muted, fontSize = 11.sp)

        Caption("Aussehen", Modifier.padding(top = 6.dp))
        ToggleRow("Wie mit dem Lineal (sonst von Hand gezeichnet)", p.clean) { s.view { it.copy(clean = !it.clean) } }
        LabeledSlider("Strich", p.width, 0.5f..4f) { w -> s.view { it.copy(width = w) } }

        Caption("Auf der Seite", Modifier.padding(top = 6.dp))
        ToggleRow("Interaktiv: Punkte und Schieberegler auf der Seite ziehen", p.interactive) { s.view { it.copy(interactive = !it.interactive) } }
    }
}
