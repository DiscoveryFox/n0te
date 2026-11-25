package dev.n0bridge.n0te

import android.graphics.Paint
import android.graphics.Path
import android.graphics.RectF
import android.graphics.RenderNode
import android.os.SystemClock
import uniffi.n0te_ffi.FfiBackground
import uniffi.n0te_ffi.FfiBrush
import uniffi.n0te_ffi.FfiOutlineRow
import uniffi.n0te_ffi.FfiSheetSpec
import uniffi.n0te_ffi.FfiStroke
import uniffi.n0te_ffi.NotebookHandle
import uniffi.n0te_ffi.SheetHandle
import java.nio.ByteBuffer
import java.nio.ByteOrder

/**
 * A stroke as drawn: its filled outline in sheet coordinates (points); pencils also carry their
 * middle band and core ([layers], see [Pencil]).
 */
/**
 * A stroke as drawn on a page. [ends]: where it starts and ends with the way it runs there (see
 * FfiStroke.ends: x0, y0, dx0, dy0, x1, y1, dx1, dy1, length), for attaching arrowheads.
 * [width]: the brush's width (points).
 */
class InkStroke private constructor(
    val id: String, val outline: FloatArray, val color: Int, val bounds: RectF, val kind: Int, val ends: FloatArray, val width: Float,
    val path: Path, val layerPaths: List<Path>,
) {
    constructor(id: String, outline: FloatArray, color: Int, bounds: RectF, kind: Int = 0, layers: List<FloatArray> = emptyList(), ends: FloatArray = FloatArray(0), width: Float = 0f) :
        this(id, outline, color, bounds, kind, ends, width, pathOf(outline), layers.map { pathOf(it) })

    /**
     * Moved by ([dx], [dy]): what the core has after moving it (to its rounding of the points),
     * without asking it: a selection of thousands of strokes put down at once.
     */
    fun moved(dx: Float, dy: Float): InkStroke = InkStroke(
        id, FloatArray(outline.size) { outline[it] + if (it % 2 == 0) dx else dy }, color, RectF(bounds).apply { offset(dx, dy) }, kind,
        FloatArray(ends.size) { ends[it] + when (it) { 0, 4 -> dx; 1, 5 -> dy; else -> 0f } }, width,
        Path(path).apply { offset(dx, dy) }, layerPaths.map { Path(it).apply { offset(dx, dy) } },
    )

    companion object {
        fun of(s: FfiStroke): InkStroke {
            val f = floatsOf(s.outline)
            return InkStroke(s.id, f, s.brush.color.toInt(), RectF(s.bounds[0], s.bounds[1], s.bounds[2], s.bounds[3]), s.brush.kind.toInt(), s.layers.map { floatsOf(it) }, s.ends.toFloatArray(), s.brush.width)
        }

        fun pathOf(f: FloatArray): Path {
            val p = Path()
            val n = f.size / 2
            if (n == 0) return p
            p.moveTo(f[0], f[1])
            for (i in 1 until n) p.lineTo(f[2 * i], f[2 * i + 1])
            p.close()
            return p
        }
    }
}

fun floatsOf(bytes: ByteArray): FloatArray {
    val fb = ByteBuffer.wrap(bytes).order(ByteOrder.LITTLE_ENDIAN).asFloatBuffer()
    return FloatArray(fb.remaining()).also { fb.get(it) }
}

fun bytesOf(f: FloatArray, n: Int = f.size): ByteArray {
    val bb = ByteBuffer.allocate(n * 4).order(ByteOrder.LITTLE_ENDIAN)
    bb.asFloatBuffer().put(f, 0, n)
    return bb.array()
}

/** A sheet placed in the notebook view (world coordinates = points). */
class Placed(
    val sheetId: String,
    val node: String,
    /** The main sheet's node this one belongs to (itself for a main sheet). */
    val mainNode: String,
    val spec: FfiSheetSpec,
    val rect: RectF,
    /** 0 main sheet, -1, -2 … to the left, 1, 2 … to the right. */
    val side: Int,
    /** Page number (1-based, counting every page, also folded ones); side sheets carry their main page's. */
    val number: Int,
) {
    val isBoard get() = spec.width <= 0f || spec.height <= 0f
    /** "3", or for side sheets "3 L1" / "3 R2". */
    val label: String get() = if (side == 0) "$number" else "$number ${if (side < 0) "L" else "R"}${kotlin.math.abs(side)}"

    fun at(r: RectF) = Placed(sheetId, node, mainNode, spec, r, side, number)
}

/** A group's title row between the sheets. */
class Header(val node: String, val title: String, val depth: Int, val collapsed: Boolean, val count: Int, val rect: RectF)

/** One sheet's content as the UI holds it. UI thread only, except where noted. */
/** The id of a text page's body as a text box (n0te-core `BODY_ID`). */
const val BODY_ID = "__body"

class SheetState(val id: String) {
    @Volatile var handle: SheetHandle? = null
    var strokes: MutableList<InkStroke> = ArrayList()
    var loading = false
    /** The ink recorded per page view (two panes can show this sheet, also at other sizes). */
    private val nodes = java.util.WeakHashMap<Any, RenderNode>()
    private val validFor = java.util.Collections.newSetFromMap(java.util.WeakHashMap<Any, Boolean>())
    fun nodeFor(view: Any): RenderNode = nodes.getOrPut(view) { RenderNode("sheet-$id") }
    fun nodeValid(view: Any) = view in validFor
    fun nodeRecorded(view: Any) { validFor += view }
    /** Text boxes typed on the laptop (shown, not edited here yet). */
    var texts: List<uniffi.n0te_ffi.FfiTextBox> = emptyList()
    /** Width of the sheet as laid out (a text page's body wraps in it). */
    var pageWidth = NotebookModel.A4_W
    /** Strokes the eraser took in the running gesture (removed in the core at pen-up). */
    val hidden = HashSet<String>()
    /** Strokes being moved by the lasso: drawn separately, transformed. */
    var floating: Set<String> = emptySet()
    /** Images on the sheet (drawn under the ink). */
    var images: List<uniffi.n0te_ffi.FfiImage> = emptyList()

    /** Counts the sheet's changes (the outline's page pictures are drawn anew when it moves on). */
    var version = 0; private set

    fun invalidateNode() {
        validFor.clear()
        version++
    }
}

/**
 * The open notebook: outline, layout of the sheets, their content and edits. Core calls run
 * on [N0te.io]; everything else on the UI thread.
 */
class NotebookModel(val nb: NotebookHandle, private val onChanged: () -> Unit) {

    var rows: List<FfiOutlineRow> = emptyList(); private set
    var placed: List<Placed> = emptyList(); private set
    var headers: List<Header> = emptyList(); private set
    val bounds = RectF()
    val sheets = HashMap<String, SheetState>()

    /**
     * One edit to undo: the sheets it touched (moving strokes between pages touches two) and,
     * for space inserted into a page, the page's height and gaps before and after.
     */
    private class Step(val sheets: List<String>, val reshape: Reshape? = null, val resize: Resize? = null)
    private class Reshape(val node: String, val before: Pair<Float, List<Float>>, val after: Pair<Float, List<Float>>)
    /** A page made wider by space inserted sideways: its width and height before and after. */
    private class Resize(val node: String, val before: Pair<Float, Float>, val after: Pair<Float, Float>)
    private val undoStack = ArrayDeque<Step>()
    private val redoStack = ArrayDeque<Step>()
    private val dirty = HashSet<String>()
    /** Sheets where only the search index changed: saved without counting as an edit. */
    private val quietDirty = HashSet<String>()
    private var nbDirty = false
    private var saveScheduled = false
    var title: String = nb.title(); private set

    fun canUndo() = undoStack.isNotEmpty()
    fun canRedo() = redoStack.isNotEmpty()

    // --- outline and layout ----------------------------------------------------------------

    /** Page numbers by main sheet node, over all pages (folding a group doesn't renumber). */
    private var numbers: Map<String, Int> = emptyMap()
    var totalPages = 0; private set
    fun numberOf(node: String) = numbers[node] ?: 0

    // The layout before the last change, so the view can animate (fold, unfold, move, add).
    var prevRects: Map<String, RectF> = emptyMap(); private set
    var prevHeaders: Map<String, RectF> = emptyMap(); private set
    /** Sheets gone in the last change (folded away, deleted), drawn while they fade. */
    var vanished: List<Placed> = emptyList(); private set
    var layoutAt = 0L; private set

    /** The pages marked to find again (outline nodes). */
    var marked: Set<String> = emptySet(); private set

    fun refreshOutline(then: (() -> Unit)? = null) {
        N0te.background({ Triple(nb.outline(false), nb.title(), nb.outline(true)) to runCatching { nb.marked().toSet() }.getOrDefault(emptySet()) }) { r ->
            r.getOrNull()?.let { (o, marks) ->
                val (rows, t, all) = o
                marked = marks
                this.rows = rows
                title = t
                var n = 0
                numbers = buildMap { for (row in all) if (row is FfiOutlineRow.Sheet) put(row.node, ++n) }
                totalPages = n
                layout()
                onChanged()
                then?.invoke()
                refreshHeadings()
            }
        }
    }

    /** Lays the pages out again (page gap changed in the settings). */
    fun relayout() { layout(); onChanged() }

    private fun layout() {
        val before = placed
        val beforeHeaders = headers
        val out = ArrayList<Placed>()
        val hs = ArrayList<Header>()
        var y = TOP
        var number = 0
        bounds.setEmpty()
        for (row in rows) {
            when (row) {
                is FfiOutlineRow.Group -> {
                    val left = -A4_W / 2 + row.depth.toInt() * 18f
                    val r = RectF(left, y, A4_W / 2, y + HEADER_H)
                    hs += Header(row.node, row.title, row.depth.toInt(), row.collapsed, row.count.toInt(), r)
                    bounds.union(r)
                    y += HEADER_H + 10f
                }
                is FfiOutlineRow.Sheet -> {
                    number = numbers[row.node] ?: (number + 1)
                    val (w, h) = displaySize(row.spec)
                    val main = RectF(-w / 2, y, w / 2, y + h)
                    out += Placed(row.sheetId, row.node, row.node, row.spec, main, 0, number)
                    var rowH = h
                    var x = main.left
                    row.left.forEachIndexed { i, s ->
                        val (sw, sh) = displaySize(s.spec)
                        x -= SIDE_GAP + sw
                        out += Placed(s.sheetId, s.node, row.node, s.spec, RectF(x, y, x + sw, y + sh), -(i + 1), number)
                        rowH = maxOf(rowH, sh)
                    }
                    x = main.right
                    row.right.forEachIndexed { i, s ->
                        val (sw, sh) = displaySize(s.spec)
                        out += Placed(s.sheetId, s.node, row.node, s.spec, RectF(x + SIDE_GAP, y, x + SIDE_GAP + sw, y + sh), i + 1, number)
                        x += SIDE_GAP + sw
                        rowH = maxOf(rowH, sh)
                    }
                    y += rowH + ROW_GAP
                }
            }
        }
        for (p in out) bounds.union(p.rect)
        placed = out
        headers = hs
        if (before.isNotEmpty()) {
            prevRects = before.associate { it.sheetId to it.rect }
            prevHeaders = beforeHeaders.associate { it.node to it.rect }
            val now = out.mapTo(HashSet()) { it.sheetId }
            vanished = before.filter { it.sheetId !in now }
            layoutAt = SystemClock.uptimeMillis()
        }
    }

    /** Pages at their size; boards get a landscape frame in the column. */
    private fun displaySize(spec: FfiSheetSpec): Pair<Float, Float> =
        if (spec.width <= 0f || spec.height <= 0f) BOARD_W to BOARD_H else spec.width to spec.height

    // --- sheet content ---------------------------------------------------------------------

    fun state(id: String) = sheets.getOrPut(id) { SheetState(id) }

    /** Loads a sheet's strokes once (background), then redraws. */
    /** Loads a sheet's strokes and text (boxes, a text page's body for a [pageWidth] page) once. */
    fun ensureLoaded(id: String, pageWidth: Float = A4_W) {
        val st = state(id)
        st.pageWidth = pageWidth
        if (st.handle != null || st.loading) return
        st.loading = true
        N0te.background({
            val h = nb.openSheet(id)
            Triple(h, inkStrokes(h.allStrokes()), h.textBoxesWithBody(pageWidth))
        }) { r ->
            st.loading = false
            r.getOrNull()?.let { (h, strokes, texts) ->
                st.handle = h
                st.strokes = strokes.toMutableList()
                st.texts = texts
                st.images = runCatching { h.images() }.getOrDefault(emptyList())
                st.invalidateNode()
                onChanged()
            }
        }
    }

    /**
     * Stores a finished stroke. [preview] (its outline from the live layer) shows at once; the
     * core's stored stroke replaces it when the save returns.
     */
    fun addStroke(sheetId: String, brush: FfiBrush, samples: ByteArray, preview: FloatArray, previewLayers: List<FloatArray> = emptyList()) {
        val st = state(sheetId)
        val tmpId = "tmp-${SystemClock.uptimeMillis()}-${st.strokes.size}"
        st.strokes += InkStroke(tmpId, preview, brush.color.toInt(), boundsOf(preview), brush.kind.toInt(), previewLayers, width = brush.width)
        st.invalidateNode()
        onChanged()
        withHandle(st) { h ->
            N0te.background({ InkStroke.of(h.addStroke(brush, samples)) }) { r ->
                val i = st.strokes.indexOfFirst { it.id == tmpId }
                r.getOrNull()?.let { real -> if (i >= 0) st.strokes[i] = real else st.strokes += real }
                    ?: run { if (i >= 0) st.strokes.removeAt(i) }
                st.invalidateNode()
                pushUndo(sheetId)
                markDirty(sheetId)
                onChanged()
            }
        }
    }

    /** Ends an eraser gesture: the hidden strokes are removed for good (one undo step). */
    fun commitErase(sheetId: String) {
        val st = state(sheetId)
        val ids = st.hidden.toList()
        if (ids.isEmpty()) return
        st.strokes.removeAll { it.id in st.hidden }
        st.hidden.clear()
        st.invalidateNode()
        onChanged()
        withHandle(st) { h -> N0te.background({ h.remove(ids) }) { pushUndo(sheetId); markDirty(sheetId) } }
    }

    fun deleteStrokes(sheetId: String, ids: Collection<String>) {
        val st = state(sheetId)
        st.strokes.removeAll { it.id in ids }
        st.images = st.images.filter { it.id !in ids }
        st.invalidateNode()
        onChanged()
        withHandle(st) { h -> N0te.background({ h.remove(ids.toList()) }) { pushUndo(sheetId); markDirty(sheetId) } }
    }

    fun recolor(sheetId: String, ids: Collection<String>, color: Int) {
        val st = state(sheetId)
        withHandle(st) { h -> N0te.background({ h.recolor(ids.toList(), color.toUInt()); reloadSome(h, ids) }) { r -> applySome(st, r); pushUndo(sheetId); markDirty(sheetId) } }
    }

    fun transform(sheetId: String, ids: Collection<String>, dx: Float, dy: Float, scale: Float, ox: Float, oy: Float, done: () -> Unit) {
        val st = state(sheetId)
        // Only moved: the strokes here are moved along (the core's outlines would be the same).
        val moved = scale == 1f
        withHandle(st) { h ->
            N0te.background({ h.transform(ids.toList(), dx, dy, scale, ox, oy); if (moved) null else reloadSome(h, ids) }) { r ->
                val some = when {
                    r.isFailure -> Changed(emptySet(), emptyList())
                    moved -> ids.toHashSet().let { set -> Changed(set, st.strokes.filter { it.id in set }.map { it.moved(dx, dy) }) }
                    else -> r.getOrThrow()!!
                }
                applySome(st, Result.success(some))
                pushUndo(sheetId)
                markDirty(sheetId)
                done()
            }
        }
    }

    /** Stretches strokes and images about (ax, ay): kx across, ky down (a selection's side dragged; width kept). */
    fun stretch(sheetId: String, ids: Collection<String>, ax: Float, ay: Float, kx: Float, ky: Float, done: () -> Unit) {
        val st = state(sheetId)
        withHandle(st) { h ->
            N0te.background({ h.stretch(ids.toList(), ax, ay, kx, ky); reloadSome(h, ids) }) { r ->
                applySome(st, r)
                pushUndo(sheetId)
                markDirty(sheetId)
                done()
            }
        }
    }

    /** Turns and stretches strokes about (ax, ay) by re + i·im (a line's end dragged; width kept). */
    fun spin(sheetId: String, ids: Collection<String>, ax: Float, ay: Float, re: Float, im: Float, done: () -> Unit) {
        val st = state(sheetId)
        withHandle(st) { h ->
            N0te.background({ h.spin(ids.toList(), ax, ay, re, im); reloadSome(h, ids) }) { r ->
                applySome(st, r)
                pushUndo(sheetId)
                markDirty(sheetId)
                done()
            }
        }
    }

    /** A stroke's first and last sample (page points): x0, y0, x1, y1. */
    fun strokeEnds(sheetId: String, id: String, then: (FloatArray?) -> Unit) {
        withHandle(state(sheetId)) { h ->
            N0te.background({ h.strokeData(listOf(id)).firstOrNull()?.samples }) { r ->
                val b = r.getOrNull()
                if (b == null || b.size < 16) { then(null); return@background }
                val buf = java.nio.ByteBuffer.wrap(b).order(java.nio.ByteOrder.LITTLE_ENDIAN)
                val n = b.size / 16
                then(floatArrayOf(buf.getFloat(0), buf.getFloat(4), buf.getFloat((n - 1) * 16), buf.getFloat((n - 1) * 16 + 4)))
            }
        }
    }

    fun undo() = history(undoStack, redoStack, back = true) { it.undo() }
    fun redo() = history(redoStack, undoStack, back = false) { it.redo() }

    private fun history(from: ArrayDeque<Step>, to: ArrayDeque<Step>, back: Boolean, op: (SheetHandle) -> Boolean) {
        commitPendingTexts()
        val step = from.removeLastOrNull() ?: return
        val sts = step.sheets.map { state(it) }
        if (sts.any { it.handle == null }) return
        val shape = step.reshape?.let { if (back) it.node to it.before else it.node to it.after }
        val size = step.resize?.let { if (back) it.node to it.before else it.node to it.after }
        // Text edits are undo steps too: reload the text boxes with the strokes.
        N0te.background({
            val lists = sts.map { st -> val h = st.handle!!; if (op(h)) reloadStrokes(h) to h.textBoxesWithBody(st.pageWidth) else null }
            // Space inserted: the page's height (and a PDF page's split) goes back too, or its width.
            val reshaped = shape?.let { (node, hg) -> runCatching { nb.reshape(node, hg.first, hg.second); nb.save() }.isSuccess } ?: false
            val resized = size?.let { (node, wh) -> runCatching { nb.setSize(node, wh.first, wh.second); nb.save() }.isSuccess } ?: false
            lists to (reshaped || resized)
        }) { r ->
            r.getOrNull()?.let { (lists, reshaped) ->
                var any = reshaped
                sts.zip(lists).forEach { (st, pair) -> if (pair != null) { st.texts = pair.second; applyReload(st, Result.success(pair.first)); markDirty(st.id); any = true } }
                if (any) to.addLast(step)
                // A heading marked or unmarked is undone too: the outline shows it.
                if (reshaped) refreshOutline() else if (any) refreshHeadings()
            }
            onChanged()
        }
    }

    /**
     * Space inserted (dy > 0) or taken out at [y] on [p] as [plan] says (see the core's
     * plan_space): what lies below moves, the page takes the plan's height and gaps. One undo
     * step for both. [done] gets whether anything changed.
     */
    fun insertSpace(p: Placed, y: Float, plan: uniffi.n0te_ffi.FfiSpacePlan, done: (Boolean) -> Unit) {
        val st = state(p.sheetId)
        val h = st.handle ?: return done(false)
        val before = p.spec.height to p.spec.gaps
        val after = plan.height to plan.gaps
        val reshape = !p.isBoard && before != after
        if (plan.dy == 0f && !reshape) return done(false)
        N0te.background({
            val moved = if (plan.dy != 0f) h.shiftBelow(y, plan.dy).toInt() else 0
            if (reshape) { nb.reshape(p.node, after.first, after.second); nb.save() }
            val reload = if (moved > 0) reloadStrokes(h) to h.textBoxesWithBody(st.pageWidth) else null
            moved to reload
        }) { r ->
            val (moved, reload) = r.getOrNull() ?: return@background done(false)
            reload?.let { (strokes, texts) -> st.texts = texts; applyReload(st, Result.success(strokes)); markDirty(st.id) }
            // Only a sheet that really changed takes part in the undo step (else undo would take back an older edit).
            undoStack.addLast(Step(if (moved > 0) listOf(p.sheetId) else emptyList(), if (reshape) Reshape(p.node, before, after) else null))
            while (undoStack.size > 500) undoStack.removeFirst()
            redoStack.clear()
            if (reshape) refreshOutline { done(true) } else { onChanged(); done(true) }
        }
    }

    /**
     * Space inserted sideways (dx > 0) or taken out at [x] on [p] as [plan] says (see the core's
     * plan_space_x): what lies right of the line moves, the page takes the plan's width. One undo
     * step for both. [done] gets whether anything changed.
     */
    fun insertSpaceX(p: Placed, x: Float, plan: uniffi.n0te_ffi.FfiSpacePlanX, done: (Boolean) -> Unit) {
        val st = state(p.sheetId)
        val h = st.handle ?: return done(false)
        val before = p.spec.width to p.spec.height
        val after = plan.width to p.spec.height
        val resize = !p.isBoard && before != after
        if (plan.dx == 0f && !resize) return done(false)
        N0te.background({
            val moved = if (plan.dx != 0f) h.shiftRightOf(x, plan.dx).toInt() else 0
            if (resize) { nb.setSize(p.node, after.first, after.second); nb.save() }
            val reload = if (moved > 0) reloadStrokes(h) to h.textBoxesWithBody(st.pageWidth) else null
            moved to reload
        }) { r ->
            val (moved, reload) = r.getOrNull() ?: return@background done(false)
            reload?.let { (strokes, texts) -> st.texts = texts; applyReload(st, Result.success(strokes)); markDirty(st.id) }
            undoStack.addLast(Step(if (moved > 0) listOf(p.sheetId) else emptyList(), resize = if (resize) Resize(p.node, before, after) else null))
            while (undoStack.size > 500) undoStack.removeFirst()
            redoStack.clear()
            if (resize) refreshOutline { done(true) } else { onChanged(); done(true) }
        }
    }

    // --- text boxes and text pages --------------------------------------------------------------

    /** Typed text not yet written to the sheet (sheet id to box id → text), written 0.5 s after the last key. */
    private val pendingText = LinkedHashMap<Pair<String, String>, String>()
    private val commitTexts = Runnable { commitPendingTexts() }

    /** A new, empty text box at (x, y) on the sheet; [then] gets its id. */
    fun addTextBox(sheetId: String, x: Float, y: Float, w: Float, size: Float, then: (String) -> Unit) {
        val st = state(sheetId)
        withHandle(st) { h ->
            N0te.background({ h.addTextBox(x, y, w, size, "") }) { r ->
                r.getOrNull()?.let { id ->
                    st.texts = st.texts + uniffi.n0te_ffi.FfiTextBox(id, x, y, w, size, "")
                    st.invalidateNode()
                    onChanged()
                    pushUndo(sheetId)
                    markDirty(sheetId)
                    then(id)
                }
            }
        }
    }

    /** The text of a box ([BODY_ID]: the text page's body) as typed: shown at once, saved shortly after. */
    fun setText(sheetId: String, id: String, text: String) {
        val st = state(sheetId)
        st.texts = if (id == BODY_ID && st.texts.none { it.id == BODY_ID }) {
            listOf(uniffi.n0te_ffi.FfiTextBox(BODY_ID, 56f, 64f, (st.pageWidth - 112f).coerceAtLeast(100f), 12f, text)) + st.texts
        } else {
            st.texts.map { if (it.id == id) it.copy(text = text) else it }
        }
        // Recorded before anyone hears about the change: listeners compare against it (the
        // other way round, the editor got the previous keystroke back and lost characters).
        pendingText[sheetId to id] = text
        st.invalidateNode()
        onChanged()
        N0te.main.removeCallbacks(commitTexts)
        N0te.main.postDelayed(commitTexts, 500)
    }

    /** Text being typed into this box that isn't written yet (the editor owns it now). */
    fun isTyping(sheetId: String, id: String): Boolean = (sheetId to id) in pendingText

    /** Writes typed text into the sheets now (each write is one undo step and syncs). */
    fun commitPendingTexts() {
        N0te.main.removeCallbacks(commitTexts)
        if (pendingText.isEmpty()) return
        val items = pendingText.toList()
        pendingText.clear()
        for ((key, text) in items) {
            val (sheetId, id) = key
            val st = state(sheetId)
            // Dirty right away: a flush() right after saves it behind this write (one io thread).
            dirty += sheetId
            withHandle(st) { h ->
                N0te.background({ if (id == BODY_ID) h.setPageText(text) else h.setText(id, text) }) { r ->
                    if (r.isSuccess) { pushUndo(sheetId); markDirty(sheetId) }
                }
            }
        }
    }

    /** The current text of a box as shown (including what is still being typed). */
    fun textOf(sheetId: String, id: String): String? = pendingText[sheetId to id] ?: sheets[sheetId]?.texts?.firstOrNull { it.id == id }?.text

    /** Moves / resizes a text box (already moved on screen by the view). */
    fun placeTextBox(sheetId: String, id: String, x: Float, y: Float, w: Float, size: Float) {
        if (id == BODY_ID) return
        val st = state(sheetId)
        st.texts = st.texts.map { if (it.id == id) it.copy(x = x, y = y, w = w, size = size) else it }
        st.invalidateNode()
        onChanged()
        withHandle(st) { h -> N0te.background({ h.placeTextBox(id, x, y, w, size) }) { if (it.isSuccess) { pushUndo(sheetId); markDirty(sheetId) } } }
    }

    /** Deletes a text box (for [BODY_ID]: empties the text page). */
    fun deleteTextBox(sheetId: String, id: String) {
        pendingText.remove(sheetId to id)
        val st = state(sheetId)
        st.texts = st.texts.filter { it.id != id }
        st.invalidateNode()
        onChanged()
        withHandle(st) { h ->
            N0te.background({ if (id == BODY_ID) h.setPageText("") else h.remove(listOf(id)) }) { if (it.isSuccess) { pushUndo(sheetId); markDirty(sheetId) } }
        }
    }

    // --- copy, paste, moving between sheets, area eraser ----------------------------------------

    /** The strokes' data for the clipboard (brush and samples). */
    fun copyStrokes(sheetId: String, ids: Collection<String>, then: (List<uniffi.n0te_ffi.FfiStrokeData>) -> Unit) {
        val st = state(sheetId)
        withHandle(st) { h -> N0te.background({ h.strokeData(ids.toList()) }) { r -> r.getOrNull()?.let(then) } }
    }

    /** Adds clipboard strokes to a sheet (moved by dx, dy; one undo step); [then] gets their ids. */
    /** Places a stored blob as an image (one undo step); [then] gets its id. */
    fun addImage(sheetId: String, blob: String, x: Float, y: Float, w: Float, h: Float, then: (String) -> Unit = {}) {
        val st = state(sheetId)
        withHandle(st) { hd ->
            N0te.background({ hd.addImage(blob, x, y, w, h).also { st.images = hd.images() } }) { r ->
                r.getOrNull()?.let { id ->
                    st.invalidateNode()
                    pushUndo(sheetId)
                    markDirty(sheetId)
                    onChanged()
                    then(id)
                }
            }
        }
    }

    /** Puts a coordinate system on a sheet (one undo step); [then] gets its id. */
    fun addPlot(sheetId: String, plot: uniffi.n0te_ffi.FfiPlot, then: (String) -> Unit = {}) {
        val st = state(sheetId)
        withHandle(st) { hd ->
            N0te.background({ hd.addPlot(plot).also { st.images = hd.images() } }) { r ->
                r.getOrNull()?.let { id ->
                    pushUndo(sheetId)
                    markDirty(sheetId)
                    onChanged()
                    then(id)
                }
            }
        }
    }

    /** Changes a coordinate system (its functions, ranges, look; one undo step); [then] once the page has it. */
    fun setPlot(sheetId: String, plot: uniffi.n0te_ffi.FfiPlot, then: () -> Unit = {}) {
        val st = state(sheetId)
        withHandle(st) { hd ->
            N0te.background({ hd.setPlot(plot); st.images = hd.images() }) { r ->
                if (r.isSuccess) { pushUndo(sheetId); markDirty(sheetId); onChanged() }
                then()
            }
        }
    }

    fun pasteStrokes(sheetId: String, data: List<uniffi.n0te_ffi.FfiStrokeData>, dx: Float, dy: Float, then: (List<InkStroke>) -> Unit = {}) {
        val st = state(sheetId)
        withHandle(st) { h ->
            N0te.background({ inkStrokes(h.addStrokes(data, dx, dy, 1f, 0f, 0f)) }) { r ->
                r.getOrNull()?.let { added ->
                    st.strokes.addAll(added)
                    st.invalidateNode()
                    pushUndo(sheetId)
                    markDirty(sheetId)
                    onChanged()
                    then(added)
                }
            }
        }
    }

    /**
     * Moves strokes to another sheet: scaled by [k] about (ox, oy) and moved by (dx, dy) in the
     * source's coordinates, then shifted by [shiftX]/[shiftY] into the target's. One undo step.
     */
    fun moveStrokes(from: String, to: String, ids: Collection<String>, dx: Float, dy: Float, k: Float, ox: Float, oy: Float, shiftX: Float, shiftY: Float, then: (List<InkStroke>) -> Unit) {
        val src = state(from); val dst = state(to)
        withHandle(src) { hs ->
            withHandle(dst) { hd ->
                N0te.background({
                    val data = hs.strokeData(ids.toList())
                    val added = inkStrokes(hd.addStrokes(data, dx + shiftX, dy + shiftY, k, ox, oy))
                    // Images go along (scaled about the same point, then shifted).
                    for (im in hs.images().filter { it.id in ids }) {
                        val nx = ox + (im.x - ox) * k + dx + shiftX; val ny = oy + (im.y - oy) * k + dy + shiftY
                        val plot = im.plot
                        if (plot != null) hd.addPlot(plot.copy(x = nx, y = ny, w = im.w * k, h = im.h * k))
                        else hd.addImage(im.blob, nx, ny, im.w * k, im.h * k)
                    }
                    hs.remove(ids.toList())
                    dst.images = hd.images()
                    added
                }) { r ->
                    r.getOrNull()?.let { added ->
                        // Gone from the source: the rest of it stays as it is.
                        applySome(src, Result.success(Changed(ids.toHashSet(), emptyList())))
                        dst.strokes.addAll(added)
                        dst.invalidateNode()
                        pushUndo(listOf(to, from))
                        markDirty(from); markDirty(to)
                        onChanged()
                        then(added)
                    }
                }
            }
        }
    }

    /** Area eraser: path as x, y, radius triples (sheet coordinates). One undo step. */
    fun eraseArea(sheetId: String, path: FloatArray, then: () -> Unit) {
        val st = state(sheetId)
        withHandle(st) { h ->
            N0te.background({ if (h.eraseArea(bytesOf(path)) > 0u) reloadStrokes(h) else null }) { r ->
                r.getOrNull()?.let { list -> applyReload(st, Result.success(list)); pushUndo(sheetId); markDirty(sheetId) }
                then()
            }
        }
    }

    /** Strokes the path (x, y pairs) crosses within [radius] (scribble-to-delete). */
    fun strokesCrossed(sheetId: String, path: FloatArray, radius: Float, then: (List<String>) -> Unit) {
        val st = state(sheetId)
        withHandle(st) { h -> N0te.background({ h.hitPath(bytesOf(path), radius) }) { r -> then(r.getOrNull().orEmpty()) } }
    }

    /** The core's strokes as drawn; many at once (a page, a paste) on several threads. */
    private fun inkStrokes(src: List<FfiStroke>): List<InkStroke> =
        if (src.size < 256) src.map { InkStroke.of(it) }
        else src.parallelStream().map { InkStroke.of(it) }.collect(java.util.stream.Collectors.toList())

    private fun reloadStrokes(h: SheetHandle): List<InkStroke> = inkStrokes(h.allStrokes())

    /** Strokes [ids] as the core has them now (moved, turned, recoloured), with which ones were asked for. */
    private class Changed(val ids: Set<String>, val strokes: List<InkStroke>)

    /** Only [ids] again from the core (I/O thread): the page's other strokes stay as they are. */
    private fun reloadSome(h: SheetHandle, ids: Collection<String>) = Changed(ids.toHashSet(), inkStrokes(h.strokesById(ids.toList())))

    /** [r]'s strokes in the places of the ones asked for (one the core no longer has goes). */
    private fun applySome(st: SheetState, r: Result<Changed>) {
        val c = r.getOrNull() ?: return
        val byId = c.strokes.associateBy { it.id }
        val out = ArrayList<InkStroke>(st.strokes.size)
        for (s in st.strokes) if (s.id !in c.ids) out += s else byId[s.id]?.let { out += it }
        st.strokes = out
        st.handle?.let { h -> st.images = runCatching { h.images() }.getOrDefault(st.images) }
        st.hidden.clear()
        st.floating = emptySet()
        st.invalidateNode()
        onChanged()
    }

    private fun applyReload(st: SheetState, r: Result<List<InkStroke>>) {
        r.getOrNull()?.let {
            st.strokes = it.toMutableList()
            st.handle?.let { h -> st.images = runCatching { h.images() }.getOrDefault(st.images) }
            st.hidden.clear()
            st.floating = emptySet()
            st.invalidateNode()
            onChanged()
        }
    }

    private fun pushUndo(sheetId: String) = pushUndo(listOf(sheetId))

    private fun pushUndo(sheets: List<String>) {
        undoStack.addLast(Step(sheets))
        while (undoStack.size > 500) undoStack.removeFirst()
        redoStack.clear()
    }

    private fun withHandle(st: SheetState, f: (SheetHandle) -> Unit) {
        st.handle?.let(f) ?: run {
            // Not loaded yet (drawn right after opening): load, then do it.
            ensureLoaded(st.id)
            N0te.io.execute { N0te.main.post { st.handle?.let(f) } }
        }
    }

    // --- structure ---------------------------------------------------------------------------

    /** Inserts a new main sheet after the main sheet [afterNode] (or at the end). */
    fun addSheetAfter(afterNode: String?, spec: FfiSheetSpec, then: (String) -> Unit = {}) {
        N0te.background({
            val (parent, index) = positionAfter(afterNode)
            nb.addSheet(parent, index, spec).also { nb.save() }
        }) { r -> refreshOutline { r.getOrNull()?.let { then(it[1]) } } }
    }

    fun addSideSheet(mainNode: String, right: Boolean, spec: FfiSheetSpec, then: (String) -> Unit = {}) {
        N0te.background({ nb.addSideSheet(mainNode, right, UInt.MAX_VALUE, spec).also { nb.save() } }) { r ->
            refreshOutline { r.getOrNull()?.let { then(it[1]) } }
        }
    }

    /** A new group after [afterNode]; [then] gets its node (to name it right away). */
    fun addGroupAfter(afterNode: String?, title: String, then: (String) -> Unit = {}) {
        N0te.background({
            val (parent, index) = positionAfter(afterNode)
            nb.addGroup(parent, index, title).also { nb.save() }
        }) { r -> refreshOutline { r.getOrNull()?.let(then) } }
    }

    /** A new group or page at the end of [group] (unfolds it). */
    fun addGroupIn(group: String, title: String, then: (String) -> Unit = {}) {
        N0te.background({ nb.setCollapsed(group, false); nb.addGroup(group, UInt.MAX_VALUE, title).also { nb.save() } }) { r ->
            refreshOutline { r.getOrNull()?.let(then) }
        }
    }

    fun addSheetIn(group: String, spec: FfiSheetSpec, then: (String) -> Unit = {}) {
        N0te.background({ nb.setCollapsed(group, false); nb.addSheet(group, UInt.MAX_VALUE, spec).also { nb.save() } }) { r ->
            refreshOutline { r.getOrNull()?.let { then(it[1]) } }
        }
    }

    /** Marks a page to find again (or not). */
    fun setMarked(node: String, on: Boolean) {
        marked = if (on) marked + node else marked - node
        onChanged()
        N0te.background({ nb.setMarked(node, on); nb.save() }) { refreshOutline() }
    }

    /** Folds or unfolds several groups at once (one save). */
    fun setCollapsedAll(groups: List<String>, collapsed: Boolean) {
        if (groups.isEmpty()) return
        N0te.background({ groups.forEach { nb.setCollapsed(it, collapsed) }; nb.save() }) { refreshOutline() }
    }

    fun setCollapsed(group: String, collapsed: Boolean) {
        N0te.background({ nb.setCollapsed(group, collapsed); nb.save() }) { refreshOutline() }
    }

    fun rename(node: String, title: String) {
        N0te.background({ nb.setNodeTitle(node, title); nb.save() }) { refreshOutline() }
    }

    fun deleteNode(node: String, then: () -> Unit = {}) {
        N0te.background({ nb.deleteNode(node); nb.save() }) { refreshOutline(then) }
    }

    /** Takes back the last [deleteNode] (the page returns with its side pages and content). */
    fun undoDelete(done: (Boolean) -> Unit = {}) {
        N0te.background({ nb.undoDelete().also { if (it) nb.save() } }) { r -> refreshOutline { done(r.getOrDefault(false)) } }
    }

    fun moveNode(node: String, parent: String?, index: Int) {
        N0te.background({ nb.moveNode(node, parent, index.toUInt()); nb.save() }) { refreshOutline() }
    }

    /** Imports a PDF (one sheet per page) right after the main sheet [afterNode]. */
    fun importPdfAfter(bytes: ByteArray, afterNode: String?, then: (Result<Int>) -> Unit) {
        val svc = N0te.pdf ?: return then(Result.failure(IllegalStateException("PDF nicht verfügbar")))
        N0te.background({
            val (parent, index) = positionAfter(afterNode)
            nb.importPdf(svc, bytes, parent, index).size.also { nb.save() }
        }) { r -> refreshOutline { then(r) } }
    }

    /** Changes the paper of sheets ([nodes], main or side sheets), then relayouts. */
    fun setPaper(nodes: List<String>, bg: FfiBackground, spacing: Float) {
        N0te.background({
            for (n in nodes) runCatching { nb.setPaper(n, bg, spacing) }
            nb.save()
        }) { refreshOutline() }
    }

    /** What would stick out of [nodes] at [width] × [height] (open pages count unsaved strokes). */
    fun outside(nodes: List<String>, width: Float, height: Float, then: (uniffi.n0te_ffi.FfiOutside?) -> Unit) {
        N0te.background({ nb.outside(nodes, width, height) }) { r -> then(r.getOrNull()) }
    }

    /** Changes the size of sheets ([nodes]; PDF pages and boards refuse), then relayouts. */
    fun setSize(nodes: List<String>, width: Float, height: Float, then: (Int) -> Unit = {}) {
        N0te.background({
            val n = nodes.count { runCatching { nb.setSize(it, width, height) }.isSuccess }
            nb.save()
            n
        }) { r -> refreshOutline { then(r.getOrDefault(0)) } }
    }

    /** Every plain page of the notebook (main and side sheets; no PDF pages, no boards). */
    fun allPlainSheetNodes(then: (List<String>) -> Unit) {
        N0te.background({
            nb.outline(true).filterIsInstance<FfiOutlineRow.Sheet>().flatMap { r ->
                (r.left.map { it.node to it.spec } + (r.node to r.spec) + r.right.map { it.node to it.spec })
            }.filter { (_, spec) -> spec.pdfBlob == null && spec.width > 0f }.map { it.first }
        }) { r -> r.getOrNull()?.let(then) }
    }

    /** The whole outline, also inside folded groups. */
    fun allRows(then: (List<FfiOutlineRow>) -> Unit) {
        N0te.background({ nb.outline(true) }) { r -> r.getOrNull()?.let(then) }
    }

    /** Exports [sheetIds] (null: all) to [file]; [sidesAtEnd]: side pages after all main pages, else right after theirs. */
    fun exportPdf(sheetIds: List<String>?, file: java.io.File, sidesAtEnd: Boolean, then: (Result<Int>) -> Unit) {
        val svc = N0te.pdf ?: return then(Result.failure(IllegalStateException("PDF nicht verfügbar")))
        flush()
        N0te.background({ nb.exportPdfToFile(svc, sheetIds, sidesAtEnd, file.absolutePath).toInt() }, then)
    }

    /** Where a node sits: its parent group (null = main column), index and the siblings (I/O thread). */
    private class Position(val parent: String?, val index: Int, val siblings: List<Pair<String, Boolean>>)

    private fun positionOf(node: String): Position? {
        val all = nb.outline(true)
        val stack = ArrayList<String>()
        val children = HashMap<String?, MutableList<Pair<String, Boolean>>>()
        var found: Pair<String?, Int>? = null
        for (row in all) {
            val depth = when (row) { is FfiOutlineRow.Group -> row.depth.toInt(); is FfiOutlineRow.Sheet -> row.depth.toInt() }
            while (stack.size > depth) stack.removeAt(stack.size - 1)
            val parent = stack.lastOrNull()
            val id = when (row) { is FfiOutlineRow.Group -> row.node; is FfiOutlineRow.Sheet -> row.node }
            val list = children.getOrPut(parent) { ArrayList() }
            if (id == node) found = parent to list.size
            list += id to (row is FfiOutlineRow.Group)
            if (row is FfiOutlineRow.Group) stack += row.node
        }
        val (parent, index) = found ?: return null
        return Position(parent, index, children[parent].orEmpty())
    }

    /**
     * Moves a page or group: "up"/"down" among its siblings, "out" of its group (after it), or
     * "into" the group right above it (at its end).
     */
    fun move(node: String, how: String) {
        N0te.background({
            val pos = positionOf(node) ?: return@background
            when (how) {
                "up" -> if (pos.index > 0) nb.moveNode(node, pos.parent, (pos.index - 1).toUInt())
                "down" -> if (pos.index < pos.siblings.size - 1) nb.moveNode(node, pos.parent, (pos.index + 1).toUInt())
                "out" -> pos.parent?.let { g -> positionOf(g)?.let { gp -> nb.moveNode(node, gp.parent, (gp.index + 1).toUInt()) } }
                "into" -> pos.siblings.getOrNull(pos.index - 1)?.takeIf { it.second }?.let { (g, _) -> nb.moveNode(node, g, UInt.MAX_VALUE) }
            }
            nb.save()
        }) { refreshOutline() }
    }

    /** Parent and index for inserting right after [afterNode] (I/O thread). */
    private fun positionAfter(afterNode: String?): Pair<String?, UInt> {
        if (afterNode == null) return null to UInt.MAX_VALUE
        // Walk the full tree to find the node's parent group and position.
        val all = nb.outline(true)
        val stack = ArrayList<Pair<String?, Int>>() // (group node, depth)
        val counters = HashMap<String?, Int>()
        for (row in all) {
            val depth = when (row) { is FfiOutlineRow.Group -> row.depth.toInt(); is FfiOutlineRow.Sheet -> row.depth.toInt() }
            while (stack.size > depth) stack.removeAt(stack.size - 1)
            val parent = stack.lastOrNull()?.first
            val idx = counters.getOrDefault(parent, 0)
            counters[parent] = idx + 1
            val node = when (row) { is FfiOutlineRow.Group -> row.node; is FfiOutlineRow.Sheet -> row.node }
            if (node == afterNode) return parent to (idx + 1).toUInt()
            if (row is FfiOutlineRow.Group) stack += row.node to depth
        }
        return null to UInt.MAX_VALUE
    }

    // --- saving -------------------------------------------------------------------------------

    private fun markDirty(sheetId: String) {
        dirty += sheetId
        scheduleSave()
        indexSoon(sheetId)
    }

    // --- handwriting: read for search, headings ------------------------------------------------

    /** Every page's headings (page order, top to bottom), for the outline. */
    var headings: List<uniffi.n0te_ffi.FfiPageHeading> = emptyList(); private set
    private val indexTimers = HashMap<String, Runnable>()

    /** Reads [sheetId]'s handwriting again once the pen has rested on it for a while. */
    fun indexSoon(sheetId: String) {
        if (!Settings.handwritingSearch || !Handwriting.ready) return
        indexTimers.remove(sheetId)?.let { N0te.main.removeCallbacks(it) }
        val r = Runnable {
            indexTimers.remove(sheetId)
            val h = sheets[sheetId]?.handle ?: return@Runnable
            Handwriting.worker.execute {
                val changed = runCatching { Handwriting.indexSheet(h) }.getOrDefault(false)
                if (changed) N0te.main.post { quietDirty += sheetId; scheduleSave() }
            }
        }
        indexTimers[sheetId] = r
        N0te.main.postDelayed(r, 3000)
    }

    /**
     * Reads every page of the notebook that isn't read yet, one after the other in the
     * background (pages not open here through a handle of their own, saved right after).
     */
    fun indexAll() {
        if (!Settings.handwritingSearch || !Handwriting.ready) return
        Handwriting.worker.execute {
            val ids = runCatching { nb.outline(true).filterIsInstance<FfiOutlineRow.Sheet>().flatMap { r -> r.left.map { it.sheetId } + r.sheetId + r.right.map { it.sheetId } } }.getOrDefault(emptyList())
            for (id in ids) {
                if (!Settings.handwritingSearch) break
                val open = sheets[id]?.handle
                if (open != null) {
                    if (runCatching { Handwriting.indexSheet(open) }.getOrDefault(false)) N0te.main.post { quietDirty += id; scheduleSave() }
                } else {
                    runCatching { val h = nb.openSheet(id); if (Handwriting.indexSheet(h)) { h.saveQuiet(); SyncHub.notifyChanged(nb.id()) } }
                }
            }
        }
    }

    /** Where [query]'s words are in this notebook (handwriting, typed text, headings, PDFs). */
    fun search(query: String, then: (List<uniffi.n0te_ffi.FfiSearchHit>) -> Unit) {
        commitPendingTexts()
        N0te.background({ nb.search(query, N0te.pdf) }) { r -> then(r.getOrDefault(emptyList())) }
    }

    /** Re-reads every page's headings (for the outline). */
    fun refreshHeadings() {
        N0te.background({ nb.headings() }) { r -> r.getOrNull()?.let { headings = it; onChanged() } }
    }

    /** What the handwriting recogniser read in [ids] on [sheetId] (to suggest a heading's words). */
    fun readText(sheetId: String, ids: Collection<String>, then: (String) -> Unit) {
        val h = sheets[sheetId]?.handle ?: return then("")
        val set = ids.toSet()
        N0te.background({ h.indexLines().filter { l -> l.strokes.any { it in set } }.joinToString(" ") { it.text } }) { r -> then(r.getOrDefault("")) }
    }

    /** Marks [ids] on [sheetId] as a heading reading [text] (one undo step). */
    fun addHeading(sheetId: String, ids: Collection<String>, text: String) {
        val st = state(sheetId)
        withHandle(st) { h -> N0te.background({ h.addHeading(text, ids.toList()) }) { pushUndo(sheetId); dirty += sheetId; scheduleSave(); refreshHeadings() } }
    }

    /** The heading [ids] on [sheetId] already belong to, if any (to rename or unmark it). */
    fun headingOf(sheetId: String, ids: Collection<String>, then: (uniffi.n0te_ffi.FfiHeading?) -> Unit) {
        val h = sheets[sheetId]?.handle ?: return then(null)
        val set = ids.toSet()
        N0te.background({ h.headings().firstOrNull { hd -> hd.strokes.any { it in set } } }) { r -> then(r.getOrNull()) }
    }

    fun setHeadingText(sheetId: String, id: String, text: String) {
        val st = state(sheetId)
        withHandle(st) { h -> N0te.background({ h.setHeadingText(id, text) }) { pushUndo(sheetId); dirty += sheetId; scheduleSave(); refreshHeadings() } }
    }

    fun removeHeading(sheetId: String, id: String) {
        val st = state(sheetId)
        withHandle(st) { h -> N0te.background({ h.removeHeading(id) }) { pushUndo(sheetId); dirty += sheetId; scheduleSave(); refreshHeadings() } }
    }

    private fun scheduleSave() {
        if (saveScheduled) return
        saveScheduled = true
        // Connected (live): soon, so the laptop sees it quickly; otherwise batch more.
        N0te.main.postDelayed({ saveScheduled = false; flush() }, if (SyncHub.online) 350L else 1200L)
    }

    /** Writes every changed sheet now (also on pause). */
    fun flush() {
        commitPendingTexts()
        val ids = dirty.toList()
        val quiet = (quietDirty - dirty).toList()
        dirty.clear()
        quietDirty.clear()
        val handles = ids.mapNotNull { sheets[it]?.handle }
        val quietHandles = quiet.mapNotNull { sheets[it]?.handle }
        val saveNb = nbDirty
        nbDirty = false
        if (handles.isEmpty() && quietHandles.isEmpty() && !saveNb) return
        N0te.io.execute {
            handles.forEach { runCatching { it.save() } }
            quietHandles.forEach { runCatching { it.saveQuiet() } }
            if (saveNb) runCatching { nb.save() }
            SyncHub.notifyChanged(nb.id())
        }
    }

    /**
     * Another device changed this notebook: merge what the sync engine saved into the open
     * documents, re-read the outline (if [index]) and the [changed] sheets (all if empty).
     * [then] gets the reloaded sheet ids.
     */
    fun refreshFromDisk(index: Boolean, changed: List<String>, then: (List<String>) -> Unit = {}) {
        val targets = (if (changed.isEmpty()) sheets.keys.toList() else changed).mapNotNull { id -> sheets[id]?.takeIf { it.handle != null } }
        N0te.background({
            val outline = runCatching { nb.refresh() }.getOrDefault(false)
            val reloaded = targets.mapNotNull { st ->
                val h = st.handle ?: return@mapNotNull null
                if (runCatching { h.refresh() }.getOrDefault(false)) Triple(st, reloadStrokes(h), h.textBoxesWithBody(st.pageWidth)) else null
            }
            outline to reloaded
        }) { r ->
            val (outline, reloaded) = r.getOrNull() ?: return@background
            for ((st, strokes, texts) in reloaded) {
                st.strokes = strokes.toMutableList()
                // What is still being typed here stays on screen (it is written in a moment).
                st.texts = texts.map { b -> pendingText[st.id to b.id]?.let { b.copy(text = it) } ?: b }
                st.hidden.clear()
                st.invalidateNode()
            }
            if (index || outline) refreshOutline() else { onChanged(); if (reloaded.isNotEmpty()) refreshHeadings() }
            // Written elsewhere (the laptop): read here too.
            reloaded.forEach { indexSoon(it.first.id) }
            then(reloaded.map { it.first.id })
        }
    }

    companion object {
        const val A4_W = 595f
        const val TOP = 40f
        const val HEADER_H = 34f
        /** Gap between page rows and between side sheets (settings). */
        val ROW_GAP: Float get() = Settings.pageGapV
        val SIDE_GAP: Float get() = Settings.pageGapH
        const val BOARD_W = 842f
        const val BOARD_H = 595f

        fun boundsOf(f: FloatArray): RectF {
            val r = RectF(Float.MAX_VALUE, Float.MAX_VALUE, -Float.MAX_VALUE, -Float.MAX_VALUE)
            var i = 0
            while (i + 1 < f.size) { r.union(f[i], f[i + 1]); i += 2 }
            return r
        }

        fun defaultSpec(bg: FfiBackground = Settings.defaultPaper) = FfiSheetSpec(595f, 842f, bg, Settings.spacingFor(bg), null, 0u)
        fun boardSpec() = FfiSheetSpec(0f, 0f, FfiBackground.DOTS, Settings.dotSpacing, null, 0u)

        val inkPaint = Paint(Paint.ANTI_ALIAS_FLAG).apply { style = Paint.Style.FILL }
    }
}
