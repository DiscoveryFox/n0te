package dev.n0bridge.n0te

import android.graphics.Bitmap
import android.graphics.BitmapShader
import android.graphics.Canvas
import android.graphics.Matrix
import android.graphics.Paint
import android.graphics.Path
import android.graphics.Shader
import java.nio.ByteBuffer
import kotlin.math.exp
import kotlin.math.floor
import kotlin.math.ln
import kotlin.math.pow
import kotlin.math.sqrt

/**
 * Graphite look for pencil strokes. A stroke is three translucent fills (its outline, a middle
 * band and a pressure-hungry core from the core library), each through a grain texture:
 *  - the outer band through a ragged "paper tooth" texture (graphite only on the fibre peaks),
 *  - the middle band and the core through fine, nearly solid grain.
 * Light pressure leaves only the faint, ragged outer band; hard pressure a dark core.
 *
 * The grain is fixed to the paper but its size follows the zoom in powers of two, so it stays
 * one to two screen pixels: never blocky when zoomed in, never shimmering when zoomed out.
 */
object Pencil {
    private const val N = 256
    /** Pass opacity: outer band, middle band, core (from the softness setting). */
    private val passAlpha = floatArrayOf(1f, 0f, 0f)
    private var grain = -1f
    private var softness = -1f

    /** Bumps when the look changed (pages with pencil ink must be re-recorded). */
    @Volatile var generation = 0
        private set
    /** Texture offsets (texels) per pass, so the grains don't line up. */
    private val OFFSET = arrayOf(floatArrayOf(0f, 0f), floatArrayOf(29f, 53f), floatArrayOf(71f, 17f))

    /** Raw grain (0..1, 1 = graphite) for the outer band and for the inner bands. */
    private val raw: Array<FloatArray> by lazy { makeRaw() }
    @Volatile private var textures: Array<Bitmap>? = null
    private val paintCache = HashMap<Float, Array<Paint>>()

    /** Reads the look from [Settings] (grain, softness); true when it changed. */
    fun refresh(): Boolean {
        val g = Settings.pencilGrain; val s = Settings.pencilSoftness
        if (g == grain && s == softness && textures != null) return false
        softness = s
        passAlpha[0] = 1f - 0.4f * s; passAlpha[1] = 0.35f * s; passAlpha[2] = 0.6f * s
        if (g != grain || textures == null) {
            grain = g
            // Grain 0: solid; 1: the full texture.
            textures = Array(2) { t -> alpha8(ByteArray(N * N) { i -> alpha(1f - g * (1f - raw[t][i])) }) }
        }
        paintCache.clear()
        generation++
        return true
    }

    private fun tex(i: Int): Bitmap = (textures ?: run { refresh(); textures!! })[i]

    /** World units per texel for a zoom of [scale] screen pixels per point (a power of two). */
    fun texelFor(scale: Float): Float {
        val ideal = 2f / scale.coerceAtLeast(0.01f) // at most 2 px per texel
        return 2f.pow(floor(ln(ideal) / ln(2f))).coerceIn(1f / 64f, 4f)
    }

    /** Paints for committed strokes (canvas in sheet coordinates) at grain size [texel]. */
    fun paints(texel: Float): Array<Paint> = paintCache.getOrPut(texel) { Array(3) { i -> makePaint(i, Matrix().apply { setScale(texel, texel) }) } }

    private var liveKey: Pair<Float, List<Float>>? = null
    private var livePaints: Array<Paint>? = null

    /** Paints for the live stroke on screen: the same grain, mapped through [sheetToScreen]. */
    fun livePaints(texel: Float, sheetToScreen: Matrix): Array<Paint> {
        val v = FloatArray(9).also { sheetToScreen.getValues(it) }.toList() + generation.toFloat()
        val key = texel to v
        livePaints?.let { if (liveKey == key) return it }
        val m = Matrix().apply { setScale(texel, texel); postConcat(sheetToScreen) }
        return Array(3) { i -> makePaint(i, m) }.also { livePaints = it; liveKey = key }
    }

    private fun makePaint(pass: Int, base: Matrix) = Paint(Paint.ANTI_ALIAS_FLAG or Paint.FILTER_BITMAP_FLAG).apply {
        style = Paint.Style.FILL
        shader = BitmapShader(tex(if (pass == 0) 0 else 1), Shader.TileMode.REPEAT, Shader.TileMode.REPEAT).apply {
            setLocalMatrix(Matrix(base).apply { preTranslate(OFFSET[pass][0], OFFSET[pass][1]) })
        }
    }

    /** Draws a pencil stroke: [outer] plus [layers] (middle band, core), in [color]. */
    fun draw(c: Canvas, paints: Array<Paint>, outer: Path, layers: List<Path>, color: Int) {
        val a = (color ushr 24) / 255f
        val rgb = color and 0x00FFFFFF
        fun paint(i: Int) = paints[i].apply { this.color = rgb or ((a * passAlpha[i] * 255f).toInt().coerceIn(0, 255) shl 24) }
        c.drawPath(outer, paint(0))
        layers.forEachIndexed { i, p -> if (i < 2 && passAlpha[i + 1] > 0.01f) c.drawPath(p, paint(i + 1)) }
    }

    // --- textures ------------------------------------------------------------------------------

    private fun makeRaw(): Array<FloatArray> {
        val rnd = java.util.Random(0x6e30L)
        val fine = normalized(blur(FloatArray(N * N) { rnd.nextFloat() }, 1, 0.6f))
        val tooth = normalized(blur(FloatArray(N * N) { rnd.nextFloat() }, 1, 1.0f))
        // Outer band: fine, continuous paper tooth with faint fibre streaks; deep valleys stay
        // light. (A coarser, two-level tooth looked like camouflage when zoomed in.)
        val edge = FloatArray(N * N) { i ->
            val x = i % N; val y = i / N
            val streak = kotlin.math.sin(x * 0.9f + kotlin.math.sin(y * 0.23f) * 3f + y * 0.05f)
            ((0.80f + 0.22f * tooth[i] + 0.06f * streak) * (if (tooth[i] < -1.3f) 0.35f else 1f)).coerceIn(0f, 1f)
        }
        // Middle band and core: fine grain, a few deep valleys.
        val solid = FloatArray(N * N) { i -> ((0.90f + 0.10f * fine[i]) * (if (tooth[i] < -1.8f) 0.55f else 1f)).coerceIn(0f, 1f) }
        return arrayOf(edge, solid)
    }

    private fun alpha(v: Float) = (v.coerceIn(0f, 1f) * 255f).toInt().toByte()

    private fun alpha8(bytes: ByteArray) = Bitmap.createBitmap(N, N, Bitmap.Config.ALPHA_8).apply { copyPixelsFromBuffer(ByteBuffer.wrap(bytes)) }

    /** Gaussian blur that wraps around (the texture tiles seamlessly). */
    private fun blur(src: FloatArray, rad: Int, sigma: Float): FloatArray {
        val k = FloatArray((2 * rad + 1) * (2 * rad + 1))
        var wsum = 0f
        for (dy in -rad..rad) for (dx in -rad..rad) { val w = exp(-(dx * dx + dy * dy) / sigma); k[(dy + rad) * (2 * rad + 1) + dx + rad] = w; wsum += w }
        val out = FloatArray(N * N)
        for (y in 0 until N) for (x in 0 until N) {
            var s = 0f
            for (dy in -rad..rad) for (dx in -rad..rad) s += src[((y + dy + N) % N) * N + (x + dx + N) % N] * k[(dy + rad) * (2 * rad + 1) + dx + rad]
            out[y * N + x] = s / wsum
        }
        return out
    }

    private fun normalized(a: FloatArray): FloatArray {
        val mean = a.average().toFloat()
        val sd = sqrt(a.fold(0f) { acc, v -> acc + (v - mean) * (v - mean) } / a.size).coerceAtLeast(1e-6f)
        return FloatArray(a.size) { (a[it] - mean) / sd }
    }
}
