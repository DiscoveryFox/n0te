package dev.n0bridge.n0te

import android.content.Context
import android.content.SharedPreferences
import uniffi.n0te_ffi.FfiBackground

/**
 * Everything the user can switch on or off (SETTINGS screen). Read anywhere; written by the
 * settings screen. Defaults are what n0te does out of the box.
 */
object Settings {
    private var p: SharedPreferences? = null

    fun load(ctx: Context) {
        p = ctx.getSharedPreferences("settings", Context.MODE_PRIVATE)
    }

    private fun b(key: String, def: Boolean) = p?.getBoolean(key, def) ?: def
    private fun f(key: String, def: Float) = p?.getFloat(key, def) ?: def
    fun str(key: String, def: String) = p?.getString(key, def) ?: def
    /** How long a "+" beside a page must be held before it adds a page (0: a tap is enough). */
    val handleHoldMs get() = f("handle_hold_ms", 500f)
    /** Laser trail: "short" (fades right behind the pen), "long" (seconds), "keep" (all of it while drawing). */
    val laserTrail get() = str("laser_trail", "short")
    /** With the page lock: a swipe inside a zoomed-in page keeps gliding (bounded by the page). */
    val lockMomentum get() = b("lock_momentum", true)
    /** A soft band behind the main column, so side pages read as beside it. */
    val mainColumn get() = b("main_column", true)
    /** Zoomed in while writing: the page follows the pen and wraps to the next line at its edge. */
    val writeFlow get() = b("write_flow", false)
    /** The selection tool's mode: "free" (lasso), "rect" (box) or "snip" (screenshot of a part of the page). */
    val selectMode get() = str("select_mode", "free")
    /** This tablet's name for the laptop and for guests (applies when the app starts). */
    val displayName get() = str("display_name", "Tablet").ifBlank { "Tablet" }
    /** Show where guests are looking (their frames, named). */
    val guestFrames get() = b("guest_frames", true)
    /** Guests see where this tablet is looking. */
    val shareView get() = b("share_view", true)
    /** PDF export: side pages all at the end (else right after their page); the export dialog's last choice. */
    val exportSidesAtEnd get() = b("export_sides_at_end", false)
    fun set(key: String, v: Boolean) { p?.edit()?.putBoolean(key, v)?.apply() }
    fun set(key: String, v: Float) { p?.edit()?.putFloat(key, v)?.apply() }
    fun set(key: String, v: String) { p?.edit()?.putString(key, v)?.apply() }

    // Pages
    val sidePages get() = b("side_pages", true)
    val handles get() = b("handles", true)
    val boards get() = b("boards", true)
    val groups get() = b("groups", true)
    // Input
    val penOnlyDefault get() = b("pen_only", true)
    val palmMm get() = f("palm_mm", 30f)
    val doubleTapFit get() = b("double_tap", true)
    val shapes get() = b("shapes", true)
    /** Handwriting is read (ML Kit, on the tablet) so it can be searched. */
    val handwritingSearch get() = b("handwriting_search", true)
    /** Arrows (a head held at a line's end, or an arrow drawn in one stroke and held), with shapes on. */
    val arrows get() = b("arrows", true)
    val sideButtonErases get() = b("side_button_erases", true)
    val scribbleErase get() = b("scribble_erase", true)
    /** A line that is only a $$…$$ formula is centred. */
    val formulaCenter get() = b("formula_center", true)
    /** Live: the other device's view as a red frame (its laser shows either way). */
    val remoteFrame get() = b("remote_frame", true)
    // Eraser: "stroke" (whole strokes) or "area" (only what it touches); size (pt across);
    // automatic = as wide as the pen, growing with speed.
    val eraserArea get() = p?.getString("eraser_mode", "stroke") == "area"
    val eraserSize get() = f("eraser_size", 12f)
    val eraserAuto get() = b("eraser_auto", false)
    // View
    val pageNumbers get() = b("page_numbers", true)
    val minimap get() = b("minimap", true)
    val showZoom get() = b("show_zoom", true)
    /** How big a page is shown when opening and for new pages: "width", "page" or a zoom in %. */
    val pageScale get() = p?.getString("page_scale", "width") ?: "width"
    val hoverCursor get() = b("hover_cursor", true)
    // Toolbar
    val toolMarker get() = b("tool_marker", true)
    val toolLasso get() = b("tool_lasso", true)
    val toolRuler get() = b("tool_ruler", true)
    /** The writing magnifier in the toolbar; its frame width as a share of the page width. */
    val toolLoupe get() = b("tool_loupe", true)
    val loupeShare get() = f("loupe_share", 0.34f)
    val loupeHintSeen get() = b("loupe_hint_seen2", false)
    /** The magnifier strip's top (share of the screen height; -1: at the bottom) and its height. */
    val loupePadTop get() = f("loupe_pad_top", -1f)
    val loupePadH get() = f("loupe_pad_h", 0.3f)
    // Look
    val theme get() = p?.getString("theme", "nothing") ?: "nothing"
    // Layout
    val pageGapV get() = f("page_gap_v", 36f)
    val pageGapH get() = f("page_gap_h", 28f)
    /** Last lock mode used: "zoom" (zoom fixed, vertical only) or "page" (zoom free, stays on the page). */
    val lockMode get() = p?.getString("lock_mode", "zoom") ?: "zoom"
    val toolLaser get() = b("tool_laser", true)
    // Pencil look (see Pencil): grain 0 = smooth … 1 = rough; softness 0 = even … 1 = soft edge,
    // dark core under pressure; opacity of new pencil strokes.
    val pencilGrain get() = f("pencil_grain", 0.7f)
    val pencilSoftness get() = f("pencil_softness", 0.4f)
    val pencilOpacity get() = f("pencil_opacity", 0.85f)
    // Paper
    val defaultPaper: FfiBackground get() = runCatching { FfiBackground.valueOf(p?.getString("paper", "GRID") ?: "GRID") }.getOrDefault(FfiBackground.GRID)
    val lineSpacing get() = f("line_spacing", 22f)
    val gridSize get() = f("grid_size", 17f)
    val dotSpacing get() = f("dot_spacing", 20f)

    /** Page size (points) for new notebooks; older versions stored a format name. */
    val newSize: Pair<Float, Float> get() {
        val w = f("new_w", 0f); val h = f("new_h", 0f)
        if (w > 0f && h > 0f) return w to h
        return when (str("new_format", "a4")) { "a4q" -> 842f to 595f; "a5" -> 420f to 595f; "letter" -> 612f to 792f; else -> 595f to 842f }
    }
    fun setNewSize(w: Float, h: Float) { set("new_w", w); set("new_h", h) }

    fun spacingFor(bg: FfiBackground) = when (bg) {
        FfiBackground.GRID -> gridSize
        FfiBackground.DOTS -> dotSpacing
        else -> lineSpacing
    }
}

/** Named colour palettes; swipe across the colours in the toolbar to switch. */
object Palettes {
    val names = listOf("Standard", "Pastell", "Neon", "Erde", "Grau", "Textmarker")
    val defaults: List<IntArray> = listOf(
        intArrayOf(0xFF111111.toInt(), 0xFF1E5BD8.toInt(), 0xFFD71921.toInt(), 0xFF138A47.toInt(), 0xFFE08A00.toInt(), 0xFF7B3FD0.toInt()),
        intArrayOf(0xFF6B7FD7.toInt(), 0xFF8FC1E3.toInt(), 0xFFF2A2B1.toInt(), 0xFF9ED9A0.toInt(), 0xFFF6D186.toInt(), 0xFFC3A6E8.toInt()),
        intArrayOf(0xFF00E5FF.toInt(), 0xFFFF2D95.toInt(), 0xFF39FF14.toInt(), 0xFFFFEA00.toInt(), 0xFFFF6B00.toInt(), 0xFFB026FF.toInt()),
        intArrayOf(0xFF5B3A29.toInt(), 0xFF8C6239.toInt(), 0xFFA3B18A.toInt(), 0xFF588157.toInt(), 0xFFBC6C25.toInt(), 0xFF3A5A40.toInt()),
        intArrayOf(0xFF000000.toInt(), 0xFF333333.toInt(), 0xFF555555.toInt(), 0xFF777777.toInt(), 0xFF999999.toInt(), 0xFFBBBBBB.toInt()),
        intArrayOf(0xFFFFE600.toInt(), 0xFF7CFC00.toInt(), 0xFF00E1FF.toInt(), 0xFFFF6EC7.toInt(), 0xFFFFA500.toInt(), 0xFFB388FF.toInt()),
    )
}
