package dev.n0bridge.n0te

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.interaction.collectIsPressedAsState
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.gestures.detectHorizontalDragGestures
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.material3.LocalTextStyle
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.text.ExperimentalTextApi
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.Font
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.font.FontVariation
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.TextUnit
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp

/** The fonts (the same as the laptop's): Space Grotesk, Space Mono, Doto; serif from the system. */
object Type {
    val Sans = FontFamily(
        Font(R.font.space_grotesk_regular, FontWeight.Normal),
        Font(R.font.space_grotesk_medium, FontWeight.Medium),
        Font(R.font.space_grotesk_bold, FontWeight.Bold),
    )
    val Mono = FontFamily(Font(R.font.space_mono_regular, FontWeight.Normal), Font(R.font.space_mono_bold, FontWeight.Bold))
    @OptIn(ExperimentalTextApi::class)
    val Dot = FontFamily(Font(R.font.doto, FontWeight.Black, variationSettings = FontVariation.Settings(FontVariation.weight(900))))
    val Serif = FontFamily.Serif
}

/** The design's voice, read by every styled control. */
object Voice {
    private val s get() = Ui.theme.style
    val mono get() = s.label == LabelVoice.MONO_CAPS
    /** Labels are written in sentence case; the mono designs shout. */
    fun cased(text: String) = if (mono) text.uppercase() else text
    val labelFont get() = when (s.label) { LabelVoice.MONO_CAPS -> Type.Mono; LabelVoice.SANS -> Type.Sans; LabelVoice.SERIF -> Type.Serif }
    val labelWeight get() = if (s.label == LabelVoice.MONO_CAPS) FontWeight.Bold else FontWeight.Medium
    val labelSize get() = when (s.label) { LabelVoice.MONO_CAPS -> 12.sp; LabelVoice.SANS -> 14.sp; LabelVoice.SERIF -> 14.5.sp }
    val labelSpacing get() = if (mono) 1.sp else 0.sp
    /** Small facts (counts, ages). */
    val metaFont get() = when (s.label) { LabelVoice.MONO_CAPS -> Type.Mono; LabelVoice.SANS -> Type.Sans; LabelVoice.SERIF -> Type.Serif }
    /** Names of things (cards, options, settings rows). */
    val titleFont get() = if (s.heading == HeadingFace.SERIF) Type.Serif else Type.Sans
    /** The secondary surface of tonal designs: a tint of the accent on dark, a grey on light. */
    val tonal get() = if (Ui.theme.dark) Ui.Red.copy(alpha = 0.16f) else Ui.Elevated
    val tonalStrong get() = if (Ui.theme.dark) Ui.Red.copy(alpha = 0.26f) else Ui.Border
    /** A hairline for the outlined and ruled designs. */
    val line get() = when (s.fill) { Fill.RULE -> Ui.Chalk.copy(alpha = 0.32f); Fill.OUTLINE -> Ui.Border; Fill.TONAL -> Color.Transparent }
}

/** Body text in Space Grotesk for everything below. */
@Composable
fun N0teType(content: @Composable () -> Unit) {
    CompositionLocalProvider(LocalTextStyle provides TextStyle(fontFamily = Type.Sans, color = Ui.Chalk), content = content)
}

/** PRIMARY: the one main action; DANGER: deletes something (quiet, in the signal colour). */
enum class BtnKind { PRIMARY, SECONDARY, QUIET, SELECTED, DANGER }

/** A button in the design's shape and voice. Write [text] in sentence case. */
@Composable
fun Btn(text: String, kind: BtnKind = BtnKind.SECONDARY, modifier: Modifier = Modifier, enabled: Boolean = true, keepCase: Boolean = false, onClick: () -> Unit) {
    val st = Ui.theme.style
    val shape = RoundedCornerShape(st.button.dp)
    val (bg, fg, line) = when (kind) {
        BtnKind.PRIMARY -> Triple(Ui.Red, Ui.OnAccent, Ui.Red)
        BtnKind.SELECTED -> Triple(Ui.Chalk, Ui.Black, Ui.Chalk)
        BtnKind.QUIET -> Triple(Color.Transparent, Ui.Muted, Color.Transparent)
        BtnKind.DANGER -> Triple(Color.Transparent, Ui.Red, Color.Transparent)
        BtnKind.SECONDARY -> when (st.fill) {
            Fill.OUTLINE -> Triple(Color.Transparent, Ui.Chalk, Ui.Border)
            Fill.TONAL -> Triple(Voice.tonal, Ui.Chalk, Color.Transparent)
            Fill.RULE -> Triple(Color.Transparent, Ui.Chalk, Ui.Chalk.copy(alpha = 0.5f))
        }
    }
    Box(
        modifier
            .then(if (st.lift && kind == BtnKind.PRIMARY) Modifier.shadow(3.dp, shape) else Modifier)
            .clip(shape)
            .background(bg)
            .then(if (line.alpha > 0f) Modifier.border(1.dp, line, shape) else Modifier)
            .clickable(enabled = enabled, onClick = onClick)
            .padding(horizontal = if (Voice.mono) 16.dp else 18.dp, vertical = 10.dp),
        contentAlignment = Alignment.Center,
    ) {
        Text(if (keepCase) text else Voice.cased(text), color = if (enabled) fg else fg.copy(alpha = 0.45f), fontSize = Voice.labelSize, fontFamily = Voice.labelFont,
            fontWeight = Voice.labelWeight, letterSpacing = Voice.labelSpacing, maxLines = 1)
    }
}

/** The old name: a secondary button, or the primary one. */
@Composable
fun Pill(text: String, primary: Boolean = false, onClick: () -> Unit) = Btn(text, if (primary) BtnKind.PRIMARY else BtnKind.SECONDARY, onClick = onClick)

/** A choice among a few (paper, page size): the chosen one is filled. */
@Composable
fun Choice(text: String, on: Boolean, keepCase: Boolean = false, onClick: () -> Unit) = Btn(text, if (on) BtnKind.SELECTED else BtnKind.SECONDARY, keepCase = keepCase, onClick = onClick)

/** A compact choice for menus (nib, format): the chosen one is filled. */
@Composable
fun SmallChoice(text: String, on: Boolean, enabled: Boolean = true, keepCase: Boolean = false, onClick: () -> Unit) {
    val st = Ui.theme.style
    Text(if (keepCase) text else Voice.cased(text), color = if (on) Ui.Black else Ui.Chalk, fontSize = 12.sp, fontFamily = Voice.labelFont, letterSpacing = Voice.labelSpacing,
        fontWeight = if (on) FontWeight.Bold else FontWeight.Normal, maxLines = 1,
        modifier = Modifier.clip(RoundedCornerShape(minOf(st.button, 10f).dp))
            .background(if (on) Ui.Chalk else if (st.fill == Fill.TONAL) Voice.tonal else Ui.Elevated)
            .clickable(enabled = enabled, onClick = onClick).padding(horizontal = 10.dp, vertical = 7.dp))
}


/** One line of a changelog: a small accent dot, then the change. */
@Composable
fun ChangeItem(text: String) {
    Row(verticalAlignment = Alignment.Top) {
        Box(Modifier.padding(top = 8.dp).size(6.dp).clip(androidx.compose.foundation.shape.CircleShape).background(Ui.Red))
        Text(text, color = Ui.Chalk, fontSize = 14.sp, lineHeight = 20.sp, modifier = Modifier.padding(start = 12.dp))
    }
}

@Composable
fun Caption(text: String, modifier: Modifier = Modifier) {
    when (Ui.theme.style.label) {
        LabelVoice.MONO_CAPS -> Text(text.uppercase(), modifier, color = Ui.Muted, fontSize = 11.sp, fontFamily = Type.Mono, letterSpacing = 1.5.sp)
        LabelVoice.SANS -> Text(text, modifier, color = Ui.Muted, fontSize = 13.sp, fontFamily = Type.Sans, fontWeight = FontWeight.Medium)
        LabelVoice.SERIF -> Text(text, modifier, color = Ui.Muted, fontSize = 14.sp, fontFamily = Type.Serif, fontStyle = FontStyle.Italic)
    }
}

/** A heading in the design's display face. */
@Composable
fun Heading(text: String, size: TextUnit = 28.sp, modifier: Modifier = Modifier) {
    when (Ui.theme.style.heading) {
        HeadingFace.DOT -> Text(text.lowercase(), modifier, color = Ui.Chalk, fontSize = size * 1.3f, fontFamily = Type.Dot)
        HeadingFace.MONO -> Text(text.uppercase(), modifier, color = Ui.Chalk, fontSize = size * 0.8f, fontFamily = Type.Mono, fontWeight = FontWeight.Bold, letterSpacing = 2.sp)
        HeadingFace.SANS -> Text(text, modifier, color = Ui.Chalk, fontSize = size, fontFamily = Type.Sans, fontWeight = FontWeight.Bold)
        HeadingFace.SERIF -> Text(text, modifier, color = Ui.Chalk, fontSize = size * 1.05f, fontFamily = Type.Serif, fontWeight = FontWeight.Medium)
    }
}

/** A dialog or floating card. */
fun Modifier.panel(): Modifier {
    val st = Ui.theme.style
    val shape = RoundedCornerShape(st.panel.dp)
    val bg = if (st.fill == Fill.TONAL && Ui.theme.dark) Ui.Elevated else Ui.Surface
    return this
        .then(if (st.lift) Modifier.shadow(18.dp, shape) else Modifier)
        .clip(shape)
        .background(bg)
        .then(if (Voice.line.alpha > 0f) Modifier.border(1.dp, if (st.fill == Fill.RULE) Ui.Chalk.copy(alpha = 0.4f) else Ui.Border, shape) else Modifier)
}

/** A floating bar in the editor (toolbar, outline, chips): its radius capped by the design's. */
fun Modifier.bar(radius: Float): Modifier {
    val st = Ui.theme.style
    val shape = RoundedCornerShape(minOf(st.panel, radius).dp)
    return this.clip(shape).background(Ui.Bar)
        .then(if (Voice.line.alpha > 0f) Modifier.border(1.dp, if (st.fill == Fill.RULE) Ui.Chalk.copy(alpha = 0.4f) else Ui.Border, shape) else Modifier)
}

/** A group inside a screen (a settings section). */
fun Modifier.group(): Modifier {
    val st = Ui.theme.style
    val shape = RoundedCornerShape(minOf(st.card, 18f).dp)
    return when (st.fill) {
        Fill.TONAL -> this.then(if (st.lift) Modifier.shadow(2.dp, shape) else Modifier).clip(shape).background(Ui.Surface)
        Fill.OUTLINE -> this.clip(shape).background(Ui.Surface).border(1.dp, Ui.Border, shape)
        Fill.RULE -> this.border(1.dp, Ui.Chalk.copy(alpha = 0.22f), shape)
    }
}

/** A notebook card. */
fun Modifier.cardBox(): Modifier {
    val st = Ui.theme.style
    val shape = RoundedCornerShape(st.card.dp)
    return when (st.fill) {
        Fill.TONAL -> this.then(if (st.lift) Modifier.shadow(4.dp, shape) else Modifier).clip(shape).background(Ui.Surface)
        Fill.OUTLINE -> this.clip(shape).background(Ui.Surface).border(1.dp, Ui.Border, shape)
        Fill.RULE -> this.clip(shape).border(1.dp, Ui.Chalk.copy(alpha = 0.22f), shape)
    }
}

/** A selectable tile (export scope, design): the chosen one carries the accent. */
fun Modifier.option(on: Boolean): Modifier {
    val st = Ui.theme.style
    val shape = RoundedCornerShape(minOf(st.card, 16f).dp)
    val bg = if (st.fill == Fill.TONAL) Voice.tonal else Ui.Surface
    val line = when {
        on -> Ui.Red
        st.fill == Fill.TONAL -> Color.Transparent
        st.fill == Fill.RULE -> Ui.Chalk.copy(alpha = 0.3f)
        else -> Ui.Border
    }
    return this.clip(shape).background(bg).then(if (line.alpha > 0f) Modifier.border(if (on) 2.dp else 1.dp, line, shape) else Modifier)
}

/** An on/off switch: a pill (square in OLED); Nothing-style light track with the red knob in the mono designs. */
@Composable
fun Switch(value: Boolean) {
    val st = Ui.theme.style
    val square = st.fill == Fill.RULE
    val track = RoundedCornerShape(if (square) 0.dp else 14.dp)
    val (trackColor, knob) = when {
        Voice.mono && value -> Ui.Chalk to Ui.Red
        Voice.mono -> Ui.Black to Ui.Muted
        value -> Ui.Red to Color.White
        else -> Ui.Border to (if (Ui.theme.dark) Ui.Muted else Color.White)
    }
    Box(
        Modifier.size(width = 48.dp, height = 28.dp).clip(track).background(trackColor)
            .border(1.dp, if (value) trackColor else if (square) Ui.Chalk.copy(alpha = 0.5f) else Ui.Border, track).padding(4.dp),
        contentAlignment = if (value) Alignment.CenterEnd else Alignment.CenterStart,
    ) {
        Box(Modifier.size(20.dp).clip(if (square) RoundedCornerShape(0.dp) else CircleShape).background(knob))
    }
}

/**
 * A slider in the design: a thin track, the filled part and a round knob in the accent. The
 * whole 40 dp row takes the finger; [onDone] runs when it lets go (save, apply to the page).
 */
@Composable
fun N0Slider(value: Float, onChange: (Float) -> Unit, range: ClosedFloatingPointRange<Float>, modifier: Modifier = Modifier,
             enabled: Boolean = true, onDone: () -> Unit = {}) {
    val square = Ui.theme.style.fill == Fill.RULE
    val span = (range.endInclusive - range.start).takeIf { it > 0f } ?: 1f
    val f = ((value - range.start) / span).coerceIn(0f, 1f)
    val latest = androidx.compose.runtime.rememberUpdatedState(onChange)
    val done = androidx.compose.runtime.rememberUpdatedState(onDone)
    fun at(x: Float, w: Int) = range.start + (x / w.coerceAtLeast(1)).coerceIn(0f, 1f) * span
    androidx.compose.foundation.layout.BoxWithConstraints(
        modifier.height(40.dp).fillMaxWidth().alpha(if (enabled) 1f else 0.4f)
            .then(if (!enabled) Modifier else Modifier.pointerInput(range) {
                detectTapGestures(onTap = { latest.value(at(it.x, size.width)); done.value() })
            }.pointerInput(range) {
                detectHorizontalDragGestures(
                    onDragStart = { latest.value(at(it.x, size.width)) },
                    onDragEnd = { done.value() },
                    onDragCancel = { done.value() },
                ) { change, _ -> change.consume(); latest.value(at(change.position.x, size.width)) }
            }),
        contentAlignment = Alignment.CenterStart,
    ) {
        val knob = 18.dp
        val track = RoundedCornerShape(if (square) 0.dp else 2.dp)
        Box(Modifier.fillMaxWidth().padding(horizontal = knob / 2).height(4.dp).clip(track).background(Ui.Border))
        Box(Modifier.padding(start = knob / 2).width((maxWidth - knob) * f).height(4.dp).clip(track).background(Ui.Red))
        Box(Modifier.offset(x = (maxWidth - knob) * f).size(knob).clip(if (square) RoundedCornerShape(0.dp) else CircleShape).background(Ui.Red))
    }
}

/** A one-line text field in the design's shape, with an optional [label] above it. */
@Composable
fun Field(value: String, hint: String, mono: Boolean = false, digits: Boolean = false, label: String? = null, number: Boolean = false,
          modifier: Modifier = Modifier, onChange: (String) -> Unit) {
    if (label != null) {
        androidx.compose.foundation.layout.Column(modifier, verticalArrangement = androidx.compose.foundation.layout.Arrangement.spacedBy(6.dp)) {
            Caption(label)
            Field(value, hint, mono, digits, null, number, Modifier, onChange)
        }
        return
    }
    val st = Ui.theme.style
    val shape = RoundedCornerShape(minOf(st.button, 12f).dp)
    val bg = if (st.fill == Fill.TONAL) Voice.tonal else Ui.Black
    Box(
        modifier.fillMaxWidth().clip(shape).background(bg)
            .then(if (st.fill == Fill.TONAL) Modifier else Modifier.border(1.dp, if (st.fill == Fill.RULE) Ui.Chalk.copy(alpha = 0.35f) else Ui.Border, shape))
            .padding(horizontal = 14.dp, vertical = 12.dp)
    ) {
        val font = if (mono) Type.Mono else Type.Sans
        if (value.isEmpty()) Text(hint, color = Ui.Muted, fontSize = 15.sp, fontFamily = font)
        BasicTextField(value, onChange, singleLine = true, textStyle = TextStyle(color = Ui.Chalk, fontSize = 15.sp, fontFamily = font),
            cursorBrush = SolidColor(Ui.Red), modifier = Modifier.fillMaxWidth(),
            // Numbers only (a PIN): the number pad instead of letters.
            keyboardOptions = when {
                digits -> androidx.compose.foundation.text.KeyboardOptions(keyboardType = androidx.compose.ui.text.input.KeyboardType.NumberPassword)
                number -> androidx.compose.foundation.text.KeyboardOptions(keyboardType = androidx.compose.ui.text.input.KeyboardType.Decimal)
                else -> androidx.compose.foundation.text.KeyboardOptions.Default
            })
    }
}

/**
 * A tap target in the tool bar: it lightens while pressed and nothing else. No hover or focus
 * ring: the S Pen hovering over a button and leaving the screen's range right there never
 * tells the button it left, and the ring would stay behind on a tool that isn't selected.
 */
@androidx.compose.runtime.Composable
@OptIn(androidx.compose.foundation.ExperimentalFoundationApi::class)
fun Modifier.toolTap(enabled: Boolean = true, onLongClick: (() -> Unit)? = null, onClick: () -> Unit): Modifier {
    val source = androidx.compose.runtime.remember { androidx.compose.foundation.interaction.MutableInteractionSource() }
    val pressed = source.collectIsPressedAsState()
    return this.background(if (pressed.value) Ui.Chalk.copy(alpha = 0.16f) else Color.Transparent)
        .combinedClickable(interactionSource = source, indication = null, enabled = enabled, onLongClick = onLongClick, onClick = onClick)
}
