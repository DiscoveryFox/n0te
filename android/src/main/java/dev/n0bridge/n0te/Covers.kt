package dev.n0bridge.n0te

import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.unit.dp

/** A notebook's first page as saved when it was last closed (the library's and the split menu's cover). */
fun coverThumb(id: String) = java.io.File(java.io.File(N0te.root.parentFile, "thumbs"), "$id.png")

/**
 * A cover: the first page (saved when the notebook was last closed) or a preset — dot grid,
 * the title's initial in dots, stripes, or a colour.
 */
@Composable
fun CoverArt(cover: String, title: String, id: String, modifier: Modifier) {
    val file = coverThumb(id)
    val stamp = file.lastModified()
    val thumb by androidx.compose.runtime.produceState<androidx.compose.ui.graphics.ImageBitmap?>(null, id, stamp, cover) {
        value = if (cover.isBlank() || cover == "page") kotlinx.coroutines.withContext(kotlinx.coroutines.Dispatchers.IO) {
            runCatching { android.graphics.BitmapFactory.decodeFile(file.path)?.asImageBitmap() }.getOrNull()
        } else null
    }
    androidx.compose.foundation.Canvas(modifier.clip(RoundedCornerShape(maxOf(0f, minOf(Ui.theme.style.card - 8f, 10f)).dp))) {
        val initial = title.trim().firstOrNull { it.isLetterOrDigit() }?.toString() ?: "N"
        fun dotsInitial(color: Color, cellFrac: Float = 0.085f) {
            val cell = size.minDimension * cellFrac
            with(DotFont) { drawDots(initial, androidx.compose.ui.geometry.Offset((size.width - cell * 5) / 2, (size.height - cell * 7) / 2), cell, color) }
        }
        when (cover) {
            "dots" -> {
                drawRect(Color(0xFF111113))
                val step = 14.dp.toPx()
                var y = step / 2
                while (y < size.height) { var x = step / 2; while (x < size.width) { drawCircle(Color(0xFF3A3A40), 1.6.dp.toPx(), androidx.compose.ui.geometry.Offset(x, y)); x += step }; y += step }
                drawCircle(Ui.Red, 5.dp.toPx(), androidx.compose.ui.geometry.Offset(size.width - step * 1.5f, step * 1.5f))
            }
            "initial" -> { drawRect(Color(0xFF111113)); dotsInitial(Color(0xFFF2F2F4)); drawCircle(Ui.Red, 4.dp.toPx(), androidx.compose.ui.geometry.Offset(size.width * 0.78f, size.height * 0.2f)) }
            "stripes" -> {
                drawRect(Color(0xFF111113))
                val w = 10.dp.toPx()
                var x = -size.height
                while (x < size.width) {
                    drawLine(Ui.Red, androidx.compose.ui.geometry.Offset(x, size.height), androidx.compose.ui.geometry.Offset(x + size.height, 0f), w * 0.5f)
                    x += w * 2.2f
                }
            }
            "black", "red", "blue", "green", "orange", "purple" -> {
                drawRect(Color(LibraryActivity.COVER_COLORS.getValue(cover)))
                dotsInitial(Color(0xE6FFFFFF), 0.07f)
            }
            else -> {
                // First page (or plain paper until there is one).
                drawRect(Ui.Paper)
                val t = thumb
                if (t != null) {
                    val k = size.width / t.width
                    drawImage(t, dstSize = androidx.compose.ui.unit.IntSize(size.width.toInt(), (t.height * k).toInt()), filterQuality = androidx.compose.ui.graphics.FilterQuality.Medium)
                } else {
                    var y = 26.dp.toPx()
                    repeat(6) { drawLine(Color(0xFFE3E7EE), androidx.compose.ui.geometry.Offset(14.dp.toPx(), y), androidx.compose.ui.geometry.Offset(size.width - 14.dp.toPx(), y), 1.dp.toPx()); y += 18.dp.toPx() }
                }
            }
        }
    }
}

/** When something happened, as a short German phrase ("vor 5 min", "gestern", "3. Nov."). */
fun timeAgo(ms: Long): String {
    val s = (System.currentTimeMillis() - ms) / 1000
    return when {
        s < 60 -> "gerade eben"
        s < 3600 -> "vor ${s / 60} min"
        s < 86_400 -> "vor ${s / 3600} h"
        s < 2 * 86_400 -> "gestern"
        s < 14 * 86_400 -> "vor ${s / 86_400} Tagen"
        else -> java.text.SimpleDateFormat("d. MMM", java.util.Locale.GERMAN).format(java.util.Date(ms))
    }
}
