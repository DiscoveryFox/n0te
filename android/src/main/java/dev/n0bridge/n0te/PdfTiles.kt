package dev.n0bridge.n0te

import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.Paint
import android.graphics.RectF
import android.util.LruCache
import uniffi.n0te_ffi.FfiTile
import uniffi.n0te_ffi.FfiTileRequest
import uniffi.n0te_ffi.TileSink
import java.nio.ByteBuffer
import kotlin.math.ceil
import kotlin.math.max
import kotlin.math.min

/**
 * A PDF page's size in pixels at [b] pixels per point — rounded exactly like the core's
 * `page_pixels` (pinned by PdfBindingsTest). With `ceil` an A4 PDF (595.28 × 841.89 pt) came
 * out one pixel larger at some zoom levels; the worker refused the last tile column as
 * "outside the page" and that strip of the PDF (and the whole thumbnail) stayed empty.
 */
fun pagePixels(w: Float, h: Float, b: Float): Pair<Int, Int> =
    maxOf(1, Math.round(w * b)) to maxOf(1, Math.round(h * b))

/** Pixels per point of a PDF page's whole-page picture (under its sharp tiles). */
const val PDF_THUMB = 0.25f

/**
 * The scale of the whole-page picture of a [w] × [h] point PDF page: [PDF_THUMB] pixels per point,
 * halved until it is at most 2048 pixels a side, so its memory doesn't grow with the page (a
 * poster of a few metres gets a coarser one; the sharp tiles on top only cover the screen).
 */
fun pdfThumbScale(w: Float, h: Float): Float {
    var t = PDF_THUMB
    while (maxOf(w, h) * t > 2048f && t > 1f / 1024f) t /= 2f
    return t
}

/**
 * Which PDF tiles the page views have asked the worker for and not got yet (main thread). The
 * worker drops the queued tiles of a view when that view's generation moves on (also the
 * thumbnails, which wait behind the sharp tiles), merges a request for a tile another view asked
 * for into that one (the answer names only the last view), and sends nothing for what it drops.
 * So a tile is forgotten when its view's generation moves on, done for every view when it
 * arrives, and asked again when no answer came for [PENDING_MS]: none stays waiting for good.
 */
class TileWait {
    /** Per tile key: the views waiting for it and when each asked. */
    private val pending = HashMap<String, HashMap<UInt, Long>>()
    /** Tiles the worker refused, and when: asked again only after [RETRY_MS] (not every frame). */
    private val refused = HashMap<String, Long>()
    /** Tiles waited for. */
    val size get() = pending.size

    /** Whether [view] should ask for tile [key] now (and remembers that it did). */
    fun ask(key: String, view: UInt, now: Long): Boolean {
        refused[key]?.let { if (now - it < RETRY_MS) return false }
        val views = pending.getOrPut(key) { HashMap(2) }
        val at = views[view]
        if (at != null && now - at < PENDING_MS) return false
        views[view] = now
        return true
    }

    fun arrived(key: String) { pending.remove(key); refused.remove(key) }
    fun failed(key: String, now: Long) { pending.remove(key); refused[key] = now }

    /** [view]'s zoom level changed: the worker drops what it had queued for it. */
    fun newGeneration(view: UInt) {
        val it = pending.values.iterator()
        while (it.hasNext()) { val views = it.next(); views.remove(view); if (views.isEmpty()) it.remove() }
    }

    companion object {
        const val RETRY_MS = 4000L
        const val PENDING_MS = 5000L
    }
}

/**
 * PDF page backgrounds as bitmap tiles from the core's PDF worker. A page is drawn from a small
 * thumbnail (always requested, so a fast scroll never shows white) with sharp 512 px tiles at
 * the zoom's power-of-two scale on top. Tiles arrive on the worker thread; the cache and the
 * pending set live on the main thread.
 */
object PdfTiles : TileSink {
    private const val TILE = 512
    private const val THUMB = PDF_THUMB

    /**
     * One page view's zoom level and request generation: two panes (and the page pictures) each
     * have their own, so a zoom in one doesn't drop the tiles the other is waiting for.
     */
    class Slot {
        internal val view: UInt = nextView++
        internal var lastBucket = 0f
        internal var generation = 1uL
    }
    /** 2 is the screenshots'. */
    private var nextView = 3u

    private val cache = object : LruCache<String, Bitmap>(96 * 1024 * 1024) {
        override fun sizeOf(key: String, value: Bitmap) = value.byteCount
    }
    /** Tiles asked for and not there yet (see [TileWait]). */
    private val waiting = TileWait()
    private val paint = Paint(Paint.FILTER_BITMAP_FLAG)
    private val dst = RectF()

    /** Called on the main thread when a tile arrived (redraw). */
    var listener: (() -> Unit)? = null

    /** Smallest power of two at or above [scale] pixels per point. */
    fun bucketFor(scale: Float): Float {
        var b = THUMB
        while (b < scale && b < 8f) b *= 2f
        return b
    }

    private fun key(blob: String, page: UInt, b: Float, tx: Int, ty: Int) = "$blob/$page/$b/$tx/$ty"


    /**
     * Draws [p]'s PDF page (canvas in world coordinates) and adds the tiles it lacks to [out].
     * [vis] is the visible world rectangle, [screenScale] pixels per point on screen.
     */
    fun draw(c: Canvas, p: Placed, screenScale: Float, vis: RectF, out: MutableList<FfiTileRequest>, slot: Slot) {
        val blob = p.spec.pdfBlob ?: return
        val page = p.spec.pdfPage
        val r = p.rect
        val b = bucketFor(screenScale)
        if (b != slot.lastBucket) {
            // Zoom level changed: tiles this view queued for the old level are no longer wanted.
            slot.lastBucket = b
            slot.generation++
            waiting.newGeneration(slot.view)
        }
        // Space inserted into the page splits the PDF: each band of it is drawn where it lies now
        // (the PDF at its own size, moved down by the space above), blank paper in between.
        val gaps = p.spec.gaps
        val base = p.spec.height - (1 until gaps.size step 2).sumOf { gaps[it].toDouble() }.toFloat()
        for (band in bands(gaps, base)) {
            val (src, len, dst) = band
            val origin = RectF(r.left, r.top + dst - src, r.right, r.top + dst - src + base)
            val clip = RectF(r.left, r.top + dst, r.right, r.top + dst + len)
            if (gaps.isNotEmpty()) { c.save(); c.clipRect(clip) }
            // Underneath: the thumbnail (a few tiles for the whole page).
            val thumb = pdfThumbScale(p.spec.width, base)
            drawLevel(c, blob, page, origin, p.spec.width, base, thumb, clip, out, priority = 10u, slot)
            if (b > thumb) {
                val part = RectF(max(vis.left, clip.left), max(vis.top, clip.top), min(vis.right, clip.right), min(vis.bottom, clip.bottom))
                if (part.width() > 0 && part.height() > 0) drawLevel(c, blob, page, origin, p.spec.width, base, b, part, out, priority = 0u, slot)
            }
            if (gaps.isNotEmpty()) c.restore()
        }
    }

    /** The PDF's bands on the page (src, len, dst), as the core's SheetSpec::bands: [gaps] are (at, height) pairs. */
    fun bands(gaps: List<Float>, base: Float): List<FloatArray> {
        val out = ArrayList<FloatArray>(gaps.size / 2 + 1)
        var src = 0f; var shift = 0f
        for (i in 0 until gaps.size / 2) {
            val at = gaps[2 * i]; val h = gaps[2 * i + 1]
            out += floatArrayOf(src, at - src, src + shift)
            shift += h; src = at
        }
        out += floatArrayOf(src, base - src, src + shift)
        return out
    }

    /** [w]×[h]: the PDF page in points (the sheet's size, exactly what the worker renders). */
    private fun drawLevel(c: Canvas, blob: String, page: UInt, r: RectF, w: Float, h: Float, b: Float, part: RectF, out: MutableList<FfiTileRequest>, priority: UInt, slot: Slot) {
        val (pw, ph) = pagePixels(w, h, b)
        val x0 = ((part.left - r.left) * b).toInt().coerceIn(0, pw - 1) / TILE
        val y0 = ((part.top - r.top) * b).toInt().coerceIn(0, ph - 1) / TILE
        val x1 = (ceil((part.right - r.left) * b).toInt() - 1).coerceIn(0, pw - 1) / TILE
        val y1 = (ceil((part.bottom - r.top) * b).toInt() - 1).coerceIn(0, ph - 1) / TILE
        for (ty in y0..y1) for (tx in x0..x1) {
            val x = tx * TILE; val y = ty * TILE
            val tw = min(TILE, pw - x); val th = min(TILE, ph - y)
            val k = key(blob, page, b, tx, ty)
            val bmp = cache.get(k)
            if (bmp != null) {
                dst.set(r.left + x / b, r.top + y / b, r.left + (x + tw) / b, r.top + (y + th) / b)
                c.drawBitmap(bmp, null, dst, paint)
            } else if (waiting.ask(k, slot.view, android.os.SystemClock.uptimeMillis())) {
                out += FfiTileRequest(blob, page, b, x.toUInt(), y.toUInt(), tw.toUInt(), th.toUInt(), priority, slot.view, slot.generation)
            }
        }
    }

    fun flush(out: List<FfiTileRequest>) {
        if (out.isEmpty()) return
        val svc = N0te.pdf ?: return
        // Only enqueues on the PDF worker: cheap, and must not wait behind a long import on n0te-io.
        runCatching { svc.requestTiles(out) }
    }

    /**
     * Screenshots: one tile each, at their own resolution (view 2), handed to a callback. All in
     * one generation: a newer one would make the worker drop the snips still queued (the bands of
     * a PDF split by inserted space are several snips at once; the screenshot never came).
     */
    private const val SNIP_VIEW = 2u
    private const val SNIP_GENERATION = 1uL
    private val snips = HashMap<String, MutableList<(Bitmap?) -> Unit>>()

    /**
     * Renders part of a PDF page at [scale] pixels per point (the region in page points), for a
     * screenshot in full resolution whatever the zoom; [done] on the main thread (null: failed).
     */
    fun snip(blob: String, page: UInt, pageW: Float, pageH: Float, scale: Float, region: RectF, done: (Bitmap?) -> Unit) {
        val svc = N0te.pdf ?: return done(null)
        val (pw, ph) = pagePixels(pageW, pageH, scale)
        val x = (region.left * scale).toInt().coerceIn(0, pw - 1)
        val y = (region.top * scale).toInt().coerceIn(0, ph - 1)
        val w = (ceil(region.right * scale).toInt() - x).coerceIn(1, pw - x)
        val h = (ceil(region.bottom * scale).toInt() - y).coerceIn(1, ph - y)
        val k = key(blob, page, scale, x, y) + "/$w/$h/snip"
        snips.getOrPut(k) { ArrayList(1) } += done
        runCatching { svc.requestTiles(listOf(FfiTileRequest(blob, page, scale, x.toUInt(), y.toUInt(), w.toUInt(), h.toUInt(), 0u, SNIP_VIEW, SNIP_GENERATION))) }
            .onFailure { snips.remove(k)?.forEach { it(null) } }
    }

    /** Worker thread. */
    override fun onTile(tile: FfiTile) {
        val r = tile.request
        if (r.view == SNIP_VIEW) {
            val k = key(r.blob, r.page, r.scale, r.x.toInt(), r.y.toInt()) + "/${r.width}/${r.height}/snip"
            val bmp = if (tile.error == null && tile.rgba.size >= r.width.toInt() * r.height.toInt() * 4)
                Bitmap.createBitmap(r.width.toInt(), r.height.toInt(), Bitmap.Config.ARGB_8888).also { it.copyPixelsFromBuffer(ByteBuffer.wrap(tile.rgba)) } else null
            N0te.main.post { snips.remove(k)?.forEach { it(bmp) } }
            return
        }
        val k = key(r.blob, r.page, r.scale, (r.x.toInt() / TILE), (r.y.toInt() / TILE))
        if (tile.error != null || tile.rgba.size < r.width.toInt() * r.height.toInt() * 4) {
            android.util.Log.w(N0te.TAG, "PDF tile $k: ${tile.error ?: "short buffer"}")
            N0te.main.post { waiting.failed(k, android.os.SystemClock.uptimeMillis()) }
            return
        }
        // ARGB_8888 is stored as R, G, B, A bytes: the core's RGBA copies straight in.
        val bmp = Bitmap.createBitmap(r.width.toInt(), r.height.toInt(), Bitmap.Config.ARGB_8888)
        bmp.copyPixelsFromBuffer(ByteBuffer.wrap(tile.rgba))
        N0te.main.post {
            waiting.arrived(k)
            cache.put(k, bmp)
            listener?.invoke()
        }
    }
}
