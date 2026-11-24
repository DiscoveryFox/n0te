package dev.n0bridge.n0te

import androidx.compose.foundation.background
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.nativeCanvas
import androidx.compose.ui.unit.dp
import uniffi.n0te_ffi.FfiBrush
import uniffi.n0te_ffi.strokeLayers
import uniffi.n0te_ffi.strokeOutline

/**
 * A sample squiggle drawn exactly like the page draws it (outlines from the core, pencil grain),
 * light → hard → light pressure. [version] forces a redraw when the look changed.
 */
@Composable
fun PenSample(t: PenType, color: Int, width: Float, modifier: Modifier, alpha: Int = t.alpha, version: Int = 0) {
    @Suppress("UNUSED_VARIABLE") val v = version
    androidx.compose.foundation.Canvas(modifier.clip(RoundedCornerShape(8.dp)).background(Ui.Paper)) {
        val k = density * 1.15f // px per point, about the page at "fit width"
        val wPt = size.width / k; val hPt = size.height / k
        val n = 56
        val samples = FloatArray(n * 4)
        for (i in 0 until n) {
            val f = i / (n - 1f)
            samples[i * 4] = wPt * (0.08f + 0.84f * f)
            samples[i * 4 + 1] = hPt * (0.5f + 0.26f * kotlin.math.sin(f * 2 * Math.PI).toFloat())
            samples[i * 4 + 2] = 0.12f + 0.5f * kotlin.math.sin(f * Math.PI).toFloat()
            samples[i * 4 + 3] = i * 8f
        }
        val argb = (color and 0x00FFFFFF) or (alpha shl 24)
        // The fountain pen's options shape the sample like a real stroke.
        if (t == PenType.FOUNTAIN) Fountain.apply(samples, n)
        val brush = FfiBrush(width * t.widthMul, if (t == PenType.FOUNTAIN) Fountain.thinning() else t.thinning, argb.toUInt(), t.kind)
        val bytes = bytesOf(samples)
        val outer = InkStroke.pathOf(floatsOf(strokeOutline(brush, bytes)))
        val nc = drawContext.canvas.nativeCanvas
        nc.save(); nc.scale(k, k)
        if (t.kind.toInt() == NotebookView.KIND_PENCIL) {
            Pencil.draw(nc, Pencil.paints(Pencil.texelFor(k)), outer, strokeLayers(brush, bytes).map { InkStroke.pathOf(floatsOf(it)) }, argb)
        } else {
            nc.drawPath(outer, android.graphics.Paint(android.graphics.Paint.ANTI_ALIAS_FLAG).apply { this.color = argb })
        }
        nc.restore()
    }
}

/** The alpha a new stroke of [t] gets (the pencil's comes from the settings). */
fun PenType.inkAlpha(): Int = if (kind.toInt() == NotebookView.KIND_PENCIL) (Settings.pencilOpacity * 255f).toInt().coerceIn(40, 255) else alpha
