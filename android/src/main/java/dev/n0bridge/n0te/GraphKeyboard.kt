package dev.n0bridge.n0te

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.focusable
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.input.key.onPreviewKeyEvent
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawWithContent
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.drawscope.drawIntoCanvas
import androidx.compose.ui.graphics.nativeCanvas
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.input.key.Key as HwKey
import androidx.compose.ui.input.key.isCtrlPressed
import androidx.compose.ui.input.key.isMetaPressed
import androidx.compose.ui.input.key.isShiftPressed
import androidx.compose.ui.input.key.key
import androidx.compose.ui.input.key.type
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.text.TextLayoutResult
import androidx.compose.ui.text.TextRange
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.TextFieldValue
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.TextUnit
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp

/** Height of the graph keyboard (the panel and bars stay above it). */
val GRAPH_KEYS_H = 292.dp

/**
 * A line typed with the graph keyboard instead of the system one (like GeoGebra's input):
 * its text and cursor; what Enter does.
 */
class MathField(text: String = "") {
    var value by mutableStateOf(TextFieldValue(text, TextRange(text.length)))
    var onEnter: () -> Unit = {}
    val text: String get() = value.text

    fun set(t: String) { value = TextFieldValue(t, TextRange(t.length)) }

    /**
     * Typing [before] replaces the selection; with [after] (brackets) they go around it. The
     * cursor ends between them.
     */
    fun insert(before: String, after: String = "") {
        val v = value
        val a = v.selection.min; val b = v.selection.max
        val inner = if (after.isEmpty()) "" else v.text.substring(a, b)
        val t = v.text.substring(0, a) + before + inner + after + v.text.substring(b)
        value = TextFieldValue(t, TextRange(a + before.length + inner.length))
    }

    /** A template like Schneide(f, g): what is inside its brackets is selected, to type over. */
    fun template(tmpl: String) {
        val v = value
        val a = v.selection.min; val b = v.selection.max
        val t = v.text.substring(0, a) + tmpl + v.text.substring(b)
        val open = tmpl.indexOf('(')
        value = if (open >= 0 && tmpl.endsWith(")") && tmpl.length - open > 2) TextFieldValue(t, TextRange(a + open + 1, a + tmpl.length - 1))
        else TextFieldValue(t, TextRange(a + tmpl.length))
    }

    /** An opening bracket brings its closing one (around the selection, if any). */
    fun open() = insert("(", ")")

    /** A closing bracket steps over the one already there. */
    fun close() {
        val v = value
        if (v.selection.collapsed && v.text.getOrNull(v.selection.start) == ')') value = v.copy(selection = TextRange(v.selection.start + 1))
        else insert(")")
    }

    fun backspace() {
        val v = value
        if (!v.selection.collapsed) { insert(""); return }
        val i = v.selection.start
        if (i == 0) return
        // An empty pair of brackets goes at once.
        val pair = (v.text[i - 1] == '(' && v.text.getOrNull(i) == ')') || (v.text[i - 1] == '|' && v.text.getOrNull(i) == '|')
        val end = if (pair) i + 1 else i
        value = TextFieldValue(v.text.substring(0, i - 1) + v.text.substring(end), TextRange(i - 1))
    }

    fun left() { val v = value; value = v.copy(selection = TextRange(if (v.selection.collapsed) (v.selection.start - 1).coerceAtLeast(0) else v.selection.min)) }
    fun right() { val v = value; value = v.copy(selection = TextRange(if (v.selection.collapsed) (v.selection.start + 1).coerceAtMost(v.text.length) else v.selection.max)) }

    // --- a keyboard plugged in: what its keys do -----------------------------------------------

    /** Shift + arrow: the selection grows or shrinks at its moving end. */
    fun extend(by: Int) {
        val v = value
        value = v.copy(selection = TextRange(v.selection.start, (v.selection.end + by).coerceIn(0, v.text.length)))
    }
    fun home(select: Boolean = false) { val v = value; value = v.copy(selection = if (select) TextRange(v.selection.start, 0) else TextRange(0)) }
    fun end(select: Boolean = false) { val v = value; value = v.copy(selection = if (select) TextRange(v.selection.start, v.text.length) else TextRange(v.text.length)) }
    fun selectAll() { value = value.copy(selection = TextRange(0, text.length)) }
    fun selected(): String { val v = value; return v.text.substring(v.selection.min, v.selection.max) }

    /** Delete (the key right of Backspace): the selection, else the character after the cursor. */
    fun deleteForward() {
        val v = value
        if (!v.selection.collapsed) { insert(""); return }
        val i = v.selection.start
        if (i >= v.text.length) return
        value = TextFieldValue(v.text.substring(0, i) + v.text.substring(i + 1), TextRange(i))
    }

    /** A character typed on a keyboard: brackets come in pairs (also LaTeX's braces), a closing one steps over. */
    fun type(c: Char) {
        when (c) {
            '(' -> open()
            ')' -> close()
            '{' -> insert("{", "}")
            '}' -> { val v = value; if (v.selection.collapsed && v.text.getOrNull(v.selection.start) == '}') value = v.copy(selection = TextRange(v.selection.start + 1)) else insert("}") }
            else -> insert(c.toString())
        }
    }
}

/**
 * The graph keyboard's state: the field it types into (null: none), its page, shift. With a
 * keyboard plugged in the one on screen stays down unless asked for ([onScreen]).
 */
class GraphKeys {
    var active by mutableStateOf<MathField?>(null)
    var tab by mutableStateOf(0)
    var shift by mutableStateOf(false)
    /** A hardware keyboard is there. */
    var hardware by mutableStateOf(false)
    /** The keyboard on screen asked for although a hardware one is there. */
    var onScreen by mutableStateOf(false)
    /** The keyboard on screen is up (the panels make room for it). */
    val shown: Boolean get() = active != null && (!hardware || onScreen)
}

/** Notices a keyboard being plugged in or out (the one on screen goes down or comes back). */
@Composable
fun GraphKeys.watchHardware() {
    val cfg = androidx.compose.ui.platform.LocalConfiguration.current
    val now = cfg.keyboard == android.content.res.Configuration.KEYBOARD_QWERTY && cfg.hardKeyboardHidden == android.content.res.Configuration.HARDKEYBOARDHIDDEN_NO
    if (hardware != now) hardware = now
}

/** A key of a plugged-in keyboard for [field] (true: used). */
fun mathKey(field: MathField, keys: GraphKeys, e: androidx.compose.ui.input.key.KeyEvent, clipboard: androidx.compose.ui.platform.ClipboardManager): Boolean {
    if (e.type != androidx.compose.ui.input.key.KeyEventType.KeyDown) return false
    val ctrl = e.isCtrlPressed || e.isMetaPressed
    val shift = e.isShiftPressed
    when (e.key) {
        HwKey.Enter, HwKey.NumPadEnter -> field.onEnter()
        HwKey.Backspace -> field.backspace()
        HwKey.Delete -> field.deleteForward()
        HwKey.DirectionLeft -> if (shift) field.extend(-1) else field.left()
        HwKey.DirectionRight -> if (shift) field.extend(1) else field.right()
        HwKey.MoveHome, HwKey.DirectionUp -> field.home(shift)
        HwKey.MoveEnd, HwKey.DirectionDown -> field.end(shift)
        HwKey.Escape -> keys.active = null
        HwKey.Tab -> return false
        else -> {
            if (ctrl) {
                when (e.key) {
                    HwKey.A -> field.selectAll()
                    HwKey.C -> field.selected().takeIf { it.isNotEmpty() }?.let { clipboard.setText(androidx.compose.ui.text.AnnotatedString(it)) }
                    HwKey.X -> field.selected().takeIf { it.isNotEmpty() }?.let { clipboard.setText(androidx.compose.ui.text.AnnotatedString(it)); field.insert("") }
                    HwKey.V -> clipboard.getText()?.text?.replace('\n', ' ')?.let { field.insert(it) }
                    else -> return false
                }
                return true
            }
            val c = e.nativeKeyEvent.unicodeChar
            if (c == 0 || Character.isISOControl(c)) return false
            field.type(c.toChar())
        }
    }
    return true
}

/**
 * A field for the graph keyboard: the text with its cursor (blinking) and selection; a tap
 * puts the cursor there and brings the keyboard.
 */
@Composable
fun MathInputBox(field: MathField, keys: GraphKeys, placeholder: String, error: Boolean, modifier: Modifier = Modifier, size: TextUnit = 15.sp) {
    val active = keys.active === field
    var layout by remember { mutableStateOf<TextLayoutResult?>(null) }
    // A plugged-in keyboard types here while the field is the active one.
    val focus = remember { androidx.compose.ui.focus.FocusRequester() }
    val clipboard = androidx.compose.ui.platform.LocalClipboardManager.current
    LaunchedEffect(active) { if (active) runCatching { focus.requestFocus() } }
    val scroll = rememberScrollState()
    val pad = with(LocalDensity.current) { 10.dp.toPx() }
    var blink by remember { mutableStateOf(true) }
    LaunchedEffect(active, field.value) { blink = true; while (active) { kotlinx.coroutines.delay(530); blink = !blink } }
    // The cursor stays in view in a long line.
    LaunchedEffect(field.value.selection, layout) {
        val l = layout ?: return@LaunchedEffect
        // The layout may still be of the text before the last keys (typed fast): only its offsets.
        val x = l.getCursorRect(field.value.selection.end.coerceIn(0, l.layoutInput.text.length)).left
        val w = scroll.viewportSize.toFloat()
        if (w > 0f) {
            if (x < scroll.value) scroll.scrollTo((x - 20f).toInt().coerceAtLeast(0))
            else if (x > scroll.value + w - 20f) scroll.scrollTo((x - w + 40f).toInt())
        }
    }
    val shape = RoundedCornerShape(10.dp)
    Box(modifier.clip(shape).background(Ui.Black)
        .border(if (active) 1.5.dp else 1.dp, if (error) Ui.Red else if (active) Ui.Chalk.copy(alpha = 0.6f) else Ui.Border, shape)
        .focusRequester(focus)
        .onPreviewKeyEvent { e -> active && mathKey(field, keys, e, clipboard) }
        .focusable()
        .pointerInput(field) {
            detectTapGestures { pos ->
                keys.active = field
                runCatching { focus.requestFocus() }
                layout?.let { l ->
                    val off = l.getOffsetForPosition(Offset(pos.x - pad + scroll.value, (pos.y - pad).coerceAtLeast(0f)))
                    field.value = field.value.copy(selection = TextRange(off.coerceIn(0, field.text.length)))
                }
            }
        }
        .padding(10.dp)) {
        if (field.text.isEmpty()) Text(placeholder, color = Ui.Muted, fontSize = size, fontFamily = Type.Mono, maxLines = 1)
        Box(Modifier.fillMaxWidth().horizontalScroll(scroll)) {
            Text(field.text, color = Ui.Chalk, fontSize = size, fontFamily = Type.Mono, softWrap = false, maxLines = 1, onTextLayout = { layout = it },
                modifier = Modifier.widthIn(min = 4.dp).drawWithContent {
                    val l = layout
                    val sel = field.value.selection
                    val n = l?.layoutInput?.text?.length ?: 0
                    if (active && l != null && !sel.collapsed) drawPath(l.getPathForRange(sel.min.coerceIn(0, n), sel.max.coerceIn(0, n)), Ui.Red.copy(alpha = 0.32f))
                    drawContent()
                    if (active && l != null && sel.collapsed && blink) {
                        val r = l.getCursorRect(sel.start.coerceIn(0, n))
                        drawLine(Ui.Red, Offset(r.left + 1f, r.top), Offset(r.left + 1f, r.bottom), 2.dp.toPx())
                    }
                })
        }
    }
}

// --- the keyboard ------------------------------------------------------------------------------

private enum class KeyKind { SYMBOL, NUMBER, ACTION, ENTER }

private class Key(val label: String, val weight: Float = 1f, val kind: KeyKind = KeyKind.SYMBOL, val act: (MathField, GraphKeys) -> Unit)

private fun t(label: String, ins: String = label, kind: KeyKind = KeyKind.SYMBOL, weight: Float = 1f) = Key(label, weight, kind) { f, _ -> f.insert(ins) }
private fun n(label: String) = t(label, kind = KeyKind.NUMBER)
private fun wrap(label: String, before: String, after: String) = Key(label) { f, _ -> f.insert(before, after) }
private fun tpl(label: String, template: String) = Key(label) { f, _ -> f.template(template) }
private val BACK = Key("⌫", 1.6f, KeyKind.ACTION) { f, _ -> f.backspace() }
private val LEFT = Key("←", 0.8f, KeyKind.ACTION) { f, _ -> f.left() }
private val RIGHT = Key("→", 0.8f, KeyKind.ACTION) { f, _ -> f.right() }
private val OPEN = Key("(", 0.8f, KeyKind.ACTION) { f, _ -> f.open() }
private val CLOSE = Key(")", 0.8f, KeyKind.ACTION) { f, _ -> f.close() }
private val ENTER = Key("↵", 1.6f, KeyKind.ENTER) { f, _ -> f.onEnter() }
private val GAP = Key("", 0.25f) { _, _ -> }

/** GeoGebra's keyboard pages: numbers and operators, functions, letters, Greek and signs. */
private val PAGES: List<Pair<String, List<List<Key>>>> = listOf(
    "123" to listOf(
        listOf(t("x"), t("y"), t("π"), t("e"), GAP, n("7"), n("8"), n("9"), t("×", "·"), t("÷", "/"), GAP, BACK),
        listOf(t("x²", "²"), wrap("xⁿ", "^(", ")"), wrap("√", "√(", ")"), wrap("|x|", "|", "|"), GAP, n("4"), n("5"), n("6"), t("+", " + "), t("−", " − "), GAP, LEFT, RIGHT),
        listOf(t("<", " < "), t(">", " > "), t("≤", " ≤ "), t("≥", " ≥ "), GAP, n("1"), n("2"), n("3"), t("=", " = "), t(",", ", "), GAP, OPEN, CLOSE),
        listOf(t(":", ": "), t("′", "'"), t("°"), wrap("a/b", "(", ")/()"), GAP, n("0"), n("."), Key("␣", 3f, KeyKind.NUMBER) { f, _ -> f.insert(" ") }, GAP, ENTER),
    ),
    "f(x)" to listOf(
        listOf(wrap("sin", "sin(", ")"), wrap("cos", "cos(", ")"), wrap("tan", "tan(", ")"), wrap("ln", "ln(", ")"), GAP, wrap("eˣ", "e^(", ")"), wrap("10ˣ", "10^(", ")"), wrap("lg", "lg(", ")"), wrap("√", "√(", ")"), wrap("|x|", "|", "|"), GAP, BACK),
        listOf(wrap("sin⁻¹", "arcsin(", ")"), wrap("cos⁻¹", "arccos(", ")"), wrap("tan⁻¹", "arctan(", ")"), t("x²", "²"), GAP, t("f(x) =", "f(x) = "), t("g(x) =", "g(x) = "), t("f′(x)", "f'(x)"), t("x"), t("y"), GAP, LEFT, RIGHT),
        listOf(tpl("Ableitung", "Ableitung(f)"), tpl("Integral", "Integral(f, 0, 1)"), tpl("Nullstelle", "Nullstelle(f)"), tpl("Extremum", "Extremum(f)"), GAP, tpl("Schneide", "Schneide(f, g)"), tpl("Tangente", "Tangente(A, f)"), tpl("Wenn", "Wenn(x < 0, -x, x²)"), tpl("Punkt", "Punkt(f, 1)"), t(",", ", "), GAP, OPEN, CLOSE),
        listOf(tpl("Gerade", "Gerade(A, B)"), tpl("Strecke", "Strecke(A, B)"), tpl("Kreis", "Kreis(M, 2)"), tpl("Vieleck", "Vieleck(A, B, C)"), GAP, t("π"), t("e"), t("a≤x≤b", ", 0 ≤ x ≤ 2"), t("="," = "), t("t"), GAP, ENTER),
    ),
    "ABC" to listOf(
        "qwertzuiopü".map { c -> Key(c.toString()) { f, k -> f.insert(if (k.shift) c.uppercase() else c.toString()); k.shift = false } } + listOf(BACK),
        "asdfghjklöä".map { c -> Key(c.toString()) { f, k -> f.insert(if (k.shift) c.uppercase() else c.toString()); k.shift = false } } + listOf(LEFT, RIGHT),
        listOf(Key("⇧", 2f, KeyKind.ACTION) { _, k -> k.shift = !k.shift }) + "yxcvbnm".map { c -> Key(c.toString()) { f, k -> f.insert(if (k.shift) c.uppercase() else c.toString()); k.shift = false } } +
            listOf(t("_"), t("\"", "\"\""), OPEN, CLOSE),
        listOf(t(",", ", "), t("."), t("="," = "), t("\\"), Key("␣", 5f, KeyKind.NUMBER) { f, _ -> f.insert(" ") }, t("'"), t(":", ": "), ENTER),
    ),
    "αβγ" to listOf(
        "αβγδεθλμ".map { c -> t(c.toString()) } + listOf(t("σ"), t("φ"), t("ω"), BACK),
        listOf(t("≤", " ≤ "), t("≥", " ≥ "), t("<", " < "), t(">", " > "), t("=", " = "), t("°"), t("′", "'"), t("\"", "\"\""), t("_"), t(";", "; "), t("|"), LEFT, RIGHT),
        listOf(t("["), t("]"), wrap("{ }", "{", "}"), t("\\"), t(":", ": "), t("!"), t("?"), t("&"), t("·"), t("−"), t("+"), OPEN, CLOSE),
        listOf(t(",", ", "), t("."), Key("␣", 9f, KeyKind.NUMBER) { f, _ -> f.insert(" ") }, ENTER),
    ),
)

/**
 * The graph keyboard, like GeoGebra's: pages 123 (numbers, x, y, π, powers, roots), f(x)
 * (functions and commands), ABC (letters, QWERTZ) and αβγ (Greek, signs); ✕ hides it.
 */
@Composable
fun GraphKeyboard(keys: GraphKeys, modifier: Modifier = Modifier) {
    keys.watchHardware()
    val field = keys.active ?: return
    if (!keys.shown) {
        // A keyboard is plugged in: the one on screen only when asked for.
        Box(modifier.fillMaxWidth().padding(14.dp), contentAlignment = Alignment.BottomEnd) {
            Text("⌨  Tastatur am Bildschirm", color = Ui.Chalk, fontSize = 13.sp,
                modifier = Modifier.clip(RoundedCornerShape(16.dp)).background(Ui.Bar).border(1.dp, Ui.Border, RoundedCornerShape(16.dp))
                    .clickable { keys.onScreen = true }.padding(horizontal = 14.dp, vertical = 8.dp))
        }
        return
    }
    Column(modifier.fillMaxWidth().height(GRAPH_KEYS_H).background(Ui.Bar).padding(horizontal = 10.dp, vertical = 6.dp)) {
        Box(Modifier.fillMaxWidth().widthIn(max = 1100.dp).align(Alignment.CenterHorizontally)) {
            Column {
                Row(Modifier.fillMaxWidth().padding(bottom = 6.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(4.dp)) {
                    PAGES.forEachIndexed { i, (name, _) ->
                        val on = keys.tab == i
                        Text(name, color = if (on) Ui.Chalk else Ui.Muted, fontSize = 14.sp, fontWeight = if (on) FontWeight.SemiBold else FontWeight.Normal,
                            modifier = Modifier.clip(RoundedCornerShape(8.dp)).clickable { keys.tab = i }.padding(horizontal = 14.dp, vertical = 6.dp)
                                .drawWithContent { drawContent(); if (on) drawLine(Ui.Red, Offset(8f, size.height - 1f), Offset(size.width - 8f, size.height - 1f), 2.5.dp.toPx()) })
                    }
                    Spacer(Modifier.weight(1f))
                    Text("Tastatur ausblenden", color = Ui.Muted, fontSize = 13.sp, modifier = Modifier.clip(RoundedCornerShape(8.dp)).clickable { if (keys.hardware) keys.onScreen = false else keys.active = null }.padding(horizontal = 12.dp, vertical = 6.dp))
                }
                PAGES[keys.tab.coerceIn(0, PAGES.size - 1)].second.forEach { row ->
                    Row(Modifier.fillMaxWidth().weight(1f).padding(vertical = 3.dp), horizontalArrangement = Arrangement.spacedBy(5.dp)) {
                        row.forEach { k -> KeyCap(k, field, keys) }
                    }
                }
            }
        }
    }
}

@Composable
private fun RowScope.KeyCap(k: Key, field: MathField, keys: GraphKeys) {
    if (k.label.isEmpty()) { Spacer(Modifier.weight(k.weight)); return }
    val shifted = k.label == "⇧" && keys.shift
    val (bg, fg) = when (k.kind) {
        KeyKind.ENTER -> Ui.Red to Ui.OnAccent
        KeyKind.ACTION -> (if (shifted) Ui.Chalk else Ui.Elevated.copy(alpha = 0.6f)) to (if (shifted) Ui.Black else Ui.Chalk)
        KeyKind.NUMBER -> Ui.Elevated to Ui.Chalk
        KeyKind.SYMBOL -> Ui.Elevated.copy(alpha = 0.75f) to Ui.Chalk
    }
    val letter = k.label.length == 1 && k.label[0].isLetter() && keys.shift && keys.tab == 2
    Box(Modifier.weight(k.weight).fillMaxHeight().clip(RoundedCornerShape(9.dp)).background(bg).clickable { k.act(field, keys) }, contentAlignment = Alignment.Center) {
        when (k.label) {
            "↵" -> GlyphIcon(Glyph.RETURN, fg, 24.dp)
            else -> Text(if (letter) k.label.uppercase() else k.label, color = fg, fontSize = when { k.label == "⌫" -> 24.sp; k.label.length > 6 -> 12.sp; k.label.length > 3 -> 14.sp; else -> 19.sp },
                fontWeight = if (k.kind == KeyKind.NUMBER) FontWeight.Medium else FontWeight.Normal, maxLines = 1)
        }
    }
}

// --- formulas --------------------------------------------------------------------------------

/**
 * A formula set like GeoGebra's algebra list (LaTeX by JLaTeXMath), shrunk to fit; [fallback]
 * as plain text when it can't be set.
 */
@Composable
fun MathText(latex: String, fallback: String, color: Color, size: TextUnit = 16.sp, modifier: Modifier = Modifier) {
    val px = with(LocalDensity.current) { size.toPx() }
    val f = remember(latex, px, color) { if (latex.isBlank()) null else TexMath.drawable(latex, px, color.toArgb(), false) }
    if (f == null) {
        Text(fallback, color = color, fontSize = size * 0.9f, fontFamily = Type.Mono, maxLines = 2, modifier = modifier)
        return
    }
    BoxWithConstraints(modifier) {
        val maxPx = constraints.maxWidth.toFloat()
        val k = if (f.width > maxPx && maxPx > 0f) maxPx / f.width else 1f
        val d = LocalDensity.current
        Canvas(Modifier.size(with(d) { (f.width * k).toDp() }, with(d) { (f.height * k).toDp() })) {
            drawIntoCanvas { c ->
                c.nativeCanvas.save()
                c.nativeCanvas.scale(k, k)
                f.drawable.draw(c.nativeCanvas)
                c.nativeCanvas.restore()
            }
        }
    }
}
