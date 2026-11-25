package dev.n0bridge.n0te

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
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp

/** Page formats, the same list as on the laptop (from the core). Sizes in page points. */
object Formats {
    val all: List<uniffi.n0te_ffi.FfiPageFormat> by lazy { uniffi.n0te_ffi.pageFormats() }
    fun describe(w: Float, h: Float): String = uniffi.n0te_ffi.describePageSize(w, h)
    /** The format's id, "" for a size of its own. */
    fun idOf(w: Float, h: Float): String = uniffi.n0te_ffi.pageFormatId(w, h)
    fun mm(pt: Float) = pt * 25.4f / 72f
    fun pt(mm: Float) = mm * 72f / 25.4f
    /** [w] × [h] upright or on its side. */
    fun oriented(w: Float, h: Float, landscape: Boolean): Pair<Float, Float> {
        val (short, long) = if (w <= h) w to h else h to w
        return if (landscape) long to short else short to long
    }
    /** Sides a page may have (the core refuses others): 1 inch … 200 inches (about 5 m, the largest PDF page). */
    val sideMm = 25.4f..5080f
    /** "210 × 297 mm". */
    fun mmText(w: Float, h: Float) = "%.0f × %.0f mm".format(mm(w), mm(h))
}

/**
 * Picks a page size: upright or landscape, a format (A4, A5, …) or a size of its own in mm.
 * [onChange] gets the new size in points whenever a choice is made.
 */
@OptIn(ExperimentalLayoutApi::class)
@Composable
fun FormatPicker(w: Float, h: Float, modifier: Modifier = Modifier, onChange: (Float, Float) -> Unit) {
    val id = Formats.idOf(w, h)
    val landscape = w > h
    val square = kotlin.math.abs(w - h) < 1f
    var custom by remember { mutableStateOf(id.isEmpty()) }
    Column(modifier, verticalArrangement = Arrangement.spacedBy(10.dp)) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            OrientationChoice(false, !landscape, enabled = !square) { val (a, b) = Formats.oriented(w, h, false); onChange(a, b) }
            OrientationChoice(true, landscape, enabled = !square) { val (a, b) = Formats.oriented(w, h, true); onChange(a, b) }
            Spacer(Modifier.width(6.dp))
            Text(Formats.mmText(w, h), color = Ui.Muted, fontSize = 12.sp, fontFamily = Voice.metaFont)
        }
        FlowRow(horizontalArrangement = Arrangement.spacedBy(6.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
            Formats.all.forEach { f ->
                SmallChoice(f.name, !custom && id == f.id) {
                    custom = false
                    // A slide lies in landscape by nature; the others keep the page's way round.
                    val (a, b) = Formats.oriented(f.width, f.height, if (f.id == "slide") true else landscape)
                    onChange(a, b)
                }
            }
            SmallChoice("Eigene …", custom) { custom = true }
        }
        if (custom) CustomSize(w, h, onChange)
    }
}

/** Width and height in mm, applied with a button (typing "2" on the way to "210" must not resize the page). */
@Composable
private fun CustomSize(w: Float, h: Float, onChange: (Float, Float) -> Unit) {
    var tw by remember(w) { mutableStateOf("%.0f".format(Formats.mm(w))) }
    var th by remember(h) { mutableStateOf("%.0f".format(Formats.mm(h))) }
    val nw = tw.replace(',', '.').toFloatOrNull()
    val nh = th.replace(',', '.').toFloatOrNull()
    val ok = nw != null && nh != null && nw in Formats.sideMm && nh in Formats.sideMm
    val focus = androidx.compose.ui.platform.LocalFocusManager.current
    Row(verticalAlignment = Alignment.Bottom, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        Field(tw, "mm", number = true, label = "Breite (mm)", modifier = Modifier.width(110.dp)) { tw = it }
        Field(th, "mm", number = true, label = "Höhe (mm)", modifier = Modifier.width(110.dp)) { th = it }
        Btn("Übernehmen", BtnKind.SECONDARY, enabled = ok) { if (ok) { focus.clearFocus(); onChange(Formats.pt(nw!!), Formats.pt(nh!!)) } }
    }
    if (!ok) Text("Jede Seite zwischen 25 mm und 5 m", color = Ui.Muted, fontSize = 12.sp)
}

/** "Hoch" / "Quer" with a little page drawn the same way round. */
@Composable
private fun OrientationChoice(landscape: Boolean, on: Boolean, enabled: Boolean, onClick: () -> Unit) {
    val shape = RoundedCornerShape(minOf(Ui.theme.style.button, 10f).dp)
    Row(
        Modifier.clip(shape).background(if (on) Ui.Chalk else if (Ui.theme.style.fill == Fill.TONAL) Voice.tonal else Ui.Elevated)
            .clickable(enabled = enabled && !on, onClick = onClick).padding(horizontal = 10.dp, vertical = 7.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        val fg = if (on) Ui.Black else Ui.Chalk
        Box(Modifier.size(if (landscape) 16.dp else 12.dp, if (landscape) 12.dp else 16.dp).border(1.5.dp, fg, RoundedCornerShape(2.dp)))
        Text(Voice.cased(if (landscape) "Quer" else "Hoch"), color = fg, fontSize = 12.sp, fontFamily = Voice.labelFont,
            modifier = Modifier.padding(start = 7.dp))
    }
}
