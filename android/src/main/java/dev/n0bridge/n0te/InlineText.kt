package dev.n0bridge.n0te

import android.annotation.SuppressLint
import android.content.Context
import android.graphics.Canvas
import android.graphics.Paint
import android.graphics.Typeface
import android.graphics.drawable.GradientDrawable
import android.os.Build
import android.text.Editable
import android.text.InputType
import android.text.TextWatcher
import android.text.style.BackgroundColorSpan
import android.text.style.ForegroundColorSpan
import android.text.style.RelativeSizeSpan
import android.text.style.ReplacementSpan
import android.text.style.StyleSpan
import android.text.style.TypefaceSpan
import android.util.TypedValue
import android.view.Gravity
import android.view.MotionEvent
import android.view.View
import android.view.ViewConfiguration
import android.view.inputmethod.EditorInfo
import android.view.inputmethod.InputMethodManager
import android.widget.EditText
import kotlin.math.hypot

/**
 * Text typed straight onto the page, like in a word processor: an EditText laid over a text
 * box or a whole text page at the page's zoom. Markdown stays the stored format (laptop, PDF,
 * sync), but it is shown formatted while typing: headings big, **bold** bold, lists with
 * bullets and checkboxes. The marks themselves (#, **, `) only show, faintly, on the line
 * with the cursor. A drag that isn't a text selection scrolls / zooms the page underneath.
 */
@SuppressLint("ViewConstructor", "AppCompatCustomView")
class InlineTextEditor(ctx: Context, private val page: NotebookView) : EditText(ctx) {
    /** The text changed by typing or a formatting button (not by [show]). */
    var onText: ((String) -> Unit)? = null
    /** The cursor's paragraph kind changed ("p", "h1", "ul", …) — for the toolbar. */
    var onBlock: ((String) -> Unit)? = null
    /** The inline formats around the cursor ("**", "*", "`", "$") — for the toolbar. */
    var onInline: ((Set<String>) -> Unit)? = null
    private var inlineNow = emptySet<String>()

    private var ready = false
    private var busy = false
    private var quiet = false
    private var cursorLine = -2
    private var block = ""
    private val slop = ViewConfiguration.get(ctx).scaledTouchSlop.toFloat()
    private val longPress = ViewConfiguration.getLongPressTimeout().toLong()

    // Touch: drags go to the page (scroll / zoom), taps and selections stay here.
    private var down: MotionEvent? = null
    private var forwarding = false

    init {
        background = null
        setPadding(0, 0, 0, 0)
        includeFontPadding = false
        setLineSpacing(0f, 1.15f)
        typeface = Typeface.SANS_SERIF
        setTextColor(INK)
        setHintTextColor(0x998A8A92.toInt())
        highlightColor = 0x44D71921
        gravity = Gravity.TOP or Gravity.START
        inputType = InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_FLAG_MULTI_LINE or
            InputType.TYPE_TEXT_FLAG_CAP_SENTENCES or InputType.TYPE_TEXT_FLAG_AUTO_CORRECT
        imeOptions = EditorInfo.IME_FLAG_NO_EXTRACT_UI or EditorInfo.IME_FLAG_NO_FULLSCREEN
        isVerticalScrollBarEnabled = false
        isHorizontalScrollBarEnabled = false
        // A pen drag scrolls the page (no system handwriting-to-text taking over).
        if (Build.VERSION.SDK_INT >= 33) isAutoHandwritingEnabled = false
        if (Build.VERSION.SDK_INT >= 29) {
            textCursorDrawable = GradientDrawable().apply { setColor(0xFFD71921.toInt()); setSize((2 * resources.displayMetrics.density).toInt(), 1) }
        }
        addTextChangedListener(object : TextWatcher {
            private var newlineAt = -1
            override fun beforeTextChanged(s: CharSequence, start: Int, count: Int, after: Int) {}
            override fun onTextChanged(s: CharSequence, start: Int, before: Int, count: Int) {
                newlineAt = if (!busy && !quiet && before == 0 && count == 1 && s[start] == '\n') start else -1
            }
            override fun afterTextChanged(s: Editable) {
                if (busy) return
                busy = true
                try {
                    if (newlineAt >= 0) { val at = newlineAt; newlineAt = -1; continueList(s, at) }
                    restyle()
                } finally { busy = false }
                if (!quiet) onText?.invoke(s.toString())
            }
        })
        ready = true
    }

    /** The whole text, never scrolled inside: the page scrolls instead. */
    override fun onMeasure(widthMeasureSpec: Int, heightMeasureSpec: Int) {
        super.onMeasure(widthMeasureSpec, MeasureSpec.makeMeasureSpec(0, MeasureSpec.UNSPECIFIED))
    }

    /** Shows [text] without reporting it as typed (opening, or a change from the laptop). */
    fun show(text: String, keepCursor: Boolean) {
        val cur = this.text
        if (text == cur?.toString()) return
        quiet = true
        try {
            if (cur == null || cur.isEmpty() || !keepCursor) {
                setText(text)
                setSelection(text.length)
            } else {
                // Only the part that differs (a change from the laptop, an undo): no setText,
                // which would restart the keyboard and drop what is being typed.
                var p = 0
                val max = minOf(cur.length, text.length)
                while (p < max && cur[p] == text[p]) p++
                var q = 0
                while (q < max - p && cur[cur.length - 1 - q] == text[text.length - 1 - q]) q++
                cur.replace(p, cur.length - q, text, p, text.length - q)
            }
        } finally { quiet = false }
    }

    /** At the page's zoom: font size, width, padding (sheet units × [scale]), screen position. */
    fun place(x: Float, y: Float, w: Float, size: Float, minH: Float, pad: FloatArray, scale: Float) {
        val px = size * scale
        if (kotlin.math.abs(textSize - px) > 0.01f) setTextSize(TypedValue.COMPLEX_UNIT_PX, px)
        val p = IntArray(4) { (pad[it] * scale).toInt() }
        if (paddingLeft != p[0] || paddingTop != p[1] || paddingRight != p[2] || paddingBottom != p[3]) setPadding(p[0], p[1], p[2], p[3])
        val wantW = (w * scale).toInt().coerceAtLeast(20)
        val wantMin = (minH * scale).toInt()
        if (minimumHeight != wantMin) minHeight = wantMin
        val lp = layoutParams
        if (lp != null && lp.width != wantW) { lp.width = wantW; layoutParams = lp }
        translationX = x
        translationY = y
    }

    fun focusAndShowKeyboard(atEnd: Boolean) {
        requestFocus()
        if (atEnd) setSelection(text?.length ?: 0)
        post { (context.getSystemService(Context.INPUT_METHOD_SERVICE) as InputMethodManager).showSoftInput(this, InputMethodManager.SHOW_IMPLICIT) }
    }

    fun hideKeyboard() {
        (context.getSystemService(Context.INPUT_METHOD_SERVICE) as InputMethodManager).hideSoftInputFromWindow(windowToken, 0)
        clearFocus()
    }

    /** The cursor's bottom on screen (for keeping it above the keyboard), or null. */
    fun cursorBottomOnScreen(): Float? {
        val l = layout ?: return null
        val sel = selectionEnd.takeIf { it >= 0 } ?: return null
        return translationY + paddingTop + l.getLineBottom(l.getLineForOffset(sel))
    }

    // --- touch ------------------------------------------------------------------------------------

    @SuppressLint("ClickableViewAccessibility")
    override fun onTouchEvent(ev: MotionEvent): Boolean {
        when (ev.actionMasked) {
            MotionEvent.ACTION_DOWN -> {
                forwarding = false
                down?.recycle()
                down = MotionEvent.obtain(ev)
            }
            MotionEvent.ACTION_MOVE, MotionEvent.ACTION_POINTER_DOWN -> {
                val d = down
                val selecting = hasSelection() && ev.pointerCount == 1
                val moved = d != null && hypot(ev.rawX - d.rawX, ev.rawY - d.rawY) > slop
                val early = d != null && ev.eventTime - d.downTime < longPress
                if (!forwarding && d != null && !selecting && (ev.pointerCount > 1 || (moved && early))) {
                    // A scroll / pinch: hand it to the page, starting from the down.
                    forwarding = true
                    val cancel = MotionEvent.obtain(ev).apply { action = MotionEvent.ACTION_CANCEL }
                    super.onTouchEvent(cancel)
                    cancel.recycle()
                    page.navigateOnly = true
                    toPage(d).let { page.dispatchTouchEvent(it); it.recycle() }
                }
            }
        }
        if (forwarding) {
            toPage(ev).let { page.dispatchTouchEvent(it); it.recycle() }
            if (ev.actionMasked == MotionEvent.ACTION_UP || ev.actionMasked == MotionEvent.ACTION_CANCEL) {
                forwarding = false
                page.navigateOnly = false
            }
            return true
        }
        if (ev.actionMasked == MotionEvent.ACTION_UP && toggleCheckbox(ev)) {
            val cancel = MotionEvent.obtain(ev).apply { action = MotionEvent.ACTION_CANCEL }
            super.onTouchEvent(cancel)
            cancel.recycle()
            return true
        }
        return super.onTouchEvent(ev)
    }

    /** The event in the page view's coordinates (this view sits at its translation). */
    private fun toPage(ev: MotionEvent): MotionEvent = MotionEvent.obtain(ev).apply { offsetLocation(translationX + left - page.left, translationY + top - page.top) }

    /** A tap on a checklist box ticks it. */
    private fun toggleCheckbox(ev: MotionEvent): Boolean {
        val s = text ?: return false
        val off = getOffsetForPosition(ev.x, ev.y)
        val ls = lineStart(s, off)
        val m = TODO.find(s.subSequence(ls, lineEnd(s, ls))) ?: return false
        val box = ls + m.groupValues[1].length
        if (off < ls || off > box + 6 || ev.x - paddingLeft > (layout?.getPrimaryHorizontal(box + 6) ?: 0f) + 4f) return false
        val done = s[box + 3] != ' '
        s.replace(box + 3, box + 4, if (done) " " else "x")
        return true
    }

    // --- formatting ---------------------------------------------------------------------------

    /**
     * Bold / italic / code like in a word processor. With a selection: on (again: off). At the
     * cursor: on (what is typed next is bold); again right away: off; inside bold text: bold
     * ends at the cursor.
     */
    fun toggleInline(m: String) {
        val s = text ?: return
        if (!hasFocus()) focusAndShowKeyboard(atEnd = selectionStart < 0)
        val a = selectionStart.coerceAtLeast(0); val b = selectionEnd.coerceAtLeast(0)
        if (a == b) {
            // An empty pair just opened here: close it again.
            if (a >= m.length && a + m.length <= s.length && s.substring(a - m.length, a) == m && s.substring(a, a + m.length) == m) {
                s.delete(a - m.length, a + m.length)
                return
            }
            val range = pairAround(s, a, m)
            if (range != null) {
                val close = range.second - m.length
                if (a == close) setSelection(range.second) // at its end: step out
                else { s.insert(a, m + m); setSelection(a + m.length) } // in the middle: end it here, the rest stays
                return
            }
            s.insert(a, m + m)
            setSelection(a + m.length)
            return
        }
        val wrapped = a >= m.length && b + m.length <= s.length && s.substring(a - m.length, a) == m && s.substring(b, b + m.length) == m
        if (wrapped) {
            s.delete(b, b + m.length); s.delete(a - m.length, a); setSelection(a - m.length, b - m.length)
        } else {
            s.insert(b, m); s.insert(a, m); setSelection(a + m.length, b + m.length)
        }
    }

    /** Paragraph kind of the selected lines: p, h1–h3, ul, ol, todo, quote (lists toggle). */
    fun setBlock(kind: String) {
        val s = text ?: return
        if (!hasFocus()) focusAndShowKeyboard(atEnd = selectionStart < 0)
        val a = selectionStart.coerceAtLeast(0); val b = selectionEnd.coerceAtLeast(0)
        val starts = ArrayList<Int>()
        var ls = lineStart(s, a)
        while (true) {
            starts += ls
            val le = lineEnd(s, ls)
            if (le >= b || le >= s.length) break
            ls = le + 1
        }
        val toggleOff = kind in setOf("ul", "ol", "todo", "quote") && starts.all { kindOf(s, it) == kind }
        var n = 1
        // From the last line up, so earlier offsets stay valid.
        for (st in starts.asReversed()) {
            val le = lineEnd(s, st)
            val prefix = PREFIX.find(s.subSequence(st, le))?.value ?: ""
            val add = if (toggleOff) "" else when (kind) {
                "h1" -> "# "; "h2" -> "## "; "h3" -> "### "
                "ul" -> "- "; "todo" -> "- [ ] "; "quote" -> "> "
                "ol" -> "${starts.size - (n++) + 1}. "
                else -> ""
            }
            s.replace(st, st + prefix.length, add)
        }
    }

    /** The pair of [m] marks around [at] on its line (absolute start, end incl. marks), or null. */
    private fun pairAround(s: CharSequence, at: Int, m: String): Pair<Int, Int>? {
        val ls = lineStart(s, at)
        val line = s.subSequence(ls, lineEnd(s, ls)).toString()
        val r = inlineRanges(line).firstOrNull { (x0, x1, mark) -> mark == m && at - ls >= x0 + m.length && at - ls <= x1 - m.length } ?: return null
        return ls + r.first to ls + r.second
    }

    /** The kind of the paragraph at the cursor. */
    fun blockAtCursor(): String { val s = text ?: return "p"; return kindOf(s, lineStart(s, selectionStart.coerceAtLeast(0))) }

    /** Enter in a list continues it; Enter on an empty item ends the list. */
    private fun continueList(s: Editable, nl: Int) {
        val ls = lineStart(s, nl)
        val m = LIST.find(s.subSequence(ls, nl)) ?: return
        val indent = m.groupValues[1]
        val marker = m.groupValues[2]
        if (m.groupValues[4].isBlank()) {
            s.replace(ls, nl + 1, "")
            return
        }
        val next = when {
            TODO_MARK.matches(marker) -> "- [ ] "
            marker.first().isDigit() -> "${(m.groupValues[3].toIntOrNull() ?: 0) + 1}. "
            else -> marker
        }
        s.insert(nl + 1, indent + next)
    }

    // --- styling ------------------------------------------------------------------------------

    override fun onSelectionChanged(selStart: Int, selEnd: Int) {
        super.onSelectionChanged(selStart, selEnd)
        if (!ready || busy) return
        val s = text ?: return
        val line = if (hasFocus() && selStart >= 0) lineIndex(s, selStart) else -1
        if (line != cursorLine) {
            busy = true
            try { restyle() } finally { busy = false }
        }
        val k = blockAtCursor()
        if (k != block) { block = k; onBlock?.invoke(k) }
        val at = selStart.coerceAtLeast(0)
        val formats = listOf("**", "*", "`", "$").filter { pairAround(s, at, it) != null }.toSet()
        if (formats != inlineNow) { inlineNow = formats; onInline?.invoke(formats) }
    }

    override fun onFocusChanged(focused: Boolean, direction: Int, previouslyFocusedRect: android.graphics.Rect?) {
        super.onFocusChanged(focused, direction, previouslyFocusedRect)
        if (!ready) return
        busy = true
        try { restyle() } finally { busy = false }
    }

    private fun restyle() {
        val s = text ?: return
        for (sp in s.getSpans(0, s.length, Md::class.java)) s.removeSpan(sp)
        cursorLine = if (hasFocus() && selectionStart >= 0) lineIndex(s, selectionStart) else -1
        for (r in styleRuns(s, cursorLine, Settings.formulaCenter)) {
            if (r.end <= r.start || r.end > s.length) continue
            val span: Md = when (val st = r.style) {
                Style.Bold -> MdBold()
                Style.Italic -> MdItalic()
                Style.Mono -> MdMono()
                Style.Hide -> MdHide()
                is Style.Size -> MdSize(st.rel)
                is Style.Bg -> MdBg(st.color)
                is Style.Color -> MdColor(st.color)
                is Style.Glyph -> MdGlyph(st.glyph, st.color)
                is Style.Math -> MdMath(st.latex, st.display)
                Style.Center -> MdCenter()
            }
            s.setSpan(span, r.start, r.end, android.text.Spanned.SPAN_EXCLUSIVE_EXCLUSIVE)
        }
    }

    /** What a run of the text looks like (pure data, so the styling can be tested on the JVM). */
    sealed class Style {
        object Bold : Style()
        object Italic : Style()
        object Mono : Style()
        object Hide : Style()
        data class Size(val rel: Float) : Style()
        data class Bg(val color: Int) : Style()
        data class Color(val color: Int) : Style()
        data class Glyph(val glyph: String, val color: Int) : Style()
        /** A formula in place of `$…$` (`$$…$$`: display style). */
        data class Math(val latex: String, val display: Boolean) : Style()
        /** The paragraph is centred (a line that is only a display formula). */
        object Center : Style()
    }

    data class Run(val start: Int, val end: Int, val style: Style)

    companion object {
        const val INK = 0xFF111114.toInt()
        private const val MUTED = 0xFF8A8A92.toInt()
        private const val FAINT = 0xFFB4B4BC.toInt()
        private const val ACCENT = 0xFFD71921.toInt()
        private const val TINT = 0xFFF0F0F3.toInt()
        private val HEADING_N = Regex("^(#{1,3}) ")
        private val TODO = Regex("^(\\s*)- \\[[ xX]] ")
        private val TODO_MARK = Regex("- \\[[ xX]] ")
        private val BULLET = Regex("^(\\s*)[-*] ")
        private val QUOTE = Regex("^> ")
        private val NUMBER = Regex("^(\\s*)\\d+\\. ")
        private val DISPLAY_LINE = Regex("^\\$\\$[^$]+\\$\\$$")
        private val PREFIX = Regex("^(#{1,3} |\\s*- \\[[ xX]] |\\s*[-*] |\\s*\\d+\\. |> )")
        private val LIST = Regex("^(\\s*)(- \\[[ xX]] |[-*] |(\\d+)\\. |> )(.*)$")

        fun lineStart(s: CharSequence, at: Int): Int { var i = at.coerceIn(0, s.length); while (i > 0 && s[i - 1] != '\n') i--; return i }
        fun lineEnd(s: CharSequence, from: Int): Int { var i = from.coerceIn(0, s.length); while (i < s.length && s[i] != '\n') i++; return i }
        private fun lineIndex(s: CharSequence, at: Int): Int { var n = 0; for (i in 0 until at.coerceAtMost(s.length)) if (s[i] == '\n') n++; return n }

        /**
         * The styling of Markdown [s] with the cursor on line [cursorLine] (-1: none): headings,
         * lists, checkboxes, quotes, code, inline marks and formulas. Marks are faint on the
         * cursor's line and hidden elsewhere; formulas are drawn except on the cursor's line.
         */
        fun styleRuns(s: CharSequence, cursorLine: Int, centerDisplay: Boolean = true): List<Run> {
            val out = ArrayList<Run>()
            fun add(a: Int, b: Int, st: Style) { if (b > a) out += Run(a, b, st) }
            fun mark(a: Int, b: Int, visible: Boolean) {
                if (b <= a) return
                if (visible) { add(a, b, Style.Color(FAINT)); add(a, b, Style.Size(0.8f)) } else add(a, b, Style.Hide)
            }
            var ls = 0
            var line = 0
            var inCode = false
            while (ls <= s.length) {
                val le = lineEnd(s, ls)
                val text = s.subSequence(ls, le).toString()
                val here = line == cursorLine
                when {
                    text.trimStart().startsWith("```") -> { inCode = !inCode; mark(ls, le, here) }
                    inCode -> { add(ls, le, Style.Mono); add(ls, le, Style.Bg(TINT)) }
                    else -> {
                        if (centerDisplay && DISPLAY_LINE.matches(text.trim())) add(ls, le, Style.Center)
                        var content = ls
                        val level = headingLevel(text)
                        if (level > 0) {
                            add(ls, le, Style.Bold)
                            add(ls, le, Style.Size(when (level) { 1 -> 1.6f; 2 -> 1.3f; else -> 1.15f }))
                            mark(ls, ls + level + 1, here)
                            content = ls + level + 1
                        }
                        val todo = TODO.find(text)
                        val bullet = if (todo == null) BULLET.find(text) else null
                        val quote = if (todo == null && bullet == null) QUOTE.find(text) else null
                        val number = if (todo == null && bullet == null && quote == null) NUMBER.find(text) else null
                        when {
                            todo != null -> {
                                val b = ls + todo.groupValues[1].length
                                val done = text.getOrNull(todo.groupValues[1].length + 3)?.let { it != ' ' } == true
                                add(b, b + 6, Style.Glyph(if (done) "☑ " else "☐ ", ACCENT))
                                if (done) add(b + 6, le, Style.Color(MUTED))
                                content = b + 6
                            }
                            bullet != null -> {
                                val b = ls + bullet.groupValues[1].length
                                add(b, b + 2, Style.Glyph("•  ", ACCENT))
                                content = b + 2
                            }
                            quote != null -> {
                                add(ls, le, Style.Italic); add(ls, le, Style.Color(MUTED))
                                mark(ls, ls + quote.value.length, here)
                                content = ls + quote.value.length
                            }
                            number != null -> {
                                add(ls + number.groupValues[1].length, ls + number.value.length, Style.Color(ACCENT))
                                content = ls + number.value.length
                            }
                        }
                        if (content <= le) for ((a, b, m) in inlineRanges(s.subSequence(content, le).toString())) {
                            val x0 = content + a; val x1 = content + b
                            val in0 = x0 + m.length; val in1 = x1 - m.length
                            if ((m == "$" || m == "$$") && !here && in1 > in0) {
                                add(x0, x1, Style.Math(s.subSequence(in0, in1).toString(), m == "$$"))
                                continue
                            }
                            when (m) {
                                "**" -> add(in0, in1, Style.Bold)
                                "*" -> add(in0, in1, Style.Italic)
                                "`" -> { add(in0, in1, Style.Mono); add(in0, in1, Style.Bg(TINT)) }
                                "$", "$$" -> { add(in0, in1, Style.Mono); add(in0, in1, Style.Color(ACCENT)) }
                            }
                            mark(x0, in0, here)
                            mark(in1, x1, here)
                        }
                    }
                }
                if (le >= s.length) break
                ls = le + 1
                line++
            }
            return out
        }

        /** 1–3 for "# ", "## ", "### " headings, 0 otherwise. */
        fun headingLevel(line: CharSequence): Int = HEADING_N.find(line)?.groupValues?.get(1)?.length ?: 0

        /** The kind of the paragraph starting at [ls]. */
        fun kindOf(s: CharSequence, ls: Int): String {
            val line = s.subSequence(ls, lineEnd(s, ls))
            return when (headingLevel(line)) {
                1 -> "h1"
                2 -> "h2"
                3 -> "h3"
                else -> when {
                TODO.containsMatchIn(line) -> "todo"
                BULLET.containsMatchIn(line) -> "ul"
                NUMBER.containsMatchIn(line) -> "ol"
                QUOTE.containsMatchIn(line) -> "quote"
                else -> "p"
                }
            }
        }

        /** Paired inline marks in a line: (start, end incl. marks, mark) — **bold**, *italic*, `code`, $math$. */
        fun inlineRanges(line: String): List<Triple<Int, Int, String>> {
            val out = ArrayList<Triple<Int, Int, String>>()
            var i = 0
            while (i < line.length) {
                val m = when {
                    line.startsWith("**", i) -> "**"
                    line[i] == '`' -> "`"
                    line.startsWith("$$", i) -> "$$"
                    line[i] == '$' -> "$"
                    line[i] == '*' -> "*"
                    else -> null
                }
                if (m == null) { i++; continue }
                val end = line.indexOf(m, i + m.length)
                if (end > i + m.length - 1 && end > i) {
                    out += Triple(i, end + m.length, m)
                    i = end + m.length
                } else i += m.length
            }
            return out
        }
    }
}

/** Spans set by the editor's styling (removed and set again on every change). */
private interface Md
private class MdBold : StyleSpan(Typeface.BOLD), Md
private class MdItalic : StyleSpan(Typeface.ITALIC), Md
private class MdSize(rel: Float) : RelativeSizeSpan(rel), Md
private class MdMono : TypefaceSpan("monospace"), Md
private class MdBg(color: Int) : BackgroundColorSpan(color), Md
private class MdColor(color: Int) : ForegroundColorSpan(color), Md

/** Takes no room and draws nothing (a hidden Markdown mark). */
private class MdHide : ReplacementSpan(), Md {
    override fun getSize(paint: Paint, text: CharSequence?, start: Int, end: Int, fm: Paint.FontMetricsInt?): Int = 0
    override fun draw(canvas: Canvas, text: CharSequence?, start: Int, end: Int, x: Float, top: Int, y: Int, bottom: Int, paint: Paint) {}
}

/** Draws [glyph] in place of a list mark ("- " → "•", "- [ ] " → "☐"). */
private class MdGlyph(private val glyph: String, private val color: Int) : ReplacementSpan(), Md {
    override fun getSize(paint: Paint, text: CharSequence?, start: Int, end: Int, fm: Paint.FontMetricsInt?): Int {
        if (fm != null) paint.getFontMetricsInt(fm)
        return paint.measureText(glyph).toInt()
    }
    override fun draw(canvas: Canvas, text: CharSequence?, start: Int, end: Int, x: Float, top: Int, y: Int, bottom: Int, paint: Paint) {
        val c = paint.color
        paint.color = color
        canvas.drawText(glyph, x, y.toFloat(), paint)
        paint.color = c
    }
}

/** A centred paragraph (removed with the other styling spans). */
private class MdCenter : android.text.style.AlignmentSpan.Standard(android.text.Layout.Alignment.ALIGN_CENTER), Md

/** A formula in the editor (removed with the other styling spans). */
private class MdMath(latex: String, display: Boolean) : MathSpan(latex, display), Md
