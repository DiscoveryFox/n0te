package dev.n0bridge.n0te

import android.app.Activity
import android.content.Context
import android.util.Log
import androidx.core.view.WindowCompat
import uniffi.n0te_ffi.FfiDesign
import java.io.File

/** The voice of button labels. */
enum class LabelVoice { MONO_CAPS, SANS, SERIF }

/** The face of headings (screen and dialog titles). */
enum class HeadingFace { DOT, MONO, SANS, SERIF }

/** How secondary buttons, cards and dialogs are drawn. */
enum class Fill {
    /** A 1 dp outline (Nothing, Sepia). */
    OUTLINE,
    /** A tinted surface without outline (Hell, Ozean, Wald). */
    TONAL,
    /** Hairlines and square corners (OLED). */
    RULE,
}

/** A design's shapes and voice (the same as on the laptop). Radii in dp. */
data class Style(
    val desc: String,
    val button: Float, val card: Float, val panel: Float,
    val label: LabelVoice, val heading: HeadingFace, val fill: Fill,
    /** Soft shadows under cards and dialogs. */
    val lift: Boolean,
)

/** Something drawn behind the library (snow, fairy lights, stars, …); colours 0xAARRGGBB. */
data class Decor(val kind: String, val colors: List<Long>, val amount: Float, val animated: Boolean)

/** A design: colours for n0te's chrome (and the paper tint on screen; exports stay white) and its style. */
data class Theme(
    val id: String, val name: String,
    val bg: Long, val surface: Long, val elevated: Long, val border: Long,
    val fg: Long, val muted: Long, val accent: Long, val paper: Long, val dark: Boolean,
    val style: Style,
    /** Text and icons on the accent. */
    val onAccent: Long = 0xFFFFFFFF,
    val decor: List<Decor> = emptyList(),
    /** Ships with n0te (imported designs can be shared and deleted). */
    val builtin: Boolean = true,
    val author: String = "",
    /** The design as a file (to share it). */
    val json: String = "",
)

/**
 * The designs: the built-in ones come from the Rust core (the JSON files in `crates/core/designs`, the
 * same files as the laptop's), the imported ones from `files/designs`. People write their own
 * with an AI (DesignImport.kt) and import them as a small JSON file.
 */
object Themes {
    /** Used until the core is loaded, and if it can't be: Nothing. */
    val fallback = Theme("nothing", "Nothing", 0xFF000000, 0xFF141416, 0xFF1E1E22, 0xFF262629, 0xFFF2F2F4, 0xFF787880, 0xFFD71921, 0xFFFFFFFF, true,
        Style("Punktmatrix, Pillen, ein rotes Signal", 20f, 18f, 22f, LabelVoice.MONO_CAPS, HeadingFace.DOT, Fill.OUTLINE, false))

    var builtins: List<Theme> = listOf(fallback)
        private set
    var imported: List<Theme> = emptyList()
        private set
    /** How many of [builtins] are the classics; the rest are the special ones. */
    var classics = 1
        private set
    val all: List<Theme> get() = builtins + imported
    private var dir: File? = null

    /** Reads the built-in and the imported designs (at start, and after an import or delete). */
    fun load(ctx: Context) {
        dir = File(ctx.filesDir, "designs")
        runCatching {
            builtins = uniffi.n0te_ffi.designBuiltins().map(::fromCore)
            classics = uniffi.n0te_ffi.designClassics().toInt()
        }.onFailure { Log.e(N0te.TAG, "built-in designs unavailable", it) }
        reloadImported()
    }

    fun reloadImported() {
        val d = dir ?: return
        imported = runCatching { uniffi.n0te_ffi.designImported(d.absolutePath).map(::fromCore) }
            .onFailure { Log.e(N0te.TAG, "imported designs unreadable", it) }.getOrDefault(emptyList())
    }

    /** Where imported designs live. */
    val folder: String get() = dir?.absolutePath ?: ""

    fun byId(id: String) = all.firstOrNull { it.id == id } ?: builtins.first()

    private fun argb(c: UInt): Long = 0xFF000000L or c.toLong()

    fun fromCore(d: FfiDesign) = Theme(
        id = d.id, name = d.name,
        bg = argb(d.background), surface = argb(d.surface), elevated = argb(d.surface2), border = argb(d.border),
        fg = argb(d.text), muted = argb(d.muted), accent = argb(d.accent), paper = argb(d.paper), dark = d.dark,
        style = Style(d.desc, d.radiusButton, d.radiusCard, d.radiusPanel,
            when (d.label) { "mono_caps" -> LabelVoice.MONO_CAPS; "serif" -> LabelVoice.SERIF; else -> LabelVoice.SANS },
            when (d.heading) { "dot" -> HeadingFace.DOT; "mono" -> HeadingFace.MONO; "serif" -> HeadingFace.SERIF; else -> HeadingFace.SANS },
            when (d.fill) { "outline" -> Fill.OUTLINE; "rule" -> Fill.RULE; else -> Fill.TONAL },
            d.shadows),
        onAccent = argb(d.onAccent),
        decor = d.decor.map { x -> Decor(x.kind, x.colors.map(::argb), x.amount, x.animated) },
        builtin = d.builtin, author = d.author, json = d.json,
    )

    /** The system bars in the design's background (they stayed black, and dark icons vanished on light designs). */
    @Suppress("DEPRECATION")
    fun applyWindow(a: Activity) {
        a.window.decorView.setBackgroundColor(Ui.theme.bg.toInt())
        a.window.statusBarColor = Ui.theme.bg.toInt()
        a.window.navigationBarColor = Ui.theme.bg.toInt()
        WindowCompat.getInsetsController(a.window, a.window.decorView).apply {
            isAppearanceLightStatusBars = !Ui.theme.dark
            isAppearanceLightNavigationBars = !Ui.theme.dark
        }
    }
}
