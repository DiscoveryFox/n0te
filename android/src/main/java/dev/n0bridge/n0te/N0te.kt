package dev.n0bridge.n0te

import android.app.Application
import android.os.Handler
import android.os.Looper
import android.util.Log
import androidx.compose.ui.graphics.Color
import uniffi.n0te_ffi.NoteLibrary
import uniffi.n0te_ffi.PdfService
import java.io.File
import java.util.concurrent.ExecutorService
import java.util.concurrent.Executors

/** Process-wide state: the notebook library and the thread that does core I/O. */
class N0teApp : Application() {
    override fun onCreate() {
        super.onCreate()
        CrashLog.install(this)
        N0te.init(this)
    }
}

/** A copied picture (a screenshot of part of a page, or a copied image): its blob and size in points. */
class ClipImage(val blob: String, val w: Float, val h: Float)

/** Copied strokes (also between notebooks): data in sheet coordinates and their bounds; or a picture. */
class Clip(val data: List<uniffi.n0te_ffi.FfiStrokeData>, val bounds: android.graphics.RectF, val image: ClipImage? = null)

object N0te {
    /** The ink clipboard (copy / cut / paste). */
    var clip: Clip? = null
    const val TAG = "N0TE"

    lateinit var library: NoteLibrary
        private set
    lateinit var root: File
        private set
    /** PDF rendering, import and export (pdfium on its own worker thread); null if it failed to load. */
    var pdf: PdfService? = null
        private set

    /** One thread for everything that touches disk (saves, loads), so the UI never waits on it. */
    val io: ExecutorService = Executors.newSingleThreadExecutor { r -> Thread(r, "n0te-io").apply { priority = Thread.NORM_PRIORITY - 1 } }
    val main = Handler(Looper.getMainLooper())

    fun init(app: Application) {
        Settings.load(app)
        Themes.load(app)
        Ui.theme = Themes.byId(Settings.theme)
        Tools.load(app)
        Pencil.refresh()
        SyncHub.start(app)
        TexMath.init(app)
        root = File(app.filesDir, "library")
        library = NoteLibrary.open(root.absolutePath)
        Log.i(TAG, "library at $root")
        pdf = runCatching {
            PdfService.start(library, "${app.applicationInfo.nativeLibraryDir}/libpdfium.so", listOf("/system/fonts"), PdfTiles)
        }.onFailure { Log.e(TAG, "pdfium unavailable", it) }.getOrNull()
        Updater.start(app)
    }

    /** Runs [work] on the I/O thread and [done] with its result on the main thread. */
    fun <T> background(work: () -> T, done: (Result<T>) -> Unit) {
        io.execute {
            val r = runCatching(work)
            r.exceptionOrNull()?.let { Log.e(TAG, "background work failed", it) }
            main.post { done(r) }
        }
    }
}

/** Pen types: how width follows pressure, how opaque the ink is, and how it is drawn (kind). */
enum class PenType(val label: String, val hint: String, val widthMul: Float, val thinning: Float, val alpha: Int, val kind: UByte) {
    FOUNTAIN("Füller", "Strich folgt dem Druck", 1f, 0.65f, 0xFF, 0u),
    BALLPOINT("Kugelschreiber", "Gleichmäßiger Strich", 0.8f, 0.12f, 0xFF, 0u),
    PENCIL("Bleistift", "Graphit: weich am Rand, dunkler bei Druck", 1.25f, 0.2f, 0xF0, 1u),
    BRUSH("Pinsel", "Stark druckabhängig", 1.8f, 0.95f, 0xFF, 0u),
}

/** The user's pen presets (palettes, widths, pen type, recent colours), remembered across launches. */
object Tools {
    private const val PREFS = "tools"
    private val palettes: List<IntArray> = Palettes.defaults.map { it.copyOf() }
    var palette = 0
        private set
    /** The colours of the current palette (6). */
    val inks: IntArray get() = palettes[palette]
    val widths: FloatArray = Ui.widths.toFloatArray()
    var pen: PenType = PenType.FOUNTAIN
        private set
    val recent = ArrayList<Int>()
    private var prefs: android.content.SharedPreferences? = null

    fun load(ctx: android.content.Context) {
        val p = ctx.getSharedPreferences(PREFS, android.content.Context.MODE_PRIVATE).also { prefs = it }
        for ((pi, pal) in palettes.withIndex()) for (i in pal.indices) pal[i] = p.getInt("p${pi}ink$i", p.getInt(if (pi == 0) "ink$i" else "-", pal[i]))
        for (i in widths.indices) widths[i] = p.getFloat("width$i", widths[i])
        pen = runCatching { PenType.valueOf(p.getString("pen", pen.name)!!) }.getOrDefault(PenType.FOUNTAIN)
        palette = p.getInt("palette", 0).coerceIn(0, palettes.size - 1)
        recent.clear()
        p.getString("recent", "")!!.split(',').mapNotNullTo(recent) { it.toLongOrNull()?.toInt() }
    }

    fun setInk(i: Int, color: Int) {
        val c = color or 0xFF000000.toInt()
        palettes[palette][i] = c
        recent.remove(c); recent.add(0, c)
        while (recent.size > 10) recent.removeAt(recent.size - 1)
        prefs?.edit()?.putInt("p${palette}ink$i", c)?.putString("recent", recent.joinToString(",") { (it.toLong() and 0xFFFFFFFFL).toString() })?.apply()
    }
    fun setWidth(i: Int, w: Float) { widths[i] = w.coerceIn(MIN_W, MAX_W); prefs?.edit()?.putFloat("width$i", widths[i])?.apply() }
    fun choosePen(t: PenType) { pen = t; prefs?.edit()?.putString("pen", t.name)?.apply() }

    // The fountain pen's options (Fountain.kt; the pen menu).
    val fountainSensitivity get() = prefs?.getFloat("fp_sensitivity", 0.65f) ?: 0.65f
    val fountainSpeed get() = prefs?.getFloat("fp_speed", 0.5f) ?: 0.5f
    val fountainNib get() = prefs?.getString("fp_nib", "round") ?: "round"
    val fountainTaper get() = prefs?.getBoolean("fp_taper", true) ?: true
    fun setFountain(sensitivity: Float? = null, speed: Float? = null, nib: String? = null, taper: Boolean? = null) {
        prefs?.edit()?.apply {
            sensitivity?.let { putFloat("fp_sensitivity", it) }
            speed?.let { putFloat("fp_speed", it) }
            nib?.let { putString("fp_nib", it) }
            taper?.let { putBoolean("fp_taper", it) }
        }?.apply()
    }

    /** The tool, colour slot and width slot in use, kept for the next time a notebook opens. */
    val savedTool get() = prefs?.getString("tool", null)
    val savedColor get() = (prefs?.getInt("color_idx", 0) ?: 0).coerceIn(0, inks.size - 1)
    val savedWidth get() = (prefs?.getInt("width_idx", 1) ?: 1).coerceIn(0, widths.size - 1)
    private var lastSaved = ""
    fun saveSelection(tool: String, color: Int, width: Int) {
        val key = "$tool/$color/$width"
        if (key == lastSaved) return
        lastSaved = key
        prefs?.edit()?.putString("tool", tool)?.putInt("color_idx", color)?.putInt("width_idx", width)?.apply()
    }
    fun choosePalette(i: Int) { palette = ((i % palettes.size) + palettes.size) % palettes.size; prefs?.edit()?.putInt("palette", palette)?.apply() }
    fun resetPalette() { Palettes.defaults[palette].copyInto(palettes[palette]); prefs?.edit()?.apply { for (i in 0 until 6) remove("p${palette}ink$i") }?.apply() }

    const val MIN_W = 0.4f
    const val MAX_W = 14f
}

/** Nothing look: black chrome, white paper, one red accent. All colours live here. */
object Ui {
    /** The current theme (settings); activities re-create when it changes. */
    var theme: Theme = Themes.fallback
    /** Background. */
    val Black: Color get() = Color(theme.bg)
    val Surface: Color get() = Color(theme.surface)
    val Elevated: Color get() = Color(theme.elevated)
    val Border: Color get() = Color(theme.border)
    /** Foreground (text, icons). */
    val Chalk: Color get() = Color(theme.fg)
    val Muted: Color get() = Color(theme.muted)
    /** The accent. */
    val Red: Color get() = Color(theme.accent)
    /** Text/icons on the accent. */
    val OnAccent: Color get() = Color(theme.onAccent)
    val Paper: Color get() = Color(theme.paper)
    /** Floating bars and menus: opaque, so nothing on the page shows through them. */
    val Bar: Color get() = Color(theme.surface)
    /** Small chips on the page (zoom, page number). */
    val Chip: Color get() = Color(theme.surface).copy(alpha = 0.92f)

    /** Pen colours (0xAARRGGBB) offered in the toolbar. */
    val inks = listOf(0xFF111111.toInt(), 0xFF1E5BD8.toInt(), 0xFFD71921.toInt(), 0xFF138A47.toInt(), 0xFFE08A00.toInt(), 0xFF7B3FD0.toInt())
    /** Pen widths in page points. */
    val widths = listOf(1.2f, 2.0f, 3.2f, 5.0f)
    const val HIGHLIGHT_ALPHA = 0x66
}
