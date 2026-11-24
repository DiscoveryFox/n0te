package dev.n0bridge.n0te

import com.google.android.gms.tasks.Tasks
import com.google.mlkit.common.model.DownloadConditions
import com.google.mlkit.common.model.RemoteModelManager
import com.google.mlkit.vision.digitalink.recognition.DigitalInkRecognition
import com.google.mlkit.vision.digitalink.recognition.DigitalInkRecognitionModel
import com.google.mlkit.vision.digitalink.recognition.DigitalInkRecognitionModelIdentifier
import com.google.mlkit.vision.digitalink.recognition.DigitalInkRecognizer
import com.google.mlkit.vision.digitalink.recognition.DigitalInkRecognizerOptions
import com.google.mlkit.vision.digitalink.recognition.Ink
import com.google.mlkit.vision.digitalink.recognition.RecognitionContext
import com.google.mlkit.vision.digitalink.recognition.WritingArea
import uniffi.n0te_ffi.FfiIndexLine
import uniffi.n0te_ffi.SheetHandle
import java.nio.ByteBuffer
import kotlin.math.abs
import java.nio.ByteOrder
import java.util.concurrent.Executors
import java.util.concurrent.TimeUnit

/**
 * Reads handwriting so it can be searched: Google ML Kit's ink recogniser, on the tablet. The
 * model for German (about 20 MB) is downloaded once, over Wi-Fi; then it works offline and
 * nothing written leaves the device. What it reads goes into the page (the core's index:
 * searched here and on the laptop, never an undo step).
 */
object Handwriting {
    private val model: DigitalInkRecognitionModel? by lazy {
        runCatching { DigitalInkRecognitionModel.builder(DigitalInkRecognitionModelIdentifier.fromLanguageTag("de-DE")!!).build() }.getOrNull()
    }
    private val manager by lazy { RemoteModelManager.getInstance() }
    private var recognizer: DigitalInkRecognizer? = null
    /** Reading runs on one thread of its own, one line after the other. */
    val worker = Executors.newSingleThreadExecutor { r -> Thread(r, "handwriting").apply { priority = Thread.MIN_PRIORITY } }

    /** Where the model is: for the settings. */
    enum class State { UNKNOWN, MISSING, LOADING, READY, FAILED }
    @Volatile var state = State.UNKNOWN; private set
    @Volatile var error: String? = null; private set
    private val listeners = ArrayList<() -> Unit>()
    fun onChange(l: () -> Unit) { synchronized(listeners) { listeners += l } }
    fun offChange(l: () -> Unit) { synchronized(listeners) { listeners -= l } }
    private fun changed() { val ls = synchronized(listeners) { listeners.toList() }; N0te.main.post { ls.forEach { it() } } }

    val ready get() = state == State.READY

    /** Finds out whether the model is there (and loads it over Wi-Fi when [fetch] and it isn't). */
    fun check(fetch: Boolean) {
        val m = model ?: run { state = State.FAILED; error = "Deutsch nicht verfügbar"; changed(); return }
        if (state == State.READY || state == State.LOADING) return
        manager.isModelDownloaded(m).addOnSuccessListener { have ->
            if (have) { state = State.READY; changed() }
            else { state = State.MISSING; changed(); if (fetch) download() }
        }.addOnFailureListener { state = State.FAILED; error = it.message; changed() }
    }

    /** Loads the model (Wi-Fi only). */
    fun download() {
        val m = model ?: return
        if (state == State.LOADING || state == State.READY) return
        state = State.LOADING; error = null; changed()
        manager.download(m, DownloadConditions.Builder().requireWifi().build())
            .addOnSuccessListener { state = State.READY; changed() }
            .addOnFailureListener { state = State.FAILED; error = it.message; changed() }
    }

    /**
     * What a line of handwriting says: up to three readings, best first ([strokes]: each a run
     * of x, y, pressure, t samples, left to right). [before]: the text just before it (helps
     * the recogniser), [w] × [h] the line's size. On the reading thread only.
     */
    fun read(strokes: List<FloatArray>, before: String, w: Float, h: Float): List<String> {
        val m = model ?: return emptyList()
        if (!ready) return emptyList()
        val r = recognizer ?: DigitalInkRecognition.getClient(DigitalInkRecognizerOptions.builder(m).build()).also { recognizer = it }
        val ink = Ink.builder()
        var clock = 0L
        for (s in strokes) {
            val b = Ink.Stroke.builder()
            val n = s.size / 4
            for (i in 0 until n) b.addPoint(Ink.Point.create(s[i * 4], s[i * 4 + 1], clock + s[i * 4 + 3].toLong()))
            clock += (if (n > 0) s[(n - 1) * 4 + 3].toLong() else 0L) + 120L
            ink.addStroke(b.build())
        }
        val ctx = RecognitionContext.builder().setPreContext(before.takeLast(20)).setWritingArea(WritingArea(maxOf(w, 1f), maxOf(h, 1f))).build()
        return runCatching { Tasks.await(r.recognize(ink.build(), ctx), 15, TimeUnit.SECONDS).candidates.take(3).map { it.text } }.getOrDefault(emptyList())
    }

    /**
     * Reads the lines of [h]'s page that changed since it was last read and stores them;
     * lines whose strokes changed or went are dropped. True if anything changed (the page
     * wants saving). On the reading thread only.
     */
    fun indexSheet(h: SheetHandle): Boolean {
        if (!ready) return false
        val boxes = h.strokeBoxes().filter { it.kind.toInt() != 2 } // highlighter strokes say nothing
            .map { InkLines.Box(it.id, it.x0, it.y0, it.x1, it.y1) }
        val lines = InkLines.group(boxes)
        val keyed = lines.associateBy { InkLines.key(it) }
        val stored = h.indexKeys().toSet()
        val drop = stored.filter { it !in keyed }
        val fresh = keyed.filterKeys { it !in stored }
        if (fresh.isEmpty() && drop.isEmpty()) return false
        // Reading order for the text before each line.
        val known = h.indexLines().associate { it.key to it.text }
        val order = lines.sortedWith(compareBy({ it.minOf { b -> b.y0 } }, { it.minOf { b -> b.x0 } }))
        val put = ArrayList<FfiIndexLine>()
        var before = ""
        for (line in order) {
            val k = InkLines.key(line)
            if (k !in fresh) { before = known[k] ?: before; continue }
            val ids = line.sortedBy { it.x0 }.map { it.id }
            val data = runCatching { h.strokeData(ids) }.getOrNull() ?: continue
            val strokes = data.map { floatsLE(it.samples) }
            val x0 = line.minOf { it.x0 }; val y0 = line.minOf { it.y0 }; val x1 = line.maxOf { it.x1 }; val y1 = line.maxOf { it.y1 }
            val read = read(strokes, before, x1 - x0, y1 - y0)
            if (read.isEmpty()) continue
            put += FfiIndexLine(k, read[0], read.drop(1), ids, x0, y0, x1, y1)
            before = read[0]
        }
        if (put.isEmpty() && drop.isEmpty()) return false
        h.setIndex(put, drop)
        return true
    }

    private fun floatsLE(b: ByteArray): FloatArray {
        val fb = ByteBuffer.wrap(b).order(ByteOrder.LITTLE_ENDIAN).asFloatBuffer()
        return FloatArray(fb.remaining()).also { fb.get(it) }
    }
}

/**
 * The lines of handwriting on a page, from the strokes' boxes alone: strokes about as tall as
 * writing (not drawings, long rules or shapes) that lie at the same height and close together
 * form a line; a wide gap starts another one (two columns, a formula beside the text).
 */
object InkLines {
    class Box(val id: String, val x0: Float, val y0: Float, val x1: Float, val y1: Float) {
        val h get() = y1 - y0
        val cy get() = (y0 + y1) / 2
    }

    fun group(all: List<Box>): List<List<Box>> {
        if (all.isEmpty()) return emptyList()
        // How tall writing is on this page: the middle of the strokes' heights (dots aside).
        val hs = all.map { it.h }.filter { it > 2f }.sorted()
        val med = if (hs.isEmpty()) 10f else hs[hs.size / 2].coerceIn(4f, 60f)
        val text = all.filter { it.h <= med * 3.5f && (it.x1 - it.x0) <= med * 14f }
        class Line(var top: Float, var bottom: Float, var left: Float, var right: Float, val boxes: MutableList<Box>)
        val lines = ArrayList<Line>()
        // The letters first: strokes at the same height form a line.
        val (small, big) = text.partition { it.h < med * 0.45f }
        for (b in big.sortedBy { it.cy }) {
            val best = lines.maxByOrNull { l -> minOf(b.y1, l.bottom) - maxOf(b.y0, l.top) }
                ?.takeIf { l -> minOf(b.y1, l.bottom) - maxOf(b.y0, l.top) >= 0.4f * minOf(b.h, l.bottom - l.top) }
            if (best == null) lines += Line(b.y0, b.y1, b.x0, b.x1, mutableListOf(b))
            else { best.boxes += b; best.top = minOf(best.top, b.y0); best.bottom = maxOf(best.bottom, b.y1); best.left = minOf(best.left, b.x0); best.right = maxOf(best.right, b.x1) }
        }
        // Then dots, commas, dashes: with the line they sit on, over or under (a lone one is no writing).
        for (b in small) {
            lines.filter { l -> b.cy in (l.top - med * 0.7f)..(l.bottom + med * 0.7f) && b.x1 >= l.left - med && b.x0 <= l.right + med }
                .minByOrNull { l -> abs(b.cy - (l.top + l.bottom) / 2) }?.boxes?.add(b)
        }
        // Wide gaps split a line.
        val out = ArrayList<List<Box>>()
        for (l in lines) {
            var cur = ArrayList<Box>()
            var right = Float.NEGATIVE_INFINITY
            for (b in l.boxes.sortedBy { it.x0 }) {
                if (cur.isNotEmpty() && b.x0 - right > med * 3f) { out += cur; cur = ArrayList() }
                cur += b
                right = maxOf(right, b.x1)
            }
            if (cur.isNotEmpty()) out += cur
        }
        return out
    }

    /**
     * The word [id] belongs to: the strokes of its line of writing close enough together (a gap
     * of more than about half the writing's height parts words). A stroke that is no writing
     * (a drawing, a long line) is a word of its own.
     */
    fun word(all: List<Box>, id: String): List<String> {
        val line = group(all).firstOrNull { l -> l.any { it.id == id } } ?: return listOf(id)
        val hs = line.map { it.h }.filter { it > 2f }.sorted()
        val med = if (hs.isEmpty()) 10f else hs[hs.size / 2]
        val words = ArrayList<MutableList<Box>>()
        var right = Float.NEGATIVE_INFINITY
        for (b in line.sortedBy { it.x0 }) {
            if (words.isEmpty() || b.x0 - right > med * 0.55f) words += mutableListOf(b) else words.last() += b
            right = maxOf(right, b.x1)
        }
        return words.firstOrNull { w -> w.any { it.id == id } }?.map { it.id } ?: listOf(id)
    }

    /** A name for a line from its strokes (the same strokes: the same line, read once). */
    fun key(line: List<Box>): String {
        val d = java.security.MessageDigest.getInstance("SHA-1").digest(line.map { it.id }.sorted().joinToString(",").toByteArray())
        return d.take(8).joinToString("") { "%02x".format(it) }
    }
}
