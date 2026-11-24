package dev.n0bridge.n0te

import android.content.Context
import android.graphics.Canvas
import android.graphics.Paint
import android.graphics.drawable.Drawable
import android.text.style.ReplacementSpan
import android.util.LruCache
import ru.noties.jlatexmath.JLatexMathAndroid
import ru.noties.jlatexmath.JLatexMathDrawable

/**
 * LaTeX formulas (`$…$`, `$$…$$`) drawn with JLaTeXMath, cached by formula, size and colour.
 * A formula that doesn't parse stays as its source text.
 */
object TexMath {
    class Formula(val drawable: Drawable, val width: Int, val height: Int, val depth: Int)

    private val cache = LruCache<String, Any>(256)
    private val invalid = Any()
    @Volatile private var ready = false

    fun init(ctx: Context) {
        ready = runCatching { JLatexMathAndroid.init(ctx.applicationContext) }.isSuccess
    }

    fun drawable(latex: String, size: Float, color: Int, display: Boolean): Formula? {
        if (!ready || latex.isBlank()) return null
        val key = "$display|${(size * 4).toInt()}|$color|$latex"
        cache.get(key)?.let { return it as? Formula }
        val f = runCatching {
            val d = JLatexMathDrawable.builder(if (display) "\\displaystyle $latex" else latex).textSize(size).color(color).padding(0).build()
            d.setBounds(0, 0, d.intrinsicWidth, d.intrinsicHeight)
            Formula(d, d.intrinsicWidth, d.intrinsicHeight, d.icon().iconDepth)
        }.getOrNull()
        cache.put(key, f ?: invalid)
        return f
    }
}

/** A formula drawn in place of its source (size and colour follow the text around it). */
open class MathSpan(private val latex: String, private val display: Boolean) : ReplacementSpan() {
    private fun formula(paint: Paint) = TexMath.drawable(latex, paint.textSize * (if (display) 1.15f else 1f), paint.color, display)

    override fun getSize(paint: Paint, text: CharSequence?, start: Int, end: Int, fm: Paint.FontMetricsInt?): Int {
        val f = formula(paint) ?: return paint.measureText(latex).toInt()
        if (fm != null) {
            paint.getFontMetricsInt(fm)
            val ascent = -(f.height - f.depth)
            if (ascent < fm.ascent) { fm.ascent = ascent; fm.top = ascent }
            if (f.depth > fm.descent) { fm.descent = f.depth; fm.bottom = f.depth }
        }
        return f.width
    }

    override fun draw(canvas: Canvas, text: CharSequence?, start: Int, end: Int, x: Float, top: Int, y: Int, bottom: Int, paint: Paint) {
        val f = formula(paint)
        if (f == null) { canvas.drawText(latex, x, y.toFloat(), paint); return }
        canvas.save()
        canvas.translate(x, (y - (f.height - f.depth)).toFloat())
        f.drawable.draw(canvas)
        canvas.restore()
    }
}
