package dev.n0bridge.n0te

import android.graphics.Canvas
import android.graphics.Matrix
import android.graphics.RectF
import android.graphics.RenderNode
import kotlin.math.ceil
import kotlin.math.floor
import kotlin.math.ln
import kotlin.math.roundToInt

/**
 * The pure parts of drawing a page's ink in pieces (see [InkChunks]).
 *
 * Chunks: square cells of [CELL] points of the page, each with the strokes that reach into it,
 * in their drawing order, clipped to the cell. The page's strokes come in no order on the page
 * (the core keeps them by id), so cells, not runs of strokes, keep a chunk small on the page.
 * A rectangle clip falls on whole pixels, so neighbouring cells meet without a seam and a stroke
 * across them looks as drawn in one piece.
 *
 * Tiles: squares of [TILE] pixels of the page drawn at a zoom level, used instead of the cells
 * when very many strokes are in view (a whole huge page, a board zoomed out).
 */
object ChunkPlan {
    const val CELL = 256
    const val TILE = 512
    /** Ink in view up to this many strokes is drawn stroke by stroke; past it from tiles (back below [DIRECT_BACK]). */
    const val DIRECT_MAX = 2000
    const val DIRECT_BACK = 1400
    /** Strokes drawn into new tiles per frame at most while older tiles can stand in for them. */
    const val BUDGET = 5000

    /** A cell's (or tile's) key from its column and row. */
    fun key(i: Int, j: Int): Long = (i.toLong() shl 32) or (j.toLong() and 0xFFFFFFFFL)
    fun col(key: Long): Int = (key shr 32).toInt()
    fun row(key: Long): Int = key.toInt()

    /**
     * A stroke counts as in every cell its bounds come this near (points): its anti-aliased edge
     * reaches a little past them, and a clip's edge falls on the nearest whole pixel.
     */
    const val REACH = 6f

    /**
     * The cells (first column, first row, last column, last row) a stroke with bounds (l, t, r, b)
     * in page points reaches into (see [REACH]).
     */
    fun cells(l: Float, t: Float, r: Float, b: Float): IntArray {
        val c = tiles(l - REACH, t - REACH, r + REACH, b + REACH, 1f, CELL)
        // A box of no size (a dot) still lies in one cell.
        if (c[2] < c[0]) c[2] = c[0]
        if (c[3] < c[1]) c[3] = c[1]
        return c
    }

    /**
     * The cells of [n] strokes with bounds [bounds] (left, top, right, bottom per stroke): for
     * each cell key the strokes in it, in their order.
     */
    fun plan(n: Int, bounds: FloatArray): LinkedHashMap<Long, ArrayList<Int>> {
        val out = LinkedHashMap<Long, ArrayList<Int>>()
        for (i in 0 until n) {
            val c = cells(bounds[4 * i], bounds[4 * i + 1], bounds[4 * i + 2], bounds[4 * i + 3])
            for (j in c[1]..c[3]) for (k in c[0]..c[2]) out.getOrPut(key(k, j)) { ArrayList() } += i
        }
        return out
    }

    /** Whether a box (l, t, r, b) shows in the view (vl, vt, vr, vb), all in page points. */
    fun shows(l: Float, t: Float, r: Float, b: Float, vl: Float, vt: Float, vr: Float, vb: Float) = vl < vr && vt < vb && l < vr && vl < r && t < vb && vt < b

    /**
     * The zoom level tiles are drawn at while the zoom changes: the power of √2 at or above
     * [scale] pixels per point, so a tile is reused over a range of zooms and is never more
     * than √2 too sharp (shrunk on screen, not blown up).
     */
    fun level(scale: Float): Float {
        val k = ceil(2.0 * ln(scale.toDouble()) / ln(2.0) - 1e-9)
        return Math.pow(2.0, k / 2.0).toFloat()
    }

    /** The tiles (first column, first row, last column, last row) of [size] pixels at [level] pixels per point over (l, t, r, b) page points. */
    fun tiles(l: Float, t: Float, r: Float, b: Float, level: Float, size: Int = TILE): IntArray {
        val k = level / size
        return intArrayOf(floor(l * k).toInt(), floor(t * k).toInt(), ceil(r * k).toInt() - 1, ceil(b * k).toInt() - 1)
    }
}

/**
 * A page's ink for one page view, in chunks: cells of the page (see [ChunkPlan]), each its own
 * RenderNode with the strokes in it. A frame draws only the cells in view; a new or erased
 * stroke re-records only the cells it is in. Each cell keeps the strokes' order and the cells
 * meet on whole pixels, so they look exactly like one picture of all the strokes.
 *
 * With very many strokes in view (a whole huge page, a board zoomed out) the ink is drawn from
 * tiles instead: squares of the page, each a GPU layer of the chunks over it, so a frame is a few
 * pictures however much ink there is. At rest they are drawn at the zoom itself, on whole
 * pixels, so they look as the strokes do; while the zoom changes, at the next power of √2 (a
 * tile serves a range of zooms) and the tiles of the zoom before stand in for the ones not drawn
 * yet, a few new ones per frame.
 */
class InkChunks(private val name: String) {
    private class Chunk(val node: RenderNode, val key: Long) {
        var strokes: Array<InkStroke> = emptyArray()
        /** Per stroke: drawn (not being erased or carried). */
        var drawn = BooleanArray(0)
        val box = RectF()
        var pencil = false
        var texel = 0f
        var look = 0
        var valid = false
        /** Counts its recordings (a tile over it is recorded anew when it moves on). */
        var gen = 0
    }

    private class Tile(val node: RenderNode) {
        var sig = 0L
        var stamp = -1
        var empty = true
        var used = 0L
    }

    private var chunks = ArrayList<Chunk>()
    private val byKey = HashMap<Long, Chunk>()
    private var version = -1
    /** The strokes as last planned, and whether none was being erased or carried then. */
    private var planned: Array<InkStroke> = emptyArray()
    private var plannedPlain = true
    private var texts: RenderNode? = null
    private var textsOf: Any? = null
    private var textsSkip: String? = null
    private var keeper: RenderNode? = null
    private var keeperOf: List<Chunk>? = null

    /** Moves on with every change of the chunks (tiles check theirs only then). */
    private var stamp = 0
    private var lastTexel = 0f
    private var lastLook = 0
    /** The zoom is changing: pencil chunks keep the grain they were recorded with until it rests. */
    private var zooming = false
    private var tiled = false
    private var tiles = HashMap<Long, Tile>()
    private var level = 0f
    private var prev: HashMap<Long, Tile>? = null
    private var prevLevel = 0f
    private var frame = 0L
    private val m = Matrix()
    private val mv = FloatArray(9)

    /** Chunks (pieces of ink) recorded in the last [draw] (for measuring; tiles only arrange them). */
    var recorded = 0; private set

    /**
     * Draws [st]'s text boxes and ink that lie in [vis] (page points; null: all), recording what
     * changed; the canvas is in page points. [texel], [look]: the pencil grain's size and look
     * now; [skip]: a text box being edited (not drawn); [stroke] draws one stroke. With [tiles]
     * the ink may come from tiles (the page view's own drawing: the canvas only scales and
     * moves; [frame] counts the view's frames, [zooming]: the zoom is changing). Returns true when
     * tiles are still to be drawn (draw another frame).
     */
    fun draw(c: Canvas, st: SheetState, vis: RectF?, texel: Float, look: Int, skip: String?, tiles: Boolean, frame: Long, zooming: Boolean, stroke: (Canvas, InkStroke) -> Unit): Boolean {
        recorded = 0
        if (st.version != version) plan(st)
        if (texel != lastTexel || look != lastLook) { lastTexel = texel; lastLook = look; stamp++ }
        // Come to rest: the grain of the zoom now (tiles check their chunks again).
        if (this.zooming && !zooming) stamp++
        this.zooming = zooming
        if (st.texts.isNotEmpty()) drawTexts(c, st, skip)
        keep(c)
        if (tiles) {
            var inView = 0
            if (vis != null) for (ch in chunks) if (shows(ch.box, vis)) inView += ch.strokes.size
            tiled = vis != null && inView > (if (tiled) ChunkPlan.DIRECT_BACK else ChunkPlan.DIRECT_MAX)
            if (tiled && vis != null) {
                this.frame = frame
                val more = drawTiles(c, vis, texel, look, zooming, stroke)
                if (more != null) return more
                tiled = false
            }
            dropTiles()
        }
        for (ch in chunks) {
            if (vis != null && !shows(ch.box, vis)) continue
            ensure(ch, texel, look, stroke)
            c.drawRenderNode(ch.node)
        }
        return false
    }

    private fun shows(b: RectF, v: RectF) = ChunkPlan.shows(b.left, b.top, b.right, b.bottom, v.left, v.top, v.right, v.bottom)

    /**
     * Holds on to every chunk's recording while the page is drawn: Android drops the recording
     * of a node that is in no picture of a frame, also when it only moves from one tile to the
     * next (a zoom step makes new tiles) — then each zoom step recorded all the ink again. The
     * keeper is in every frame (invisible, drawing nothing) with all chunks in it.
     */
    private fun keep(c: Canvas) {
        val k = keeper ?: RenderNode("$name-keep").also { it.setPosition(0, 0, 1, 1); it.alpha = 0f; keeper = it }
        if (keeperOf !== chunks || !k.hasDisplayList()) {
            val rc = k.beginRecording()
            for (ch in chunks) rc.drawRenderNode(ch.node)
            k.endRecording()
            keeperOf = chunks
        }
        c.save()
        c.clipRect(0f, 0f, 0f, 0f)
        c.drawRenderNode(k)
        c.restore()
    }

    private fun ensure(ch: Chunk, texel: Float, look: Int, stroke: (Canvas, InkStroke) -> Unit) {
        val why = when {
            !ch.valid -> 0
            !ch.node.hasDisplayList() -> 1
            ch.pencil && !zooming && (ch.texel != texel || ch.look != look) -> 2
            else -> return
        }
        if (PerfBench.on) PerfBench.why[why]++
        record(ch, texel, look, stroke)
    }

    /** Lets go of every recording (the page is gone from the view). */
    fun discard() {
        for (ch in chunks) ch.node.discardDisplayList()
        chunks = ArrayList()
        byKey.clear()
        planned = emptyArray()
        texts?.discardDisplayList()
        textsOf = null
        keeper?.discardDisplayList()
        keeperOf = null
        version = -1
        dropTiles()
    }

    private fun record(ch: Chunk, texel: Float, look: Int, stroke: (Canvas, InkStroke) -> Unit) {
        val b = ch.box
        // In page points as they are (every cell under the very same transform, so the clips of
        // two cells meet on the same pixel edge), clipped to the cell.
        ch.node.setPosition(0, 0, ceil(b.right).toInt().coerceAtLeast(1), ceil(b.bottom).toInt().coerceAtLeast(1))
        val rc = ch.node.beginRecording()
        rc.clipRect(b)
        for (i in ch.strokes.indices) if (ch.drawn[i]) stroke(rc, ch.strokes[i])
        ch.node.endRecording()
        ch.valid = true
        ch.texel = texel
        ch.look = look
        ch.gen++
        stamp++
        recorded++
    }

    // --- tiles ---------------------------------------------------------------------------------

    /**
     * The ink from tiles; null when the canvas doesn't allow it (turned or skewed), else whether
     * tiles are still missing.
     */
    private fun drawTiles(c: Canvas, vis: RectF, texel: Float, look: Int, zooming: Boolean, stroke: (Canvas, InkStroke) -> Unit): Boolean? {
        @Suppress("DEPRECATION") c.getMatrix(m)
        m.getValues(mv)
        val s = mv[Matrix.MSCALE_X]
        if (s <= 0f || mv[Matrix.MSKEW_X] != 0f || mv[Matrix.MSKEW_Y] != 0f || mv[Matrix.MSCALE_Y] != s || mv[Matrix.MPERSP_0] != 0f || mv[Matrix.MPERSP_1] != 0f) return null
        val lv = if (zooming) ChunkPlan.level(s) else s
        if (lv != level) {
            // A new zoom level: the old tiles stand in until the new ones are drawn.
            prev?.values?.forEach { it.node.discardDisplayList() }
            prev = tiles.takeIf { it.isNotEmpty() }
            prevLevel = level
            tiles = HashMap()
            level = lv
        }
        val size = ChunkPlan.TILE
        val k = s / lv
        // At the zoom itself (k = 1) on whole pixels: the tiles' pixels are the screen's.
        val ox = if (k == 1f) mv[Matrix.MTRANS_X].roundToInt().toFloat() else mv[Matrix.MTRANS_X]
        val oy = if (k == 1f) mv[Matrix.MTRANS_Y].roundToInt().toFloat() else mv[Matrix.MTRANS_Y]
        val r = ChunkPlan.tiles(vis.left, vis.top, vis.right, vis.bottom, lv)
        var budget = ChunkPlan.BUDGET
        var missing: ArrayList<Long>? = null
        val area = RectF()
        c.save()
        val tm = Matrix().apply { setScale(k, k); postTranslate(ox, oy) }
        c.setMatrix(tm)
        val cols = r[2] - r[0] + 1
        // The chunks over each tile in view, in drawing order (one pass over the chunks, made
        // only in a frame where some tile has to look at them).
        var over: Array<ArrayList<Chunk>?>? = null
        fun overOf(i: Int, j: Int): List<Chunk> {
            val o = over ?: arrayOfNulls<ArrayList<Chunk>>(cols * (r[3] - r[1] + 1)).also { a ->
                over = a
                for (ch in chunks) {
                    val b = ch.box
                    if (!shows(b, vis)) continue
                    val q = ChunkPlan.tiles(b.left, b.top, b.right, b.bottom, lv)
                    for (y in maxOf(q[1], r[1])..minOf(q[3], r[3])) for (x in maxOf(q[0], r[0])..minOf(q[2], r[2])) {
                        val at = (y - r[1]) * cols + (x - r[0])
                        (a[at] ?: ArrayList<Chunk>().also { a[at] = it }) += ch
                    }
                }
            }
            return o[(j - r[1]) * cols + (i - r[0])] ?: emptyList()
        }
        for (j in r[1]..r[3]) for (i in r[0]..r[2]) {
            val key = (i.toLong() shl 32) or (j.toLong() and 0xFFFFFFFFL)
            area.set(i * size / lv, j * size / lv, (i + 1) * size / lv, (j + 1) * size / lv)
            var t = tiles[key]
            if (t == null || t.stamp != stamp || (!t.empty && !t.node.hasDisplayList())) {
                val mine = overOf(i, j)
                var sig = 17L; var n = 0
                for (ch in mine) { sig = sig * 31 + System.identityHashCode(ch) * 7L + ch.gen; n += ch.strokes.size }
                val stale = t == null || t.sig != sig || (!t.empty && !t.node.hasDisplayList())
                if (stale && n > 0 && budget <= 0 && t == null && prevCovers(area)) {
                    (missing ?: ArrayList<Long>().also { missing = it }) += key
                    continue
                }
                if (t == null) t = Tile(RenderNode("$name-tile").apply { setPosition(0, 0, size, size); setUseCompositingLayer(true, null) }).also { tiles[key] = it }
                if (stale) {
                    // Chunks over the tile recorded first (their recordings are what the tile draws).
                    for (ch in mine) ensure(ch, texel, look, stroke)
                    sig = 17L
                    for (ch in mine) sig = sig * 31 + System.identityHashCode(ch) * 7L + ch.gen
                    recordTile(t, i, j, lv, mine, n)
                    t.sig = sig
                    budget -= n
                }
                t.stamp = stamp
            }
            t.used = frame
            if (!t.empty) {
                c.save()
                c.translate((i * size).toFloat(), (j * size).toFloat())
                c.drawRenderNode(t.node)
                c.restore()
            }
        }
        c.restore()
        // Not drawn yet: the zoom before's tiles in their place.
        val old = prev
        val miss = missing
        if (miss != null && old != null) {
            c.save()
            c.setMatrix(tm)
            val pk = s / prevLevel
            val pm = Matrix().apply { setScale(pk, pk); postTranslate(mv[Matrix.MTRANS_X], mv[Matrix.MTRANS_Y]) }
            for (key in miss) {
                val i = (key shr 32).toInt(); val j = key.toInt()
                c.save()
                c.clipRect((i * size).toFloat(), (j * size).toFloat(), ((i + 1) * size).toFloat(), ((j + 1) * size).toFloat())
                c.setMatrix(pm)
                area.set(i * size / lv, j * size / lv, (i + 1) * size / lv, (j + 1) * size / lv)
                for ((pkey, pt) in old) {
                    if (pt.empty) continue
                    val pi = (pkey shr 32).toInt(); val pj = pkey.toInt()
                    if (!ChunkPlan.shows(pi * size / prevLevel, pj * size / prevLevel, (pi + 1) * size / prevLevel, (pj + 1) * size / prevLevel, area.left, area.top, area.right, area.bottom)) continue
                    c.save()
                    c.translate((pi * size).toFloat(), (pj * size).toFloat())
                    c.drawRenderNode(pt.node)
                    c.restore()
                }
                c.restore()
            }
            c.restore()
        }
        // Tiles out of view go; the zoom before's once nothing stands in any more.
        val gone = tiles.entries.filter { it.value.used < frame - 1 }
        for (e in gone) { e.value.node.discardDisplayList(); tiles.remove(e.key) }
        if (miss == null) { old?.values?.forEach { it.node.discardDisplayList() }; prev = null }
        return miss != null
    }

    /** Whether the zoom before's tiles cover all of [area] (page points), so they can stand in for a tile there. */
    private fun prevCovers(area: RectF): Boolean {
        val old = prev ?: return false
        val r = ChunkPlan.tiles(area.left, area.top, area.right, area.bottom, prevLevel)
        for (j in r[1]..r[3]) for (i in r[0]..r[2]) {
            val t = old[(i.toLong() shl 32) or (j.toLong() and 0xFFFFFFFFL)] ?: return false
            if (!t.empty && !t.node.hasDisplayList()) return false
        }
        return true
    }

    /** Tile (i, j) at [lv] pixels per point: the chunks over it ([mine]), [n] strokes in all. */
    private fun recordTile(t: Tile, i: Int, j: Int, lv: Float, mine: List<Chunk>, n: Int) {
        t.empty = n == 0
        if (t.empty) { t.node.discardDisplayList(); return }
        val size = ChunkPlan.TILE
        val rc = t.node.beginRecording(size, size)
        rc.translate(-(i * size).toFloat(), -(j * size).toFloat())
        rc.scale(lv, lv)
        for (ch in mine) rc.drawRenderNode(ch.node)
        t.node.endRecording()
    }

    private fun dropTiles() {
        for (t in tiles.values) t.node.discardDisplayList()
        tiles.clear()
        prev?.values?.forEach { it.node.discardDisplayList() }
        prev = null
        level = 0f
    }

    private fun drawTexts(c: Canvas, st: SheetState, skip: String?) {
        val n = texts ?: RenderNode("$name-texts").also { it.setClipToBounds(false); texts = it }
        if (textsOf !== st.texts || textsSkip != skip || !n.hasDisplayList()) {
            val rc = n.beginRecording()
            TextBoxes.draw(rc, st.texts, skip = skip)
            n.endRecording()
            textsOf = st.texts; textsSkip = skip
            recorded++
        }
        c.drawRenderNode(n)
    }

    // --- chunks --------------------------------------------------------------------------------

    /**
     * Puts the strokes into their cells again; cells with the very same strokes keep their
     * recording. Strokes only added at the end (writing) go into their cells without looking at
     * the others.
     */
    private fun plan(st: SheetState) {
        if (PerfBench.on) PerfBench.why[3]++
        version = st.version
        stamp++
        val strokes = st.strokes
        val n = strokes.size
        val plain = st.hidden.isEmpty() && st.floating.isEmpty()
        fun drawn(s: InkStroke) = plain || (s.id !in st.hidden && s.id !in st.floating)
        val old = planned
        var appended = plain && plannedPlain && n >= old.size
        if (appended) for (i in old.indices) if (old[i] !== strokes[i]) { appended = false; break }
        // The same strokes, only some being erased or carried (or no longer): the cells stay,
        // those whose strokes show differently are recorded again.
        var flagsOnly = !appended && n == old.size
        if (flagsOnly) for (i in old.indices) if (old[i] !== strokes[i]) { flagsOnly = false; break }
        planned = strokes.toTypedArray()
        plannedPlain = plain
        if (flagsOnly) {
            for (ch in chunks) {
                val now = BooleanArray(ch.strokes.size) { drawn(ch.strokes[it]) }
                if (!now.contentEquals(ch.drawn)) { ch.drawn = now; ch.valid = false }
            }
            return
        }
        if (appended) {
            if (n == old.size) return
            val grown = LinkedHashMap<Chunk, ArrayList<InkStroke>>()
            for (i in old.size until n) {
                val s = strokes[i]
                val c = ChunkPlan.cells(s.bounds.left, s.bounds.top, s.bounds.right, s.bounds.bottom)
                for (y in c[1]..c[3]) for (x in c[0]..c[2]) {
                    val k = ChunkPlan.key(x, y)
                    val ch = byKey[k] ?: newChunk(k)
                    grown.getOrPut(ch) { ArrayList() } += s
                }
            }
            for ((ch, add) in grown) {
                ch.strokes = ch.strokes + add.toTypedArray()
                ch.drawn = BooleanArray(ch.strokes.size) { true }
                ch.pencil = ch.pencil || add.any { it.kind == NotebookView.KIND_PENCIL }
                ch.valid = false
            }
            return
        }
        val bounds = FloatArray(4 * n)
        for (i in 0 until n) {
            val b = strokes[i].bounds
            bounds[4 * i] = b.left; bounds[4 * i + 1] = b.top; bounds[4 * i + 2] = b.right; bounds[4 * i + 3] = b.bottom
        }
        val cells = ChunkPlan.plan(n, bounds)
        val gone = HashMap(byKey)
        val next = ArrayList<Chunk>(cells.size)
        for ((k, idx) in cells) {
            val inCell = Array(idx.size) { strokes[idx[it]] }
            val drawnNow = BooleanArray(idx.size) { drawn(inCell[it]) }
            val prev = gone.remove(k)
            if (prev != null && same(prev, inCell, drawnNow)) {
                // Same strokes (also reloaded after an undo, as long as they are the same): kept.
                prev.strokes = inCell
                next += prev
                continue
            }
            val ch = prev ?: makeChunk(k)
            ch.strokes = inCell
            ch.drawn = drawnNow
            ch.pencil = inCell.any { it.kind == NotebookView.KIND_PENCIL }
            ch.valid = false
            next += ch
        }
        for ((k, ch) in gone) { ch.node.discardDisplayList(); byKey.remove(k) }
        chunks = next
    }

    /** A new (empty) cell at [k] among the chunks. */
    private fun newChunk(k: Long): Chunk = makeChunk(k).also { ch -> chunks = ArrayList(chunks).also { it += ch } }

    /** A cell at [k] (its node and box), known by its key. */
    private fun makeChunk(k: Long): Chunk {
        if (PerfBench.on) PerfBench.why[4]++
        val ch = Chunk(RenderNode(name).also { it.setClipToBounds(false) }, k)
        val x = ChunkPlan.col(k) * ChunkPlan.CELL.toFloat(); val y = ChunkPlan.row(k) * ChunkPlan.CELL.toFloat()
        ch.box.set(x, y, x + ChunkPlan.CELL, y + ChunkPlan.CELL)
        byKey[k] = ch
        return ch
    }

    private fun same(ch: Chunk, strokes: Array<InkStroke>, drawn: BooleanArray): Boolean {
        if (ch.strokes.size != strokes.size || !ch.drawn.contentEquals(drawn)) return false
        for (i in strokes.indices) {
            val a = ch.strokes[i]; val b = strokes[i]
            if (a !== b && (a.id != b.id || a.color != b.color || a.kind != b.kind || !a.outline.contentEquals(b.outline))) return false
        }
        return true
    }
}
