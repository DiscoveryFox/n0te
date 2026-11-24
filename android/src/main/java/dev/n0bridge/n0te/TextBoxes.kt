package dev.n0bridge.n0te

import android.graphics.Canvas
import android.graphics.Typeface
import android.text.Layout
import android.text.SpannableStringBuilder
import android.text.Spanned
import android.text.StaticLayout
import android.text.TextPaint
import android.text.style.BackgroundColorSpan
import android.text.style.BulletSpan
import android.text.style.ForegroundColorSpan
import android.text.style.LeadingMarginSpan
import android.text.style.RelativeSizeSpan
import android.text.style.StyleSpan
import android.text.style.TypefaceSpan
import uniffi.n0te_ffi.FfiTextBox

/**
 * Text boxes typed on the laptop, drawn on the page (sheet coordinates) with the same small
 * Markdown as the laptop: headings, lists, quotes, code blocks, **bold**, *italic*, `code`,
 * $formula$. Layouts are cached by content.
 */
object TextBoxes {
    private const val INK = 0xFF111114.toInt()
    private const val MUTED = 0xFF8A8A92.toInt()
    private const val ACCENT = 0xFFD71921.toInt()
    private const val TINT = 0xFFF0F0F3.toInt()
    private val cache = object : android.util.LruCache<String, StaticLayout>(256) {}
    /** A line that is only a display formula. */
    private val DISPLAY = Regex("^\\$\\$[^$]+\\$\\$$")

    fun draw(c: Canvas, boxes: List<FfiTextBox>, skip: String? = null) {
        for (b in boxes) {
            if (b.id == skip) continue
            val layout = layout(b)
            c.save()
            c.translate(b.x, b.y)
            layout.draw(c)
            c.restore()
        }
    }

    /** The box's area on its sheet (an empty box keeps one line's height, so it can be hit). */
    fun bounds(b: FfiTextBox): android.graphics.RectF {
        val h = layout(b).height.toFloat().coerceAtLeast(b.size * 1.5f)
        return android.graphics.RectF(b.x, b.y, b.x + b.w, b.y + h)
    }

    private fun layout(b: FfiTextBox): StaticLayout {
        val key = "${b.id}|${b.w}|${b.size}|${Settings.formulaCenter}|${b.text}"
        cache.get(key)?.let { return it }
        val paint = TextPaint(TextPaint.ANTI_ALIAS_FLAG).apply { color = INK; textSize = b.size; typeface = Typeface.SANS_SERIF }
        val text = render(b.text)
        val l = StaticLayout.Builder.obtain(text, 0, text.length, paint, b.w.toInt().coerceAtLeast(20))
            .setAlignment(Layout.Alignment.ALIGN_NORMAL)
            .setLineSpacing(0f, 1.15f)
            .setIncludePad(false)
            .build()
        cache.put(key, l)
        return l
    }

    /** Markdown → styled text. */
    fun render(md: String): CharSequence {
        val out = SpannableStringBuilder()
        var inCode = false
        val lines = md.split('\n')
        for ((i, raw) in lines.withIndex()) {
            if (raw.trimStart().startsWith("```")) { inCode = !inCode; continue }
            val start = out.length
            val t = raw.trimStart()
            when {
                inCode -> {
                    out.append(raw)
                    out.setSpan(TypefaceSpan("monospace"), start, out.length, Spanned.SPAN_EXCLUSIVE_EXCLUSIVE)
                    out.setSpan(BackgroundColorSpan(TINT), start, out.length, Spanned.SPAN_EXCLUSIVE_EXCLUSIVE)
                }
                t.startsWith("### ") -> heading(out, t.removePrefix("### "), 1.15f)
                t.startsWith("## ") -> heading(out, t.removePrefix("## "), 1.3f)
                t.startsWith("# ") -> heading(out, t.removePrefix("# "), 1.6f)
                t.startsWith("- [ ] ") || t.startsWith("- [x] ") || t.startsWith("- [X] ") -> {
                    val done = t[3] != ' '
                    out.append(if (done) "☑ " else "☐ ")
                    out.setSpan(ForegroundColorSpan(ACCENT), start, out.length, Spanned.SPAN_EXCLUSIVE_EXCLUSIVE)
                    val s = out.length
                    inline(out, t.substring(6))
                    if (done) out.setSpan(ForegroundColorSpan(MUTED), s, out.length, Spanned.SPAN_EXCLUSIVE_EXCLUSIVE)
                }
                t.startsWith("- ") || t.startsWith("* ") -> {
                    inline(out, t.substring(2))
                    out.setSpan(BulletSpan(14, ACCENT), start, out.length, Spanned.SPAN_EXCLUSIVE_EXCLUSIVE)
                }
                t.startsWith("> ") -> {
                    inline(out, t.substring(2))
                    out.setSpan(StyleSpan(Typeface.ITALIC), start, out.length, Spanned.SPAN_EXCLUSIVE_EXCLUSIVE)
                    out.setSpan(ForegroundColorSpan(MUTED), start, out.length, Spanned.SPAN_EXCLUSIVE_EXCLUSIVE)
                    out.setSpan(LeadingMarginSpan.Standard(12), start, out.length, Spanned.SPAN_EXCLUSIVE_EXCLUSIVE)
                }
                else -> inline(out, raw)
            }
            if (Settings.formulaCenter && DISPLAY.matches(raw.trim())) {
                out.setSpan(android.text.style.AlignmentSpan.Standard(Layout.Alignment.ALIGN_CENTER), start, out.length, Spanned.SPAN_EXCLUSIVE_EXCLUSIVE)
            }
            if (i < lines.size - 1) out.append('\n')
        }
        return out
    }

    private fun heading(out: SpannableStringBuilder, text: String, rel: Float) {
        val start = out.length
        inline(out, text)
        out.setSpan(StyleSpan(Typeface.BOLD), start, out.length, Spanned.SPAN_EXCLUSIVE_EXCLUSIVE)
        out.setSpan(RelativeSizeSpan(rel), start, out.length, Spanned.SPAN_EXCLUSIVE_EXCLUSIVE)
    }

    /** **bold**, *italic*, `code`, $math$ inside a line. */
    private fun inline(out: SpannableStringBuilder, line: String) {
        var rest = line
        while (rest.isNotEmpty()) {
            val marks = listOf("**", "`", "$$", "$", "*")
            val next = marks.mapNotNull { m -> rest.indexOf(m).takeIf { it >= 0 }?.let { it to m } }
                .sortedWith(compareBy({ it.first }, { -it.second.length }))
                .firstOrNull { (i, m) -> rest.indexOf(m, i + m.length) > 0 }
            if (next == null) { out.append(rest); return }
            val (i, m) = next
            out.append(rest.substring(0, i))
            val end = rest.indexOf(m, i + m.length)
            val s = out.length
            out.append(rest.substring(i + m.length, end))
            val span: List<Any> = when (m) {
                "**" -> listOf(StyleSpan(Typeface.BOLD))
                "*" -> listOf(StyleSpan(Typeface.ITALIC))
                "`" -> listOf(TypefaceSpan("monospace"), BackgroundColorSpan(TINT))
                else -> listOf(MathSpan(rest.substring(i + m.length, end), m == "$$")) // a LaTeX formula
            }
            span.forEach { out.setSpan(it, s, out.length, Spanned.SPAN_EXCLUSIVE_EXCLUSIVE) }
            rest = rest.substring(end + m.length)
        }
    }
}
