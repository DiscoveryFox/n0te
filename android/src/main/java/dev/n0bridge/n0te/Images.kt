package dev.n0bridge.n0te

import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.graphics.Canvas
import android.graphics.Color
import android.util.LruCache
import java.io.ByteArrayOutputStream
import java.io.File

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

    /** Decodes a file to at most [maxSide] px on the long side. */
    fun decode(path: String, maxSide: Int): Bitmap? {
        val o = BitmapFactory.Options().apply { inJustDecodeBounds = true }
        BitmapFactory.decodeFile(path, o)
        if (o.outWidth <= 0 || o.outHeight <= 0) return null
        var sample = 1
        while (maxOf(o.outWidth, o.outHeight) / (sample * 2) >= maxSide) sample *= 2
        return BitmapFactory.decodeFile(path, BitmapFactory.Options().apply { inSampleSize = sample })
    }

    /**
     * A picked picture as n0te stores it: upright (EXIF), at most [maxSide] px, on white (PNG
     * transparency), JPEG. Returns the bytes and the pixel size.
     */
    fun prepare(bytes: ByteArray, maxSide: Int = 3200): Triple<ByteArray, Int, Int>? {
        val o = BitmapFactory.Options().apply { inJustDecodeBounds = true }
        BitmapFactory.decodeByteArray(bytes, 0, bytes.size, o)
        if (o.outWidth <= 0 || o.outHeight <= 0) return null
        var sample = 1
        while (maxOf(o.outWidth, o.outHeight) / (sample * 2) >= maxSide) sample *= 2
        var bmp = BitmapFactory.decodeByteArray(bytes, 0, bytes.size, BitmapFactory.Options().apply { inSampleSize = sample }) ?: return null
        val rotate = runCatching {
            when (android.media.ExifInterface(bytes.inputStream()).getAttributeInt(android.media.ExifInterface.TAG_ORIENTATION, 1)) {
                android.media.ExifInterface.ORIENTATION_ROTATE_90 -> 90f
                android.media.ExifInterface.ORIENTATION_ROTATE_180 -> 180f
                android.media.ExifInterface.ORIENTATION_ROTATE_270 -> 270f
                else -> 0f
            }
        }.getOrDefault(0f)
        if (rotate != 0f) bmp = Bitmap.createBitmap(bmp, 0, 0, bmp.width, bmp.height, android.graphics.Matrix().apply { postRotate(rotate) }, true)
        if (maxOf(bmp.width, bmp.height) > maxSide) {
            val k = maxSide.toFloat() / maxOf(bmp.width, bmp.height)
            bmp = Bitmap.createScaledBitmap(bmp, (bmp.width * k).toInt().coerceAtLeast(1), (bmp.height * k).toInt().coerceAtLeast(1), true)
        }
        val flat = Bitmap.createBitmap(bmp.width, bmp.height, Bitmap.Config.ARGB_8888)
        Canvas(flat).apply { drawColor(Color.WHITE); drawBitmap(bmp, 0f, 0f, null) }
        return Triple(jpeg(flat, 92), flat.width, flat.height)
    }

    fun jpeg(bmp: Bitmap, quality: Int): ByteArray = ByteArrayOutputStream().also { bmp.compress(Bitmap.CompressFormat.JPEG, quality, it) }.toByteArray()
}
