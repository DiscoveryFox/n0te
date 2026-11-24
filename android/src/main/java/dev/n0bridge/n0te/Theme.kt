package dev.n0bridge.n0te

import android.app.Activity
import androidx.core.view.WindowCompat

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

/** A design: colours for n0te's chrome (and the paper tint on screen; exports stay white) and its style. */
data class Theme(
    val id: String, val name: String,
    val bg: Long, val surface: Long, val elevated: Long, val border: Long,
    val fg: Long, val muted: Long, val accent: Long, val paper: Long, val dark: Boolean,
    val style: Style,
)

object Themes {
    val all = listOf(
        Theme("nothing", "Nothing", 0xFF000000, 0xFF141416, 0xFF1E1E22, 0xFF262629, 0xFFF2F2F4, 0xFF787880, 0xFFD71921, 0xFFFFFFFF, true,
            Style("Punktmatrix, Pillen, ein rotes Signal", 20f, 18f, 22f, LabelVoice.MONO_CAPS, HeadingFace.DOT, Fill.OUTLINE, false)),
        Theme("light", "Hell", 0xFFF1F1EF, 0xFFFFFFFF, 0xFFE8E8E5, 0xFFD6D6D1, 0xFF111111, 0xFF6E6E73, 0xFFD71921, 0xFFFFFFFF, false,
            Style("Weich und ruhig: Flächen statt Linien", 12f, 16f, 20f, LabelVoice.SANS, HeadingFace.SANS, Fill.TONAL, true)),
        Theme("oled", "OLED", 0xFF000000, 0xFF000000, 0xFF101010, 0xFF1E1E1E, 0xFFFFFFFF, 0xFF8A8A8A, 0xFFFF2D2D, 0xFFFFFFFF, true,
            Style("Technisch: Haarlinien, eckig, reines Schwarz", 0f, 0f, 0f, LabelVoice.MONO_CAPS, HeadingFace.MONO, Fill.RULE, false)),
        Theme("ocean", "Ozean", 0xFF080D17, 0xFF0F1626, 0xFF182235, 0xFF233048, 0xFFE8EEF8, 0xFF7C8AA3, 0xFF3B82F6, 0xFFFFFFFF, true,
            Style("Kühl und rund: getönte Pillen", 22f, 20f, 24f, LabelVoice.SANS, HeadingFace.SANS, Fill.TONAL, true)),
        Theme("forest", "Wald", 0xFF0A100C, 0xFF121A15, 0xFF1A241E, 0xFF25312A, 0xFFEAF2EC, 0xFF7F9486, 0xFF22C55E, 0xFFFFFFFF, true,
            Style("Ruhig und organisch: Serifen-Titel, viel Luft", 14f, 22f, 26f, LabelVoice.SANS, HeadingFace.SERIF, Fill.TONAL, false)),
        Theme("sepia", "Sepia", 0xFFEDE3D1, 0xFFF7F0E3, 0xFFE6D9C2, 0xFFD5C4A6, 0xFF2B2118, 0xFF7A6A55, 0xFFB45309, 0xFFFBF6EA, false,
            Style("Papier und Druck: Serifen, feine Rahmen", 4f, 4f, 8f, LabelVoice.SERIF, HeadingFace.SERIF, Fill.OUTLINE, false)),
    )

    fun byId(id: String) = all.firstOrNull { it.id == id } ?: all[0]

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
