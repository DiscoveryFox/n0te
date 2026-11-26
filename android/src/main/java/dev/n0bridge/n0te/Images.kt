package dev.n0bridge.n0te

import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.graphics.Canvas
import android.graphics.Color
import android.util.LruCache
import java.io.ByteArrayOutputStream
import java.io.File

/** Sizes for decoding pictures (pure: JVM-tested). */
object ImageSize {
    /** The power of two to decode a picture [long] px on its long side at: the coarsest that still has [maxSide] px. */
    fun sample(long: Int, maxSide: Int): Int {
        var s = 1
        while (long / (s * 2) >= maxSide) s *= 2
        return s
    }

    /** [w] × [h] shrunk to at most [maxSide] on the long side (as it is when smaller), never 0. */
    fun fit(w: Int, h: Int, maxSide: Int): Pair<Int, Int> {
        val long = maxOf(w, h)
        if (long <= maxSide) return w to h
        val k = maxSide.toFloat() / long
        return (w * k).toInt().coerceIn(1, maxSide) to (h * k).toInt().coerceIn(1, maxSide)
    }
}

/**
 * Images on pages: decoded once per blob (at most [MAX_SIDE] px on the long side) and kept in
 * an LRU; a blob not on disk yet (still syncing) is tried again later. Plus turning picked
 * pictures into what n0te stores (JPEG on white, sensible size).
 */
object Images {
    private const val MAX_SIDE = 2400
    private val cache = object : LruCache<String, Bitmap>(64 * 1024 * 1024) {
        override fun sizeOf(key: String, value: Bitmap) = value.byteCount
    }

    /**
     * Room for pictures after the device's memory (1/16 of it, 64 to 160 MB): a page with several
     * photos must fit at once, or they push each other out and are decoded again every frame.
     */
    fun init(ctx: android.content.Context) {
        val am = ctx.getSystemService(android.content.Context.ACTIVITY_SERVICE) as? android.app.ActivityManager ?: return
        val info = android.app.ActivityManager.MemoryInfo().also { am.getMemoryInfo(it) }
        cache.resize((info.totalMem / 16).coerceIn(64L shl 20, 160L shl 20).toInt())
    }
    private val loading = HashSet<String>()
    private val missing = HashMap<String, Long>()

    /** Called on the main thread when an image became available (redraw). */
    var listener: (() -> Unit)? = null

    /** The decoded image, or null (then it is being loaded and [listener] fires when ready). */
    fun get(blob: String): Bitmap? {
        cache.get(blob)?.let { return it }
        if (blob in loading) return null
        missing[blob]?.let { if (android.os.SystemClock.uptimeMillis() - it < 3000) return null }
        loading += blob
        N0te.io.execute {
            val bmp = runCatching {
                val path = N0te.library.blobPath(blob)
                if (!File(path).exists()) null else decode(path, MAX_SIDE)
            }.getOrNull()
            N0te.main.post {
                loading -= blob
                if (bmp != null) { cache.put(blob, bmp); missing.remove(blob); listener?.invoke() } else missing[blob] = android.os.SystemClock.uptimeMillis()
            }
        }
        return null
    }

    /** Decodes a file to at most [maxSide] px on the long side (sampled down, then shrunk to fit). */
    fun decode(path: String, maxSide: Int): Bitmap? {
        val o = BitmapFactory.Options().apply { inJustDecodeBounds = true }
        BitmapFactory.decodeFile(path, o)
        if (o.outWidth <= 0 || o.outHeight <= 0) return null
        val sample = ImageSize.sample(maxOf(o.outWidth, o.outHeight), maxSide)
        val bmp = BitmapFactory.decodeFile(path, BitmapFactory.Options().apply { inSampleSize = sample }) ?: return null
        val (w, h) = ImageSize.fit(bmp.width, bmp.height, maxSide)
        return if (w == bmp.width && h == bmp.height) bmp else replaced(bmp, Bitmap.createScaledBitmap(bmp, w, h, true))
    }

    /**
     * A picked picture as n0te stores it: upright (EXIF), at most [maxSide] px, on white (PNG
     * transparency), JPEG. Returns the bytes and the pixel size.
     */
    fun prepare(bytes: ByteArray, maxSide: Int = 3200): Triple<ByteArray, Int, Int>? {
        val o = BitmapFactory.Options().apply { inJustDecodeBounds = true }
        BitmapFactory.decodeByteArray(bytes, 0, bytes.size, o)
        if (o.outWidth <= 0 || o.outHeight <= 0) return null
        val sample = ImageSize.sample(maxOf(o.outWidth, o.outHeight), maxSide)
        var bmp = BitmapFactory.decodeByteArray(bytes, 0, bytes.size, BitmapFactory.Options().apply { inSampleSize = sample }) ?: return null
        val rotate = runCatching {
            when (android.media.ExifInterface(bytes.inputStream()).getAttributeInt(android.media.ExifInterface.TAG_ORIENTATION, 1)) {
                android.media.ExifInterface.ORIENTATION_ROTATE_90 -> 90f
                android.media.ExifInterface.ORIENTATION_ROTATE_180 -> 180f
                android.media.ExifInterface.ORIENTATION_ROTATE_270 -> 270f
                else -> 0f
            }
        }.getOrDefault(0f)
        // Shrunk first, then turned, each step letting go of the one before: a big photo (the
        // decoded picture is up to twice maxSide) was held in up to four full copies at once.
        if (maxOf(bmp.width, bmp.height) > maxSide) {
            val (w, h) = ImageSize.fit(bmp.width, bmp.height, maxSide)
            bmp = replaced(bmp, Bitmap.createScaledBitmap(bmp, w, h, true))
        }
        if (rotate != 0f) bmp = replaced(bmp, Bitmap.createBitmap(bmp, 0, 0, bmp.width, bmp.height, android.graphics.Matrix().apply { postRotate(rotate) }, true))
        val flat = Bitmap.createBitmap(bmp.width, bmp.height, Bitmap.Config.ARGB_8888)
        Canvas(flat).apply { drawColor(Color.WHITE); drawBitmap(bmp, 0f, 0f, null) }
        bmp.recycle()
        val out = Triple(jpeg(flat, 92), flat.width, flat.height)
        flat.recycle()
        return out
    }

    /** [next] made from [prev]: [prev] goes (unless it is the same picture). */
    private fun replaced(prev: Bitmap, next: Bitmap): Bitmap {
        if (next !== prev) prev.recycle()
        return next
    }

    fun jpeg(bmp: Bitmap, quality: Int): ByteArray = ByteArrayOutputStream().also { bmp.compress(Bitmap.CompressFormat.JPEG, quality, it) }.toByteArray()
}
