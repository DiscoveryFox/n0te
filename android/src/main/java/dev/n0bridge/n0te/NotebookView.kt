package dev.n0bridge.n0te

import android.annotation.SuppressLint
import android.content.Context
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.DashPathEffect
import android.graphics.Matrix
import android.graphics.Paint
import android.graphics.Path
import android.graphics.PointF
import android.graphics.RectF
import android.graphics.RenderNode
import android.graphics.Typeface
import android.os.SystemClock
import android.view.MotionEvent
import android.view.VelocityTracker
import android.view.View
import android.view.ViewConfiguration
import android.widget.OverScroller
import uniffi.n0te_ffi.FfiBackground
import uniffi.n0te_ffi.FfiBrush
import uniffi.n0te_ffi.strokeOutline
import uniffi.n0te_ffi.strokeLayers
import kotlin.math.abs
import kotlin.math.hypot
import kotlin.math.max
import kotlin.math.min

/**
 * The notebook's pages: rendering and all pen/finger input.
 *
 * World coordinates are points; screen = world × [scale] + ([tx], [ty]). Each sheet's ink is one
 * RenderNode (a display list of filled outline paths) recorded once and replayed at any zoom.
 * The stroke being drawn goes to the low-latency [InkOverlay].
 *
 * Input (palm handling as in the n0bridge sketch canvas): the pen always draws (side button =
 * eraser). Pen-only mode: one finger moves, two fingers zoom; fingers never draw. Finger mode:
 * one finger draws, two fingers move and zoom. Fingers are ignored while the pen hovers or
 * touches (and 300 ms after), and when their contact is larger than [PALM_MM].
 */
class NotebookView(context: Context) : View(context) {

    enum class Tool { PEN, MARKER, ERASER, LASSO, LASER, TEXT }

    /** NONE; ZOOM: zoom fixed, only up/down; PAGE: zoom free, sideways stays on the page. In both a sideways pull moves to the next side page. */
    enum class Lock { NONE, ZOOM, PAGE }

    interface Listener {
        fun onViewState()
        fun onSelectionChanged(sel: Selection?)
        /** A "+" handle next to a page was used: -1 left, 1 right, 2 below [mainNode]'s row. */
        fun onAddPage(direction: Int, mainNode: String)
        /** A group title in the page column was tapped (not its fold button): open its menu at (x, y). */
        fun onGroupMenu(h: Header, x: Float, y: Float)
        /** A short message for the user ("3 Striche gelöscht"). */
        fun onMessage(text: String)
        /** A screenshot of part of a page is on the clipboard. */
        fun onSnipped() {}
        /** Text tool: a tap on [p] at (x, y) sheet points, on text box [boxId] (null: on empty paper). */
        fun onTextTap(p: Placed, boxId: String?, x: Float, y: Float)
        /** A finger held still on the page (fingers only navigate): a menu at screen (x, y). */
        fun onFingerHold(x: Float, y: Float) {}
        /** Following another view ended because the page was moved by hand. */
        fun onFollowStopped() {}
        /** The writing magnifier closed by itself (a board opened). */
        fun onLoupeClosed() {}
        /** The magnifier's strip moved or changed height (its controls follow). */
        fun onLoupePadMoved() {}
    }

    /** Where live strokes and this view's position go (the activity sends them to the laptop). */
    interface LiveSink {
        fun samples(sheetId: String, stroke: Long, seq: Int, brush: FfiBrush, samples: ByteArray, done: Boolean)
        /** Erasing as it happens: strokes just hidden, area-eraser dabs (x, y, r triples). */
        fun erase(sheetId: String, ids: List<String>, dabs: FloatArray)
        fun presence(sheetId: String?, rect: FloatArray, zoom: Float, laser: FloatArray?)
    }

    /** Another device's view (sheet points) and laser pointer. */
    /** Another device's view; [own]: one of my devices (its frame follows "Rahmen des Laptops"). */
    class RemoteView(val name: String, val sheetId: String, val rect: RectF, val laser: PointF?, val own: Boolean = true)

    /** A stroke another device is drawing (shown until the stored one arrives). */
    class RemoteInk(val sheetId: String, val brush: FfiBrush) {
        var samples = FloatArray(0)
        var path = Path()
        var doneAt = 0L
        /** When its last samples came (a stroke whose end never comes goes after a while). */
        var lastAt = 0L
    }

    /** A "+" next to the current page row (world position). */
    class Handle(val dir: Int, val wx: Float, val wy: Float, val mainNode: String, val ghost: RectF)

    /**
     * What the lasso caught on one sheet ([ids], their [bounds] in its points); a lasso across
     * pages adds the other pages' parts in [more]. Moving and scaling act on all parts together.
     */
    class Selection(val sheetId: String, val ids: Set<String>, val bounds: RectF, val more: List<Selection> = emptyList()) {
        val parts: List<Selection> get() = listOf(this) + more
        val multi: Boolean get() = more.isNotEmpty()
        val count: Int get() = parts.sumOf { it.ids.size }
    }

    var model: NotebookModel? = null
        set(v) { field = v; invalidate() }
    var live: LiveSink? = null
    val remotes = HashMap<String, RemoteView>()
    val remoteInk = HashMap<String, RemoteInk>()
    /** Follow this device's view (live session). */
    var followDevice: String? = null
    private var liveStrokeId = 0L
    private var liveSent = 0
    private var liveSeq = 0
    private var liveSentAt = 0L
    private var presenceAt = 0L
    /** Shape correction: the stroke snapped to a shape while the pen was held still. */
    private var snapped = false
    private val holdCheck = Runnable { trySnap(arrows = false) }
    /** Arrows want a longer hold than shapes (writing pauses for half a second, not for a whole one). */
    private val arrowCheck = Runnable { trySnap(arrows = true) }
    /** A line snapped while the pen is still down: its start (page points); moving the pen moves its end. */
    private var lineHeld = false
    private var lineAX = 0f
    private var lineAY = 0f
    private var linePressure = 0.5f
    /** The angle the held line snapped to last (degrees), for a tick when it snaps to another. */
    private var lineSnapDeg: Int? = null
    /**
     * Space going into (or out of) a page while the pen drags: the line at [y] (page points), what
     * lies below it, and the current plan (how far that moves, the page's height and gaps after).
     */
    private class SpaceDrag(val page: Placed, val y: Float, val downY: Float, val below: uniffi.n0te_ffi.FfiBelow, val step: Float,
                            val x: Float, val downX: Float, val right: uniffi.n0te_ffi.FfiBelow, val stepX: Float) {
        /** Which way, from where the pen first goes: null until it has moved a little, then down (false) or sideways (true). */
        var sideways: Boolean? = null
        var plan: uniffi.n0te_ffi.FfiSpacePlan? = null
        var planX: uniffi.n0te_ffi.FfiSpacePlanX? = null
        /** The space asked for (before the plan limits it). */
        var asked = 0f
        val dy get() = if (sideways == false) plan?.dy ?: 0f else 0f
        val dx get() = if (sideways == true) planX?.dx ?: 0f else 0f
        /** How much longer the page gets (shorter when negative). */
        val grow get() = if (sideways == false) (plan?.height ?: page.spec.height) - page.spec.height else 0f
        /** How much wider the page gets. */
        val widen get() = if (sideways == true) (planX?.width ?: page.spec.width) - page.spec.width else 0f
        /**
         * Where the page's left edge goes while it widens (the layout keeps the main page in the
         * middle, a left side page against it, a right side page where it starts).
         */
        val ownShift get() = when { page.side == 0 -> -widen / 2f; page.side < 0 -> -widen; else -> 0f }
        /** How far another page of the same row moves sideways while this one widens. */
        fun rowShift(o: Placed): Float {
            if (sideways != true || widen == 0f || o.mainNode != page.mainNode || o.sheetId == page.sheetId) return 0f
            return when {
                page.side == 0 -> if (o.side < 0) -widen / 2f else widen / 2f
                page.side < 0 -> if (o.side < page.side) -widen else 0f
                else -> if (o.side > page.side) widen else 0f
            }
        }
    }
    private var space: SpaceDrag? = null
    private val spaceFill = Paint().apply { style = Paint.Style.FILL }
    private val spaceLine = Paint(Paint.ANTI_ALIAS_FLAG).apply { style = Paint.Style.STROKE }
    private val spaceText = Paint(Paint.ANTI_ALIAS_FLAG).apply { typeface = Typeface.MONOSPACE }

    /** A closed shape snapped while the pen is still down: dragging the pen changes its size. */
    private var shapeHeld: Shapes.HeldShape? = null
    /** Where the pen was when the held shape snapped (page points), and whether it has moved off since. */
    private var shapeHeldX = 0f
    private var shapeHeldY = 0f
    private var shapeDragging = false
    /** The held shape is a square / circle right now (a tick when it becomes one). */
    private var shapeSquare = false
    /** No live samples to the laptop while a held line or shape changes (it gets the finished one). */
    private var liveHold = false
    /** Wiggled off: the held line follows the pen at any angle (wiggling again: snapping back on). */
    private var lineFree = false
    /** Wiggle detection: the pen across the line (screen px) — its last extremes, the turns' times. */
    private var lineNx = 0f
    private var lineNy = 0f
    private var wiggleDir = 0
    private var wiggleHi = 0f
    private var wiggleLo = 0f
    private val wiggleTurns = ArrayDeque<Long>()
    private var wiggleQuietUntil = 0L
    // Live erasing not yet sent (batched, ≤ 20×/s).
    private val eraseIds = ArrayList<String>()
    private val eraseDabs = ArrayList<Float>()
    private var eraseSentAt = 0L
    private var eraseSendSheet: String? = null
    private val remotePaint = Paint(Paint.ANTI_ALIAS_FLAG).apply { color = 0xCCD71921.toInt(); style = Paint.Style.STROKE }
    private val remoteLabel = Paint(Paint.ANTI_ALIAS_FLAG).apply { color = 0xFFD71921.toInt(); typeface = Typeface.MONOSPACE; isFakeBoldText = true }
    private val remoteLaser = Paint(Paint.ANTI_ALIAS_FLAG).apply { color = 0xE6FF3B30.toInt() }
    private val remoteInkPaint = Paint(Paint.ANTI_ALIAS_FLAG).apply { style = Paint.Style.FILL }
    var inkLayer: InkOverlay? = null
    var listener: Listener? = null
    /** This view's PDF zoom level and requests, and the page pictures' (see [PdfTiles.Slot]). */
    private val pdfSlot = PdfTiles.Slot()
    private val thumbSlot = PdfTiles.Slot()

    var tool = Tool.PEN
        set(v) { field = v; if (v != Tool.LASSO) clearSelection(); invalidate() }
    var penOnly = true
    var colorIdx = 0
    var widthIdx = 1

    var scale = 1f; private set
    var tx = 0f; private set
    var ty = 0f; private set
    private var fitted = false

    var selection: Selection? = null; private set

    /** Full-screen board (infinite canvas): world = the board's own coordinates, unbounded. */
    var board: Placed? = null; private set
    private var columnTransform = floatArrayOf(1f, 0f, 0f)

    /** Ruler: centre (world), angle (radians). Pen strokes starting at an edge follow it. */
    var rulerOn = false
        set(v) { field = v; if (v) placeRuler(); invalidate() }
    private var rulerCx = 0f
    private var rulerCy = 0f
    private var rulerAngle = 0f
    private var snapEdge = 0 // -1 top edge, 1 bottom edge, 0 none (for the running stroke)

    // --- input state -------------------------------------------------------------------------
    private class Finger(val id: Int, val downX: Float, val downY: Float, var x: Float, var y: Float, var rejected: Boolean, val downAt: Long)

    private val fingers = LinkedHashMap<Int, Finger>()
    private var penId = -1
    private var penUpAt = 0L
    private var hovering = false
    private var hoverExitAt = 0L
    private var hoverX = Float.NaN
    private var hoverY = Float.NaN
    private var hoverErase = false

    private enum class Gesture { NONE, PAN_PENDING, PAN, PINCH, RULER }

    private var gesture = Gesture.NONE
    private var panId = -1
    private var lastPanX = 0f
    private var lastPanY = 0f
    private var pinchA = -1
    private var pinchB = -1
    private var lastPinchDist = 0f
    private var lastPinchCx = 0f
    private var lastPinchCy = 0f
    private var afterPinch = false
    private var rulerA = -1
    private var rulerB = -1
    private var lastRulerAngle = 0f
    private var lastRulerX = 0f
    private var lastRulerY = 0f
    private var velocity: VelocityTracker? = null
    /**
     * Fastest fling (px/s), like Android's lists. Without it a burst of touch events (the app
     * busy for a moment) reads as a huge speed and the page shoots off.
     */
    private val maxFling = android.view.ViewConfiguration.get(context).scaledMaximumFlingVelocity.toFloat()
    private val scroller = OverScroller(context)
    private var lastFlingX = 0
    private var lastFlingY = 0

    /** What the active pointer (pen, or the drawing finger) is doing. */
    private enum class Action { NONE, PRE_INK, INK, ERASE, LASSO, MOVE_SEL, SCALE_SEL, END_SEL, GRIP_SEL, TAP, HANDLE, LASER, TEXT, SPACE, PLOT_BOX }

    /** The text box being edited (sheet id, box id): the inline editor shows it, the page doesn't. */
    var editingText: Pair<String, String>? = null
        set(v) {
            val old = field
            field = v
            old?.let { model?.sheets?.get(it.first)?.invalidateNode() }
            v?.let { model?.sheets?.get(it.first)?.invalidateNode() }
            invalidate()
        }
    /** Drags handed over by the inline text editor: scroll / zoom only, never draw. */
    var navigateOnly = false
    /** A guest who may only read: the pen pans, nothing changes the pages. */
    var readOnly = false
    /** Text tool: the box under the pen at its down point, where that was, whether it moved. */
    private var textHit: uniffi.n0te_ffi.FfiTextBox? = null
    private var textDownLx = 0f
    private var textDownLy = 0f
    private var textMoved = false
    private val textFrame = Paint(Paint.ANTI_ALIAS_FLAG).apply { style = Paint.Style.STROKE }

    private var action = Action.NONE
    private var actionId = -1 // pointer id driving [action]
    private var actionStylus = false
    private var target: Placed? = null
    private var actionDownX = 0f
    private var actionDownY = 0f

    // The stroke being drawn, sheet coordinates: x, y, pressure, t quads.
    private var samples = FloatArray(4 * 512)
    private var count = 0
    private var strokeStart = 0L
    /** The stroke was replaced by a recognised shape at pen-up. */
    private var shaped = false
    private var lastPressure = 0.5f
    private var stillSince = 0L
    private var stillX = 0f
    private var stillY = 0f
    private var liveOutline = FloatArray(0)
    private var liveLayers: List<FloatArray> = emptyList()
    private var liveLayerPaths: List<Path> = emptyList()
    /** Pencil grain size for the current zoom (see [Pencil.texelFor]). */
    private var pencilTexel = 0f
    private var pencilGeneration = -1
    /** Frames drawn, the zoom of the last one and when the zoom last changed (for the ink's tiles). */
    private var frames = 0L
    private var trimAt = 0L
    private var lastScale = 0f
    private var scaleAt = 0L
    private val livePath = Path()
    private var liveColor = Color.BLACK

    private val lasso = ArrayList<Float>()
    private var handle: Handle? = null
    private var handleDrag = 0f
    /** When the "+" was touched; with a hold time the ring around it fills until then + hold. */
    private var handleDownAt = 0L
    private val handleConfirm = Runnable { confirmHandle() }
    private val holdTrack = Paint(Paint.ANTI_ALIAS_FLAG).apply { style = Paint.Style.STROKE; strokeCap = Paint.Cap.ROUND }
    private val holdRing = Paint(Paint.ANTI_ALIAS_FLAG).apply { style = Paint.Style.STROKE; strokeCap = Paint.Cap.ROUND }
    private var lastTapAt = 0L
    private var lastTapX = 0f
    private var lastTapY = 0f
    private var anim: android.animation.ValueAnimator? = null

    var lock = Lock.NONE
        set(v) { field = v; if (v != Lock.NONE) lockCenterX = (currentRowSheetAtCenter() ?: currentSheet())?.rect?.centerX() ?: 0f; snapLockedX(); changed() }
    private val zoomLocked get() = lock == Lock.ZOOM
    private val pageLocked get() = lock == Lock.PAGE
    private var lockCenterX = 0f
    /** Sideways movement beyond what the lock allows (the rubber-band pull). */
    private var stretch = 0f
    private var panTracking = false
    private var panBaseTx = 0f
    private var freeDx = 0f

    // Laser pointer: screen points (x, y, time) of the trail; fades after the pen lifts.
    /** Laser points: world x, y, time, segment (one segment per pen-down). */
    private val laser = ArrayList<FloatArray>()
    /**
     * Times kept as floats count from when this view was made: a float of the uptime itself
     * (days on a tablet that is never switched off) steps by 64 ms and more, so the laser's
     * trail faded in jumps and a stroke started beside the page got times before its start.
     */
    private val timeBase = SystemClock.uptimeMillis()
    private fun since(t: Long) = (t - timeBase).toFloat()
    private var laserUpAt = 0L
    private var laserSeg = 0
    private val laserEnded = HashMap<Int, Long>()
    private val laserGlow = Paint(Paint.ANTI_ALIAS_FLAG).apply { color = 0x66FF2A2A; style = Paint.Style.STROKE; strokeCap = Paint.Cap.ROUND; strokeJoin = Paint.Join.ROUND }
    private val laserCore = Paint(Paint.ANTI_ALIAS_FLAG).apply { color = 0xFFFF3B30.toInt(); style = Paint.Style.STROKE; strokeCap = Paint.Cap.ROUND; strokeJoin = Paint.Join.ROUND }

    /** Next tap picks a colour from the page (pipette). */
    var eyedropper: ((Int) -> Unit)? = null
        set(v) { field = v; invalidate() }

    private var patternBuf = FloatArray(8192)
    /** One stroke selected: its start and end (page points), the handles to change length and angle. */
    private var selEnds: FloatArray? = null
    private var selEndsFor: Selection? = null
    /** Dragging an end: which (0 start, 1 end) and where it is now (page points). */
    private var endWhich = 0
    private var endX = 0f
    private var endY = 0f
    /** The line's end pulled sideways: it turns (else only its length changes); the angle step it snapped to. */
    private var endTurning = false
    private var endSnapDeg: Int? = null
    private var selDx = 0f
    private var selDy = 0f
    private var selScale = 1f
    /**
     * A grip of a one-page selection being dragged: 0–3 the corners (top left, top right, bottom
     * right, bottom left; even scaling), 4–7 the sides (top, right, bottom, left; one direction
     * only), 8 turning. The anchor (page points) is the opposite corner or side, or the middle.
     */
    private var selGrip = -1
    private var selKx = 1f
    private var selKy = 1f
    private var selAng = 0f
    private var gripAx = 0f
    private var gripAy = 0f
    private var gripFrom = 0f
    private var selTurnStep: Int? = null

    private var miniUntil = 0L
    private val miniBg = Paint(Paint.ANTI_ALIAS_FLAG).apply { color = 0xE61A1A1A.toInt() }
    private val miniInk = Paint().apply { color = 0xFFE8E8E8.toInt() }
    private val miniView = Paint(Paint.ANTI_ALIAS_FLAG).apply { color = 0xFFD71921.toInt(); style = Paint.Style.STROKE; strokeWidth = 2f }
    private val rulerPaint = Paint(Paint.ANTI_ALIAS_FLAG).apply { color = 0x5C1E1E22 }
    private val rulerEdge = Paint(Paint.ANTI_ALIAS_FLAG).apply { color = 0xCC1E1E22.toInt(); style = Paint.Style.STROKE }
    private val rulerTick = Paint(Paint.ANTI_ALIAS_FLAG).apply { color = 0xE6F2F2F4.toInt() }
    private val handleFill = Paint(Paint.ANTI_ALIAS_FLAG).apply { color = 0xF21E1E22.toInt() }
    private val handleRing = Paint(Paint.ANTI_ALIAS_FLAG).apply { color = 0xFF3A3A40.toInt(); style = Paint.Style.STROKE; strokeWidth = 1.5f }
    private val handlePlus = Paint(Paint.ANTI_ALIAS_FLAG).apply { color = 0xFFF2F2F4.toInt(); style = Paint.Style.STROKE; strokeCap = Paint.Cap.ROUND }
    private val ghostFill = Paint(Paint.ANTI_ALIAS_FLAG).apply { color = 0x33FFFFFF }
    private val ghostLine = Paint(Paint.ANTI_ALIAS_FLAG).apply { color = 0xFFD71921.toInt(); style = Paint.Style.STROKE }

    private val tileRequests = ArrayList<uniffi.n0te_ffi.FfiTileRequest>()

    private val touchSlop = ViewConfiguration.get(context).scaledTouchSlop.toFloat()
    private val palmPx get() = Settings.palmMm / 25.4f * resources.displayMetrics.xdpi
    private val density = resources.displayMetrics.density

    // --- paints ------------------------------------------------------------------------------
    private val bgColor = Ui.theme.bg.toInt()
    private val paperColor = Ui.theme.paper.toInt()
    private val paperPaint = Paint().apply { color = paperColor }
    private val shadowPaint = Paint().apply { color = if (Ui.theme.dark) 0x22FFFFFF else 0x22000000 }
    private val patternPaint = Paint(Paint.ANTI_ALIAS_FLAG).apply { color = 0xFFD6DCE6.toInt(); style = Paint.Style.STROKE }
    private val dotPaint = Paint(Paint.ANTI_ALIAS_FLAG).apply { color = 0xFFC2C9D4.toInt(); strokeCap = Paint.Cap.ROUND }
    private val marginPaint = Paint(Paint.ANTI_ALIAS_FLAG).apply { color = 0x66D71921; style = Paint.Style.STROKE }
    private val labelPaint = Paint(Paint.ANTI_ALIAS_FLAG).apply { color = Ui.theme.muted.toInt(); typeface = Typeface.MONOSPACE }
    private val headerPaint = Paint(Paint.ANTI_ALIAS_FLAG).apply { color = Ui.theme.fg.toInt(); typeface = Typeface.create(Typeface.DEFAULT, Typeface.BOLD) }
    private val headerLine = Paint().apply { color = Ui.theme.border.toInt() }
    private val foldFill = Paint(Paint.ANTI_ALIAS_FLAG).apply { color = Ui.theme.elevated.toInt() }
    private val foldStroke = Paint(Paint.ANTI_ALIAS_FLAG).apply { color = Ui.theme.fg.toInt(); style = Paint.Style.STROKE; strokeWidth = 1.8f; strokeCap = Paint.Cap.ROUND; strokeJoin = Paint.Join.ROUND }
    private var actionDownAt = 0L
    /** Area eraser: the path so far (x, y, radius in sheet coordinates) and what it hides. */
    private val erasePath = ArrayList<Float>()
    private var eraseClip: Path? = null
    private var eraseSheet: String? = null
    private var eraseR = 0f
    private var lastEraseX = Float.NaN
    private var lastEraseY = 0f
    private var lastEraseT = 0L
    /** The pen's side button, as far as any event reported it (some pens only say so on hover). */
    private var penButton = false
    /** The running stroke continued from another page: no shape or scribble detection at the end. */
    private var splitStroke = false
    private val haloPaint = Paint(Paint.ANTI_ALIAS_FLAG).apply { color = 0x88D71921.toInt(); style = Paint.Style.STROKE; strokeJoin = Paint.Join.ROUND }
    private val inkPaint = Paint(Paint.ANTI_ALIAS_FLAG).apply { style = Paint.Style.FILL }
    private val livePaint = Paint(Paint.ANTI_ALIAS_FLAG).apply { style = Paint.Style.FILL }
    private val lassoPaint = Paint(Paint.ANTI_ALIAS_FLAG).apply { color = 0xFFD71921.toInt(); style = Paint.Style.STROKE; pathEffect = DashPathEffect(floatArrayOf(10f, 8f), 0f) }
    private val selPaint = Paint(Paint.ANTI_ALIAS_FLAG).apply { color = 0xFFD71921.toInt(); style = Paint.Style.STROKE }
    private val handlePaint = Paint(Paint.ANTI_ALIAS_FLAG).apply { color = 0xFFD71921.toInt() }
    private val cursorPaint = Paint(Paint.ANTI_ALIAS_FLAG).apply { style = Paint.Style.STROKE; strokeWidth = 2f }

    init {
        isFocusable = true
        isHapticFeedbackEnabled = false
    }

    // ========================================================================================
    // View state
    // ========================================================================================

    val brush: FfiBrush
        get() {
            val base = Tools.inks[colorIdx]
            return if (tool == Tool.MARKER) {
                FfiBrush(Tools.widths[widthIdx] * 5f, 0f, ((base and 0x00FFFFFF) or (Ui.HIGHLIGHT_ALPHA shl 24)).toUInt(), 2u)
            } else {
                val pen = Tools.pen
                val thin = if (pen == PenType.FOUNTAIN) Fountain.thinning() else pen.thinning
                FfiBrush(Tools.widths[widthIdx] * pen.widthMul, thin, ((base and 0x00FFFFFF) or (pen.inkAlpha() shl 24)).toUInt(), pen.kind)
            }
        }

    /** The main sheet nearest the middle of the screen. */
    fun currentSheet(): Placed? {
        val m = model ?: return null
        val cy = toWorldY(height / 2f)
        return m.placed.filter { it.side == 0 }.minByOrNull { p ->
            when {
                cy < p.rect.top -> p.rect.top - cy
                cy > p.rect.bottom -> cy - p.rect.bottom
                else -> 0f
            }
        }
    }

    /** Shows [sheetId] at the top (and its row centred horizontally on side sheets). */
    fun scrollTo(sheetId: String) {
        val p = model?.placed?.firstOrNull { it.sheetId == sheetId } ?: return
        scroller.forceFinished(true)
        if (p.side != 0) tx = width / 2f - p.rect.centerX() * scale
        else tx = width / 2f
        if (lock != Lock.NONE) lockCenterX = if (p.side != 0) p.rect.centerX() else 0f
        ty = 24f * density - p.rect.top * scale
        clampTransform()
        changed()
    }

    /**
     * Shows a spot ([r], page points on [sheetId]: a search hit, a heading) in the upper middle
     * of the screen at the current zoom, and lets it light up for a moment.
     */
    fun showSpot(sheetId: String, r: RectF) {
        val p = model?.placed?.firstOrNull { it.sheetId == sheetId } ?: return
        val cx = p.rect.left + r.centerX(); val cy = p.rect.top + r.centerY()
        var tx1 = width / 2f - cx * scale
        if (lock != Lock.NONE) { lockCenterX = p.rect.centerX(); tx1 = pageTx(tx1, scale) }
        moveTo(scale, tx1, height * 0.38f - cy * scale, animate = true)
        flash = Triple(sheetId, RectF(r), SystemClock.uptimeMillis())
        invalidate()
    }

    /** A spot lighting up (page, box in page points, since when). */
    private var flash: Triple<String, RectF, Long>? = null
    private val flashPaint = Paint(Paint.ANTI_ALIAS_FLAG).apply { style = Paint.Style.FILL }

    private fun drawFlash(c: Canvas) {
        val (sheetId, r, at) = flash ?: return
        val age = SystemClock.uptimeMillis() - at
        if (age > 1800) { flash = null; return }
        val p = model?.placed?.firstOrNull { it.sheetId == sheetId } ?: return
        val a = if (age < 300) 1f else 1f - (age - 300) / 1500f
        flashPaint.color = (Ui.theme.accent.toInt() and 0x00FFFFFF) or ((0x55 * a).toInt() shl 24)
        val pad = 4f
        c.drawRoundRect(p.rect.left + r.left - pad, p.rect.top + r.top - pad, p.rect.left + r.right + pad, p.rect.top + r.bottom + pad, 4f, 4f, flashPaint)
        postInvalidateOnAnimation()
    }

    fun fitWidth() {
        if (width == 0) return
        scale = width / (NotebookModel.A4_W + 2 * MARGIN_PT)
        tx = width / 2f
        ty = 16f * density
        fitted = true
        changed()
    }

    /** 100 % = the page width fills the screen (in a board: the board frame's width). */
    fun zoomPercent(): Int {
        val ref = if (board != null) width / NotebookModel.BOARD_W else width / (NotebookModel.A4_W + 2 * MARGIN_PT)
        return if (ref > 0f) (scale / ref * 100f).toInt() else 100
    }

    /** The sheet in focus: the board, or the sheet of the current row nearest the screen's middle. */
    fun focusedSheet(): Placed? = board?.let { b -> model?.placed?.firstOrNull { it.sheetId == b.sheetId } } ?: currentRowSheetAtCenter()

    /** The sheet of the current row whose centre is nearest the middle of the screen. */
    private fun currentRowSheetAtCenter(): Placed? {
        val m = model ?: return null
        val cur = currentSheet() ?: return null
        val cx = toWorldX(width / 2f)
        return m.placed.filter { it.mainNode == cur.node }.minByOrNull { abs(it.rect.centerX() - cx) }
    }

    private fun snapLockedX() {
        if (lock == Lock.NONE || board != null) return
        animateTo(scale, if (zoomLocked) width / 2f - lockCenterX * scale else pageTx(tx, scale), ty)
    }

    /** The sheet the lock holds on to. */
    private fun lockSheet(): Placed? {
        val m = model ?: return null
        val cur = currentSheet() ?: return null
        return m.placed.filter { it.mainNode == cur.node }.minByOrNull { abs(it.rect.centerX() - lockCenterX) }
    }

    /** Page lock: [x] limited so the locked sheet fills the width (or is centred when narrower). */
    private fun pageTx(x: Float, s: Float): Float {
        val r = lockSheet()?.rect ?: return x
        val margin = 12f * density
        return if (r.width() * s + 2 * margin <= width) width / 2f - r.centerX() * s
        else x.coerceIn(width - margin - r.right * s, margin - r.left * s)
    }

    /**
     * Double tap: the whole page fits the screen; a second double tap fits its width.
     */
    /** Shows the page at the default size (Settings.pageScale): page width, whole page or a zoom. */
    fun showPage(sheetId: String, animate: Boolean = true) {
        val p = model?.placed?.firstOrNull { it.sheetId == sheetId } ?: return
        if (p.isBoard || width == 0) { scrollTo(sheetId); return }
        when (val d = Settings.pageScale) {
            "width" -> fitSheet(p, whole = false, animate = animate)
            "page" -> fitSheet(p, whole = true, animate = animate)
            else -> {
                val ref = width / (NotebookModel.A4_W + 2 * MARGIN_PT)
                val s1 = (ref * (d.toIntOrNull() ?: 100) / 100f).coerceIn(minScale(), maxScale())
                if (lock != Lock.NONE) lockCenterX = p.rect.centerX()
                moveTo(s1, width / 2f - p.rect.centerX() * s1, 104f * density - p.rect.top * s1, animate)
            }
        }
    }

    private fun moveTo(s1: Float, tx1: Float, ty1: Float, animate: Boolean) {
        if (animate) { animateTo(s1, tx1, ty1); return }
        scale = s1; tx = tx1; ty = ty1
        clampTransform()
        changed()
    }

    fun fitSheet(p: Placed, whole: Boolean? = null, animate: Boolean = true) {
        val top = 104f * density; val bottom = 20f * density; val side = 20f * density
        val availW = width - 2 * side; val availH = height - top - bottom
        if (availW <= 0 || availH <= 0) return
        val sPage = min(availW / p.rect.width(), availH / p.rect.height())
        val sWidth = availW / p.rect.width()
        val wholePage = whole ?: (abs(scale - sPage) / sPage > 0.04f)
        val s1 = if (wholePage) sPage else sWidth
        val tx1 = width / 2f - p.rect.centerX() * s1
        if (lock != Lock.NONE) lockCenterX = p.rect.centerX()
        val ty1 = if (wholePage) top + (availH - p.rect.height() * s1) / 2 - p.rect.top * s1 else top - p.rect.top * s1
        moveTo(s1, tx1, ty1, animate)
    }

    private fun animateTo(s1: Float, tx1: Float, ty1: Float) {
        scroller.forceFinished(true)
        anim?.cancel()
        val s0 = scale; val x0 = tx; val y0 = ty
        anim = android.animation.ValueAnimator.ofFloat(0f, 1f).setDuration(260).apply {
            interpolator = android.view.animation.DecelerateInterpolator(1.6f)
            addUpdateListener {
                val f = it.animatedValue as Float
                scale = s0 + (s1 - s0) * f
                tx = x0 + (tx1 - x0) * f
                ty = y0 + (ty1 - y0) * f
                changed()
            }
            start()
        }
    }

    /** The "+" handles beside the current page row (none while drawing or in board mode). */
    fun handles(): List<Handle> {
        val m = model ?: return emptyList()
        if (!Settings.handles || readOnly || board != null || (action != Action.NONE && action != Action.HANDLE)) return emptyList()
        val cur = currentSheet() ?: return emptyList()
        if (cur.rect.width() * scale < 160f) return emptyList()
        val row = m.placed.filter { it.mainNode == cur.node }
        val left = row.minByOrNull { it.rect.left } ?: return emptyList()
        val right = row.maxByOrNull { it.rect.right } ?: return emptyList()
        val off = max(NotebookModel.SIDE_GAP / 2, HANDLE_R * density * 1.6f / scale)
        val w = cur.rect.width(); val h = cur.rect.height()
        val rowBottom = row.maxOf { it.rect.bottom }
        val all = listOf(
            Handle(-1, left.rect.left - off, cur.rect.centerY(), cur.node,
                RectF(left.rect.left - NotebookModel.SIDE_GAP - w, cur.rect.top, left.rect.left - NotebookModel.SIDE_GAP, cur.rect.top + h)),
            Handle(1, right.rect.right + off, cur.rect.centerY(), cur.node,
                RectF(right.rect.right + NotebookModel.SIDE_GAP, cur.rect.top, right.rect.right + NotebookModel.SIDE_GAP + w, cur.rect.top + h)),
            Handle(2, cur.rect.centerX(), rowBottom + NotebookModel.ROW_GAP / 2, cur.node,
                RectF(cur.rect.left, rowBottom + NotebookModel.ROW_GAP, cur.rect.right, rowBottom + NotebookModel.ROW_GAP + h)),
        )
        return if (Settings.sidePages) all else all.filter { it.dir == 2 }
    }

    private fun handleAt(x: Float, y: Float): Handle? = handles().firstOrNull { hypot(it.wx * scale + tx - x, it.wy * scale + ty - y) < HANDLE_R * density * 1.6f }

    /** Opens a board full screen (infinite canvas). */
    fun enterBoard(p: Placed) {
        if (!p.isBoard || board != null) return
        clearSelection()
        closeLoupe()
        columnTransform = floatArrayOf(scale, tx, ty)
        board = Placed(p.sheetId, p.node, p.mainNode, p.spec, RectF(0f, 0f, p.rect.width(), p.rect.height()), p.side, p.number)
        // Same picture as before (the frame stays where it was), then grow it to fill the screen.
        tx += p.rect.left * scale
        ty += p.rect.top * scale
        val st = model?.state(p.sheetId)
        st?.invalidateNode()
        // Fit the frame and whatever was drawn outside it.
        val fit = RectF(0f, 0f, p.rect.width(), p.rect.height())
        st?.strokes?.forEach { fit.union(it.bounds) }
        val s1 = (min(width / fit.width(), height / fit.height()) * 0.9f).coerceIn(minScale(), maxScale())
        animateTo(s1, width / 2f - fit.centerX() * s1, height / 2f - fit.centerY() * s1)
        changed()
    }

    /** Zooms to [percent] (see [zoomPercent]) around the middle of the screen. */
    fun zoomTo(percent: Int) {
        val ref = if (board != null) width / NotebookModel.BOARD_W else width / (NotebookModel.A4_W + 2 * MARGIN_PT)
        val s1 = (ref * percent / 100f).coerceIn(minScale(), maxScale())
        val cx = width / 2f; val cy = height / 2f
        val wx = (cx - tx) / scale; val wy = (cy - ty) / scale
        val tx1 = when {
            board != null -> cx - wx * s1
            zoomLocked -> width / 2f - lockCenterX * s1
            pageLocked -> pageTx(cx - wx * s1, s1)
            else -> cx - wx * s1
        }
        animateTo(s1, tx1, cy - wy * s1)
    }

    fun exitBoard() {
        val b = board ?: return
        clearSelection()
        board = null
        // Back to the column: start from the frame where the board view is now, then return.
        val p = model?.placed?.firstOrNull { it.sheetId == b.sheetId }
        if (p != null) { tx -= p.rect.left * scale; ty -= p.rect.top * scale }
        model?.state(b.sheetId)?.invalidateNode()
        animateTo(columnTransform[0], columnTransform[1], columnTransform[2])
        changed()
    }

    /** The board's minimap for a moment (after a pan or zoom; the frame-time bench too). */
    fun showMinimap() {
        miniUntil = SystemClock.uptimeMillis() + 900
        postInvalidateDelayed(950)
    }

    private fun placeRuler() {
        rulerCx = toWorldX(width / 2f); rulerCy = toWorldY(height / 2f)
        rulerAngle = 0f
    }

    /** World point in ruler coordinates (along, across). */
    private fun rulerLocal(wx: Float, wy: Float): FloatArray {
        val dx = wx - rulerCx; val dy = wy - rulerCy
        val c = kotlin.math.cos(-rulerAngle); val sn = kotlin.math.sin(-rulerAngle)
        return floatArrayOf(dx * c - dy * sn, dx * sn + dy * c)
    }

    private fun rulerContains(wx: Float, wy: Float): Boolean {
        if (!rulerOn) return false
        val l = rulerLocal(wx, wy)
        return abs(l[0]) <= RULER_LEN / 2 && abs(l[1]) <= RULER_W / 2
    }

    /** Which ruler edge a pen at (wx, wy) is touching: -1 top, 1 bottom, 0 none. */
    private fun rulerEdgeAt(wx: Float, wy: Float): Int {
        if (!rulerOn) return 0
        val l = rulerLocal(wx, wy)
        if (abs(l[0]) > RULER_LEN / 2) return 0
        val tol = 18f * density / scale
        return when {
            abs(l[1] + RULER_W / 2) < tol -> -1
            abs(l[1] - RULER_W / 2) < tol -> 1
            else -> 0
        }
    }

    /** Projects a world point onto the chosen ruler edge (ink lies along the outside of it). */
    private fun snapToRuler(wx: Float, wy: Float, edge: Int): FloatArray {
        val l = rulerLocal(wx, wy)
        val across = edge * (RULER_W / 2 + brush.width / 2)
        val c = kotlin.math.cos(rulerAngle); val sn = kotlin.math.sin(rulerAngle)
        return floatArrayOf(rulerCx + l[0] * c - across * sn, rulerCy + l[0] * sn + across * c)
    }

    /**
     * Where the view is, relative to the page in focus: "sheet|scale|x|y|width" (x, y: the
     * screen's top-left corner in page coordinates). Saved when the notebook closes.
     */
    fun cameraState(): String? {
        if (board != null || width == 0) return null
        val p = focusedSheet() ?: return null
        return "${p.sheetId}|$scale|${toWorldX(0f) - p.rect.left}|${toWorldY(0f) - p.rect.top}|$width"
    }

    /** Back to a [cameraState]; false when that page is gone (then the caller opens as usual). */
    fun restoreCamera(state: String): Boolean {
        val f = state.split('|')
        if (f.size < 5 || width == 0) return false
        val p = model?.placed?.firstOrNull { it.sheetId == f[0] } ?: return false
        if (p.isBoard) return false
        val w0 = f[4].toFloatOrNull() ?: return false
        var s1 = f[1].toFloatOrNull() ?: return false
        val x = f[2].toFloatOrNull() ?: return false
        val y = f[3].toFloatOrNull() ?: return false
        if (w0 > 0f && w0 != width.toFloat()) s1 *= width / w0 // turned since: same part of the page across
        s1 = s1.coerceIn(minScale(), maxScale())
        if (lock != Lock.NONE) lockCenterX = p.rect.centerX()
        moveTo(s1, -(p.rect.left + x) * s1, -(p.rect.top + y) * s1, animate = false)
        return true
    }

    private fun minScale(): Float = if (board != null) 0.08f * width / NotebookModel.BOARD_W else width / ((model?.bounds?.width() ?: NotebookModel.A4_W) + 600f)
    private fun maxScale(): Float = if (board != null) 10f * width / NotebookModel.BOARD_W else width / NotebookModel.A4_W * 8f

    private fun clampTransform() {
        if (board != null) return // infinite
        val m = model ?: return
        val b = m.bounds
        if (b.isEmpty) return
        // Keep some of the content on screen.
        val slack = 0.5f
        tx = tx.coerceIn(width * slack - b.right * scale, width * (1 - slack) - b.left * scale)
        ty = ty.coerceIn(height * slack - b.bottom * scale, height * (1 - slack) - b.top * scale)
    }

    private fun changed() {
        invalidate()
        listener?.onViewState()
        sendPresence(false)
    }

    /** The text box at (lx, ly) on [p]'s sheet (a little slack around it), topmost first. */
    private fun textAt(p: Placed, lx: Float, ly: Float): uniffi.n0te_ffi.FfiTextBox? {
        val st = model?.sheets?.get(p.sheetId) ?: return null
        val slack = 6f / scale
        return st.texts.lastOrNull { TextBoxes.bounds(it).apply { inset(-slack, -slack) }.contains(lx, ly) }
    }

    /** With the text tool: dashed frames around the text boxes; the one being edited in red. */
    private fun drawTextFrames(c: Canvas, m: NotebookModel) {
        val editing = editingText
        if (tool != Tool.TEXT && editing == null) return
        for (p in m.placed) {
            if (tool != Tool.TEXT && p.sheetId != editing?.first) continue
            val st = m.sheets[p.sheetId] ?: continue
            for (b in st.texts) {
                if (b.id == BODY_ID) continue
                val on = editing?.first == p.sheetId && editing.second == b.id
                if (!on && tool != Tool.TEXT) continue
                val r = TextBoxes.bounds(b).apply { offset(p.rect.left, p.rect.top); inset(-4f / scale, -4f / scale) }
                textFrame.strokeWidth = (if (on) 2f else 1f) / scale
                textFrame.color = if (on) 0xFFD71921.toInt() else 0x80D71921.toInt()
                textFrame.pathEffect = if (on) null else DashPathEffect(floatArrayOf(5f / scale, 4f / scale), 0f)
                c.drawRect(r, textFrame)
            }
        }
    }

    /** Scrolls so the text box is in the upper part of the screen (above the keyboard). */
    fun revealText(sheetId: String, boxId: String) {
        val m = model ?: return
        val p = m.placed.firstOrNull { it.sheetId == sheetId } ?: return
        val b = m.sheets[sheetId]?.texts?.firstOrNull { it.id == boxId }
        val top = p.rect.top + (b?.y ?: 64f)
        val sy = ty + top * scale
        if (sy > height * 0.42f || sy < height * 0.08f) animateTo(scale, tx, ty - (sy - height * 0.2f))
    }

    /** Moves the pages by [dy] screen pixels (keeps the text cursor above the keyboard). */
    fun nudge(dy: Float) = animateTo(scale, tx, ty + dy)

    /** Sends this view's position now (live heartbeat, after opening). */
    fun pushPresence() = sendPresence(true)

    /** This view's position for the laptop (sheet points of the sheet in focus), ≤ 10×/s. */
    private fun sendPresence(force: Boolean, laserScreen: PointF? = null) {
        val sink = live ?: return
        val now = SystemClock.uptimeMillis()
        if (!force && now - presenceAt < 100) return
        presenceAt = now
        val p = focusedSheet() ?: return
        val r = floatArrayOf(toWorldX(0f) - p.rect.left, toWorldY(0f) - p.rect.top, toWorldX(width.toFloat()) - p.rect.left, toWorldY(height.toFloat()) - p.rect.top)
        val laser = laserScreen?.let { floatArrayOf(toWorldX(it.x) - p.rect.left, toWorldY(it.y) - p.rect.top) }
        sink.presence(p.sheetId, r, scale, laser)
    }

    /** New samples of the stroke being drawn go to the laptop (batched, ≤ 20×/s). */
    private fun sendLiveSamples(done: Boolean) {
        val sink = live ?: return
        val p = target ?: return
        val now = SystemClock.uptimeMillis()
        if (!done && liveHold) return
        if (!done && (count - liveSent < 4 || now - liveSentAt < 50)) return
        val from = liveSent.coerceAtMost(count)
        if (from == count && !done) return
        sink.samples(p.sheetId, liveStrokeId, liveSeq++, brush, bytesOf(samples.copyOfRange(from * 4, count * 4)), done)
        liveSent = count
        liveSentAt = now
    }

    /** A finger resting without moving: after [HOLD_MS] the hold menu (paste here, …). */
    private var holdId = -1
    private val fingerHold = Runnable {
        val f = fingers[holdId] ?: return@Runnable
        if (f.rejected || gesture != Gesture.PAN_PENDING || panId != holdId || activeFingers().size != 1 || penId != -1) return@Runnable
        if (hypot(f.x - f.downX, f.y - f.downY) >= touchSlop) return@Runnable
        // Taken: no pan, no tap when it lifts.
        gesture = Gesture.NONE
        panId = -1
        performHapticFeedback(android.view.HapticFeedbackConstants.LONG_PRESS)
        // On something (a picture, a graph, a written word, a drawn line): it is selected and
        // the finger carries it on. On an empty spot: the menu (paste here, text here, …).
        if (!holdSelect(f)) listener?.onFingerHold(f.x, f.y)
    }

    /** Selects what lies under the held finger [f] and lets the finger move it; false if nothing does. */
    private fun holdSelect(f: Finger): Boolean {
        val m = model ?: return false
        if (readOnly) return false
        val wx = toWorldX(f.x); val wy = toWorldY(f.y)
        val p = board ?: m.placed.firstOrNull { it.rect.contains(wx, wy) } ?: return false
        val st = m.sheets[p.sheetId] ?: return false
        val tx = wx - p.rect.left; val ty = wy - p.rect.top
        val r = 12f * density / scale
        // Ink lies on top: a stroke under the finger first (its whole word), else the box there.
        val s = st.strokes.lastOrNull { it.id !in st.hidden && RectF(it.bounds).apply { inset(-r, -r) }.contains(tx, ty) && polygonNear(it.outline, tx, ty, r) }
        if (s != null && LineGrip.straight(s.ends, LineGrip.HOLD_MIN)) {
            // A straight line on its own (even right by the writing): its ends get the grips
            // that make it longer or shorter.
            selectStrokes(p.sheetId, listOf(s))
        } else if (s != null) {
            val boxes = st.strokes.filter { it.id !in st.hidden && it.kind != 2 }.map { InkLines.Box(it.id, it.bounds.left, it.bounds.top, it.bounds.right, it.bounds.bottom) }
            val ids = InkLines.word(boxes, s.id).toSet()
            selectStrokes(p.sheetId, st.strokes.filter { it.id in ids }.ifEmpty { listOf(s) })
        } else {
            val im = st.images.lastOrNull { tx >= it.x && tx <= it.x + it.w && ty >= it.y && ty <= it.y + it.h } ?: return false
            selection = Selection(p.sheetId, setOf(im.id), RectF(im.x, im.y, im.x + im.w, im.y + im.h))
            listener?.onSelectionChanged(selection)
        }
        // The finger carries it on (lifted without moving, it just stays selected).
        selDx = 0f; selDy = 0f; selScale = 1f
        action = Action.MOVE_SEL
        actionId = f.id
        actionStylus = false
        actionDownX = f.x; actionDownY = f.y
        actionDownAt = SystemClock.uptimeMillis()
        startSelDrag()
        invalidate()
        return true
    }

    /** Pastes the clipboard centred at screen (x, y) on the page there (the hold menu). */
    fun pasteAt(x: Float, y: Float) {
        val clip = N0te.clip ?: return
        val m = model ?: return
        val wx = toWorldX(x); val wy = toWorldY(y)
        val p = board ?: m.placed.firstOrNull { it.rect.contains(wx, wy) } ?: focusedSheet() ?: return
        val cx = (if (p.isBoard) wx else wx.coerceIn(p.rect.left, p.rect.right)) - p.rect.left
        val cy = (if (p.isBoard) wy else wy.coerceIn(p.rect.top, p.rect.bottom)) - p.rect.top
        val img = clip.image
        if (img != null) {
            placeImage(p, img.blob, img.w, img.h, cx, cy)
            return
        }
        // Whole on the page where it fits (a selection from several pages is taller than it looks).
        val hw = clip.bounds.width() / 2f; val hh = clip.bounds.height() / 2f
        val px = if (board != null) cx else cx.coerceIn(min(hw, p.rect.width() / 2f), max(p.rect.width() - hw, p.rect.width() / 2f))
        val py = if (board != null) cy else cy.coerceIn(min(hh, p.rect.height() / 2f), max(p.rect.height() - hh, p.rect.height() / 2f))
        m.pasteStrokes(p.sheetId, clip.data, px - clip.bounds.centerX(), py - clip.bounds.centerY()) { added -> selectStrokes(p.sheetId, added) }
    }

    /** The page under screen (x, y) and the point there in sheet points (the hold menu's "Text hier"). */
    fun pageAt(x: Float, y: Float): Pair<Placed, android.graphics.PointF>? {
        val m = model ?: return null
        val wx = toWorldX(x); val wy = toWorldY(y)
        val p = m.placed.firstOrNull { it.rect.contains(wx, wy) && !it.isBoard } ?: return null
        return p to android.graphics.PointF(wx - p.rect.left, wy - p.rect.top)
    }

    /** A device left: its frame goes. */
    fun hideRemote(device: String) {
        // Its strokes being drawn go with it (a device that left never finishes them).
        val ink = remoteInk.keys.removeAll { it.startsWith("$device/") }
        if (remotes.remove(device) != null || ink) invalidate()
    }

    /** "Ansicht der Gäste zeigen" turned off: guests' (and the owner's) frames go at once. */
    fun hideOthers() {
        if (remotes.values.removeAll { !it.own }) invalidate()
    }

    /** Another device's view: shown as a frame; followed if it is the one to follow. */
    fun showRemote(device: String, view: RemoteView) {
        remotes[device] = view
        if (followDevice == device) lookAt(view.sheetId, view.rect)
        invalidate()
    }

    /** Last known view of another device (null: it hasn't shown where it looks). */
    fun remoteView(device: String): RemoteView? = remotes[device]

    /** Moves this view to show [rect] (sheet points) of [sheetId], animated (jump / summon / follow). */
    fun lookAt(sheetId: String, rect: RectF): Boolean {
        if (board != null) return false
        val p = model?.placed?.firstOrNull { it.sheetId == sheetId } ?: return false
        if (rect.width() <= 1f) return false
        val s1 = (width / rect.width()).coerceIn(minScale(), maxScale())
        val cx = p.rect.left + rect.centerX(); val cy = p.rect.top + rect.centerY()
        animateTo(s1, width / 2f - cx * s1, height / 2f - cy * s1)
        return true
    }

    /** Where this view looks: the sheet in focus and the screen in its points (to summon guests here). */
    fun currentView(): Pair<String, FloatArray>? {
        val p = focusedSheet() ?: return null
        return p.sheetId to floatArrayOf(toWorldX(0f) - p.rect.left, toWorldY(0f) - p.rect.top, toWorldX(width.toFloat()) - p.rect.left, toWorldY(height.toFloat()) - p.rect.top)
    }

    /** Moving the page by hand ends following (it would snap back with the next update). */
    private fun breakFollow() {
        if (followDevice == null) return
        followDevice = null
        listener?.onFollowStopped()
    }

    /** Samples of a stroke another device is drawing. */
    fun remoteSamples(device: String, stroke: Long, sheetId: String, brush: FfiBrush, bytes: ByteArray, done: Boolean) {
        val key = "$device/$stroke"
        val ri = remoteInk.getOrPut(key) { RemoteInk(sheetId, brush) }
        ri.samples += floatsOf(bytes)
        ri.path = InkStroke.pathOf(floatsOf(strokeOutline(brush, bytesOf(ri.samples))))
        val now = SystemClock.uptimeMillis()
        ri.lastAt = now
        if (done) ri.doneAt = now
        // Unfinished ones too once nothing came for a while (the other device dropped off, its
        // last packet got lost): they stayed on screen for good.
        remoteInk.entries.removeAll { (it.value.doneAt > 0 && now - it.value.doneAt > 8000) || (it.value.doneAt == 0L && now - it.value.lastAt > 15_000) }
        invalidate()
    }

    /**
     * A sheet was reloaded: finished remote strokes that it now holds go (same colour, about
     * the same extent). One drawn after the save that triggered this stays, or it would blink.
     */
    fun sheetReloaded(sheetId: String) {
        val stored = model?.sheets?.get(sheetId)?.strokes ?: return
        remoteInk.entries.removeAll { (_, ri) ->
            if (ri.sheetId != sheetId || ri.doneAt == 0L || ri.samples.size < 4) return@removeAll false
            var x0 = Float.MAX_VALUE; var y0 = Float.MAX_VALUE; var x1 = -Float.MAX_VALUE; var y1 = -Float.MAX_VALUE
            for (i in 0 until ri.samples.size / 4) {
                val x = ri.samples[4 * i]; val y = ri.samples[4 * i + 1]
                if (x < x0) x0 = x; if (y < y0) y0 = y; if (x > x1) x1 = x; if (y > y1) y1 = y
            }
            val tol = 2f + ri.brush.width * 2f + 0.2f * maxOf(x1 - x0, y1 - y0)
            stored.any { s ->
                s.color == ri.brush.color.toInt() && kotlin.math.abs(s.bounds.left - x0) <= tol && kotlin.math.abs(s.bounds.top - y0) <= tol &&
                    kotlin.math.abs(s.bounds.right - x1) <= tol && kotlin.math.abs(s.bounds.bottom - y1) <= tol
            }
        }
        invalidate()
    }

    private fun drawRemote(c: Canvas, m: NotebookModel) {
        for (ri in remoteInk.values) {
            val p = m.placed.firstOrNull { it.sheetId == ri.sheetId } ?: continue
            c.save(); c.translate(p.rect.left, p.rect.top)
            remoteInkPaint.color = ri.brush.color.toInt()
            c.drawPath(ri.path, remoteInkPaint)
            c.restore()
        }
        for (rv in remotes.values) {
            val p = m.placed.firstOrNull { it.sheetId == rv.sheetId } ?: continue
            val r = RectF(rv.rect).apply { offset(p.rect.left, p.rect.top) }
            // Guests and owners: shown or not when they arrive (Settings.guestFrames).
            if (!rv.own || Settings.remoteFrame) {
                remotePaint.strokeWidth = 2f / scale
                c.drawRoundRect(r, 4f / scale, 4f / scale, remotePaint)
                remoteLabel.textSize = 10f / scale
                c.drawText(rv.name.uppercase(), r.left + 5f / scale, r.top + 13f / scale, remoteLabel)
            }
            rv.laser?.let { c.drawCircle(p.rect.left + it.x, p.rect.top + it.y, 6f / scale, remoteLaser) }
        }
    }

    private fun transformed() {
        clampTransform()
        if (board != null) showMinimap()
        changed()
    }

    fun clearSelection() {
        if (selection == null) return
        val sel = selection!!
        for (part in sel.parts) model?.state(part.sheetId)?.let { it.floating = emptySet(); it.invalidateNode() }
        selection = null
        for (e in selInk.values) e.node.discardDisplayList()
        selInk.clear()
        listener?.onSelectionChanged(null)
        invalidate()
    }

    /** Copies the selection to the ink clipboard; [cut] also removes it. */
    fun copySelection(cut: Boolean, then: () -> Unit = {}) {
        val sel = selection ?: return
        val m = model ?: return
        if (!sel.multi) {
            m.copyStrokes(sel.sheetId, sel.ids) { data ->
                N0te.clip = Clip(data, RectF(sel.bounds))
                if (cut) deleteSelection(sel)
                then()
            }
            return
        }
        // Several pages: every part shifted into one frame (the box's top-left at 0, 0).
        val u = selUnion(sel)
        val all = ArrayList<uniffi.n0te_ffi.FfiStrokeData>()
        var left = sel.parts.size
        for (part in sel.parts) {
            val p = placedFor(part.sheetId)
            if (p == null) { left--; continue }
            m.copyStrokes(part.sheetId, part.ids) { data ->
                all += data.map { shifted(it, p.rect.left - u.left, p.rect.top - u.top) }
                if (--left == 0) {
                    N0te.clip = Clip(all, RectF(0f, 0f, u.width(), u.height()))
                    if (cut) deleteSelection(sel)
                    then()
                }
            }
        }
    }

    /** Stroke data moved by (dx, dy) (samples: little-endian x, y, pressure, time). */
    private fun shifted(d: uniffi.n0te_ffi.FfiStrokeData, dx: Float, dy: Float): uniffi.n0te_ffi.FfiStrokeData {
        val b = java.nio.ByteBuffer.wrap(d.samples.copyOf()).order(java.nio.ByteOrder.LITTLE_ENDIAN)
        var i = 0
        while (i + 16 <= d.samples.size) { b.putFloat(i, b.getFloat(i) + dx); b.putFloat(i + 4, b.getFloat(i + 4) + dy); i += 16 }
        return uniffi.n0te_ffi.FfiStrokeData(d.brush, b.array())
    }

    /** Pastes the clipboard in the middle of the screen on the sheet in focus, selected. */
    fun paste() {
        val clip = N0te.clip ?: return
        val m = model ?: return
        val p = focusedSheet() ?: return
        val cx = (if (board != null) toWorldX(width / 2f) else toWorldX(width / 2f).coerceIn(p.rect.left, p.rect.right)) - p.rect.left
        val cy = (if (board != null) toWorldY(height / 2f) else toWorldY(height / 2f).coerceIn(p.rect.top, p.rect.bottom)) - p.rect.top
        val img = clip.image
        if (img != null) {
            placeImage(p, img.blob, img.w, img.h, cx, cy)
            return
        }
        m.pasteStrokes(p.sheetId, clip.data, cx - clip.bounds.centerX(), cy - clip.bounds.centerY()) { added -> selectStrokes(p.sheetId, added) }
    }

    /** Puts a stored picture on [p] centred at (cx, cy) (page points), at most 90 % of the page, and selects it. */
    private fun placeImage(p: Placed, blob: String, w0: Float, h0: Float, cx: Float, cy: Float, maxFrac: Float = 0.9f) {
        val m = model ?: return
        val k = minOf(1f, p.rect.width() * maxFrac / w0, if (p.isBoard) 1f else p.rect.height() * maxFrac / h0)
        val w = w0 * k; val h = h0 * k
        val x = if (p.isBoard) cx - w / 2 else (cx - w / 2).coerceIn(0f, (p.rect.width() - w).coerceAtLeast(0f))
        val y = if (p.isBoard) cy - h / 2 else (cy - h / 2).coerceIn(0f, (p.rect.height() - h).coerceAtLeast(0f))
        m.addImage(p.sheetId, blob, x, y, w, h) { id ->
            selection = Selection(p.sheetId, setOf(id), RectF(x, y, x + w, y + h))
            selDx = 0f; selDy = 0f; selScale = 1f
            listener?.onSelectionChanged(selection)
            invalidate()
        }
    }

    /** A dropped picture: onto the page under the screen point (sx, sy), centred there. */
    fun insertImageAt(blob: String, pxW: Int, pxH: Int, sx: Float, sy: Float) {
        val m = model ?: return
        val wx = toWorldX(sx); val wy = toWorldY(sy)
        val p = board ?: m.placed.firstOrNull { it.rect.contains(wx, wy) } ?: focusedSheet() ?: return
        placeImage(p, blob, pxW / 2f, pxH / 2f, wx.coerceIn(p.rect.left, p.rect.right) - p.rect.left, wy.coerceIn(p.rect.top, p.rect.bottom) - p.rect.top, maxFrac = 0.6f)
    }

    /** A picked picture (stored blob, pixel size): onto the page in view, about 2 px per point. */
    fun insertImage(blob: String, pxW: Int, pxH: Int) {
        val p = focusedSheet() ?: return
        val w = pxW / 2f; val h = pxH / 2f
        val cx = toWorldX(width / 2f).coerceIn(p.rect.left, p.rect.right) - p.rect.left
        val cy = toWorldY(height / 2f).coerceIn(p.rect.top, p.rect.bottom) - p.rect.top
        placeImage(p, blob, w, h, cx, cy, maxFrac = 0.6f)
    }

    /**
     * A screenshot of [r] (page points) on [p]: rendered fresh at up to 8 px per point — the
     * PDF straight from the PDF engine at that resolution, then images, text and ink on top —
     * whatever the zoom; stored as a JPEG and put on the clipboard to paste anywhere.
     */
    private fun snip(m: NotebookModel, p: Placed, st: SheetState, r: RectF) {
        val s = minOf(8f, 4096f / maxOf(r.width(), r.height()), kotlin.math.sqrt(14_000_000f / (r.width() * r.height()))).coerceAtLeast(1f)
        val wPx = (r.width() * s).toInt().coerceAtLeast(1); val hPx = (r.height() * s).toInt().coerceAtLeast(1)
        listener?.onMessage("Ausschnitt wird aufgenommen …")
        /** One rendered piece of the PDF: [q] its region in the PDF as printed, lying [shift] lower on the page. */
        class Piece(val q: RectF, val shift: Float, var bmp: android.graphics.Bitmap? = null)
        fun compose(pieces: List<Piece>) {
            val bmp = android.graphics.Bitmap.createBitmap(wPx, hPx, android.graphics.Bitmap.Config.ARGB_8888)
            val c = Canvas(bmp)
            c.drawColor(Color.WHITE)
            for (pc in pieces) {
                val pdf = pc.bmp ?: continue
                // The tile starts at the whole pixel at or before the piece's corner.
                val ox = (pc.q.left * s).toInt().toFloat() - r.left * s
                val oy = (pc.q.top * s).toInt().toFloat() + (pc.shift - r.top) * s
                c.save()
                c.clipRect(0f, (pc.q.top + pc.shift - r.top) * s, wPx.toFloat(), (pc.q.bottom + pc.shift - r.top) * s)
                c.drawBitmap(pdf, ox, oy, android.graphics.Paint(android.graphics.Paint.FILTER_BITMAP_FLAG))
                c.restore()
            }
            c.scale(s, s)
            c.translate(-r.left, -r.top)
            for (im in st.images) drawBox(c, im)
            for (stroke in st.strokes) if (RectF.intersects(stroke.bounds, r)) drawStroke(c, stroke)
            TextBoxes.draw(c, st.texts)
            N0te.background({
                val bytes = Images.jpeg(bmp, 93)
                N0te.library.putBlob(bytes)
            }) { res ->
                res.onSuccess { blob ->
                    N0te.clip = Clip(emptyList(), RectF(0f, 0f, r.width(), r.height()), ClipImage(blob, r.width(), r.height()))
                    listener?.onSnipped()
                }.onFailure { listener?.onMessage("Ausschnitt fehlgeschlagen: ${it.message}") }
            }
        }
        val blob = p.spec.pdfBlob ?: return compose(emptyList())
        // Space inserted splits the PDF: each band the region shows is rendered on its own.
        val gaps = p.spec.gaps
        val base = p.spec.height - (1 until gaps.size step 2).sumOf { gaps[it].toDouble() }.toFloat()
        val pieces = PdfTiles.bands(gaps, base).mapNotNull { (src, len, dst) ->
            val top = max(r.top, dst); val bottom = min(r.bottom, dst + len)
            if (bottom <= top) null else Piece(RectF(r.left, top - dst + src, r.right, bottom - dst + src), dst - src)
        }
        if (pieces.isEmpty()) return compose(emptyList())
        var left = pieces.size
        for (pc in pieces) PdfTiles.snip(blob, p.spec.pdfPage, p.spec.width, base, s, pc.q) { b -> pc.bmp = b; if (--left == 0) compose(pieces) }
    }

    private fun polyExtent(poly: FloatArray): Float {
        var x0 = Float.MAX_VALUE; var x1 = -Float.MAX_VALUE; var y0 = Float.MAX_VALUE; var y1 = -Float.MAX_VALUE
        for (i in 0 until poly.size / 2) { x0 = minOf(x0, poly[2 * i]); x1 = maxOf(x1, poly[2 * i]); y0 = minOf(y0, poly[2 * i + 1]); y1 = maxOf(y1, poly[2 * i + 1]) }
        return maxOf(x1 - x0, y1 - y0)
    }

    private val imagePaint = Paint(Paint.FILTER_BITMAP_FLAG or Paint.ANTI_ALIAS_FLAG)

    /** A page's images (under the ink); ones being moved are drawn with the selection. */
    private fun drawImages(c: Canvas, p: Placed, st: SheetState) {
        if (st.images.isEmpty()) return
        c.save()
        c.translate(p.rect.left, p.rect.top)
        for (im in st.images) {
            if (im.id in st.floating) continue
            if (im.plot != null) { drawBox(c, im); continue }
            val dst = RectF(im.x, im.y, im.x + im.w, im.y + im.h)
            val bmp = Images.get(im.blob)
            if (bmp != null) c.drawBitmap(bmp, null, dst, imagePaint)
            else c.drawRect(dst, shadowPaint) // still arriving (sync)
        }
        c.restore()
    }

    /** A coordinate system being edited: drawn as in the editor (in its stored box), not as stored. */
    var plotDraft: uniffi.n0te_ffi.FfiPlot? = null
        set(v) { field = v; invalidate() }

    /** An image or a coordinate system (canvas in page points). */
    private fun drawBox(c: Canvas, im: uniffi.n0te_ffi.FfiImage) {
        val plot = plotDraft?.takeIf { it.id == im.id }?.copy(x = im.x, y = im.y, w = im.w, h = im.h) ?: im.plot
        if (plot != null) { Plots.strokes(plot)?.forEach { drawStroke(c, it) }; return }
        Images.get(im.blob)?.let { b -> c.drawBitmap(b, null, RectF(im.x, im.y, im.x + im.w, im.y + im.h), imagePaint) }
    }

    // --- coordinate systems ------------------------------------------------------------------

    /** A coordinate system being put down: the box the pen draws (page points), what to do with it. */
    private class PlotBox(val then: (String, uniffi.n0te_ffi.FfiPlot) -> Unit) {
        var page: Placed? = null
        var x0 = 0f; var y0 = 0f; var x1 = 0f; var y1 = 0f
    }
    private var plotBox: PlotBox? = null
    private val plotBoxPaint = Paint(Paint.ANTI_ALIAS_FLAG).apply { style = Paint.Style.STROKE }
    private val plotBoxText = Paint(Paint.ANTI_ALIAS_FLAG).apply { textSize = 13f * resources.displayMetrics.scaledDensity }

    /** The next stroke of the pen (or a drawing finger) draws a coordinate system's box; [then] gets it to edit. */
    fun startPlotBox(then: (String, uniffi.n0te_ffi.FfiPlot) -> Unit) {
        clearSelection()
        plotBox = PlotBox(then)
        invalidate()
    }

    fun cancelPlotBox() { plotBox = null; invalidate() }
    val placingPlot: Boolean get() = plotBox != null

    /**
     * The plot the drawn box makes: on squared paper snapped to the squares, a square to the
     * unit and the origin on the line nearest the middle; else 20 pt to the unit, origin in
     * the middle. A tap (or a tiny box) puts a plot of the usual size there.
     */
    private fun plotOfBox(pb: PlotBox): uniffi.n0te_ffi.FfiPlot? {
        val p = pb.page ?: return null
        val squares = p.spec.background == FfiBackground.GRID && p.spec.spacing > 2f
        val u = if (squares) p.spec.spacing else 20f
        fun snap(v: Float) = if (squares) Math.round(v / u) * u else v
        val l = snap(min(pb.x0, pb.x1)); val r = snap(max(pb.x0, pb.x1)); val t = snap(min(pb.y0, pb.y1)); val b = snap(max(pb.y0, pb.y1))
        if (r - l < 4 * u || b - t < 3 * u) {
            return Plots.fresh(pb.x0, pb.y0, if (p.isBoard) 0f else p.rect.width(), if (p.isBoard) 0f else p.rect.height(), p.spec.background, p.spec.spacing)
        }
        val ox = snap((l + r) / 2); val oy = snap((t + b) / 2)
        return uniffi.n0te_ffi.FfiPlot("", l, t, r - l, b - t, ((l - ox) / u).toDouble(), ((r - ox) / u).toDouble(), ((oy - b) / u).toDouble(), ((oy - t) / u).toDouble(),
            axes = true, grid = !squares, minor = false, clean = false, color = Plots.AXES.toUInt(), width = 1.2f, pi = false, xName = "", yName = "", snap = 0u, align = squares, interactive = false, objs = emptyList())
    }

    private fun finishPlotBox(commit: Boolean) {
        val pb = plotBox ?: return
        val p = pb.page ?: return
        plotBox = null
        invalidate()
        if (!commit) return
        val plot = plotOfBox(pb) ?: return
        model?.addPlot(p.sheetId, plot) { id ->
            selection = Selection(p.sheetId, setOf(id), RectF(plot.x, plot.y, plot.x + plot.w, plot.y + plot.h))
            selDx = 0f; selDy = 0f; selScale = 1f
            listener?.onSelectionChanged(selection)
            invalidate()
            pb.then(p.sheetId, plot.copy(id = id))
        }
    }

    /** The box being drawn: its frame, where the axes will cross, its size in squares/units. */
    private fun drawPlotBox(c: Canvas) {
        val pb = plotBox ?: return
        val p = pb.page ?: return
        if (action != Action.PLOT_BOX) return
        val plot = plotOfBox(pb) ?: return
        val l = p.rect.left + plot.x; val t = p.rect.top + plot.y
        plotBoxPaint.color = Ui.theme.accent.toInt()
        plotBoxPaint.strokeWidth = 1.5f / scale
        plotBoxPaint.pathEffect = DashPathEffect(floatArrayOf(6f / scale, 4f / scale), 0f)
        c.drawRect(l, t, l + plot.w, t + plot.h, plotBoxPaint)
        plotBoxPaint.pathEffect = null
        val ox = l + ((0 - plot.xMin) / (plot.xMax - plot.xMin) * plot.w).toFloat()
        val oy = t + ((plot.yMax - 0) / (plot.yMax - plot.yMin) * plot.h).toFloat()
        if (ox in l..(l + plot.w)) c.drawLine(ox, t, ox, t + plot.h, plotBoxPaint)
        if (oy in t..(t + plot.h)) c.drawLine(l, oy, l + plot.w, oy, plotBoxPaint)
        val squares = p.spec.background == FfiBackground.GRID && p.spec.spacing > 2f
        val label = if (squares) "${Math.round(plot.w / p.spec.spacing)} × ${Math.round(plot.h / p.spec.spacing)} Kästchen"
            else "x ${Plots.format(plot.xMin)} … ${Plots.format(plot.xMax)} · y ${Plots.format(plot.yMin)} … ${Plots.format(plot.yMax)}"
        plotBoxText.color = Ui.theme.accent.toInt()
        c.save()
        c.translate(l, t - 6f / scale)
        c.scale(1f / scale, 1f / scale)
        c.drawText(label, 0f, 0f, plotBoxText)
        c.restore()
        postInvalidateOnAnimation()
    }

    /** A coordinate system as stored now (its box may have moved since an editor opened). */
    fun storedPlot(sheetId: String, id: String): uniffi.n0te_ffi.FfiPlot? = model?.sheets?.get(sheetId)?.images?.firstOrNull { it.id == id }?.plot

    /** A coordinate system on [sheetId] next to [r] (page points; handwriting beside or in it). */
    fun plotNear(sheetId: String, r: RectF): uniffi.n0te_ffi.FfiPlot? {
        val near = RectF(r).apply { inset(-40f, -40f) }
        return model?.sheets?.get(sheetId)?.images?.mapNotNull { it.plot }?.firstOrNull { RectF.intersects(RectF(it.x, it.y, it.x + it.w, it.y + it.h), near) }
    }

    /** A new coordinate system with [expr] beside [r] on [sheetId] (right of it if there is room, else below); [then] gets it to edit. */
    fun insertPlotBeside(sheetId: String, r: RectF, expr: String, color: Int, then: (String, uniffi.n0te_ffi.FfiPlot) -> Unit) {
        val m = model ?: return
        val p = m.placed.firstOrNull { it.sheetId == sheetId } ?: board?.takeIf { it.sheetId == sheetId } ?: return
        val pw = if (p.isBoard) 0f else p.rect.width(); val ph = if (p.isBoard) 0f else p.rect.height()
        val probe = Plots.fresh(0f, 0f, pw, ph, p.spec.background, p.spec.spacing)
        val right = pw <= 0f || r.right + 20f + probe.w <= pw
        val cx = if (right) r.right + 20f + probe.w / 2 else r.left + probe.w / 2
        val cy = if (right) r.centerY() else r.bottom + 20f + probe.h / 2
        val empty = Plots.fresh(cx, cy, pw, ph, p.spec.background, p.spec.spacing)
        val plot = runCatching { uniffi.n0te_ffi.plotAdd(empty, expr) }.getOrElse { listener?.onMessage("Graph: ${GraphSession.errText(it)}"); return }
        m.addPlot(sheetId, plot) { id ->
            selection = Selection(sheetId, setOf(id), RectF(plot.x, plot.y, plot.x + plot.w, plot.y + plot.h))
            selDx = 0f; selDy = 0f; selScale = 1f
            listener?.onSelectionChanged(selection)
            invalidate()
            then(sheetId, plot.copy(id = id))
        }
    }

    /**
     * Brings [r] (page points on [sheetId]) into view left of a panel [rightPx] wide, in the
     * upper part of the screen (the keyboard takes the lower), smaller if it doesn't fit.
     */
    fun showLeftOf(sheetId: String, r: RectF, rightPx: Float) = showBeside(sheetId, r, 0f, rightPx)

    /** Brings [r] into view between a panel [leftPx] wide on the left and one [rightPx] wide on the right. */
    fun showBeside(sheetId: String, r: RectF, leftPx: Float, rightPx: Float) {
        val p = model?.placed?.firstOrNull { it.sheetId == sheetId } ?: board?.takeIf { it.sheetId == sheetId } ?: return
        val room = (width - rightPx - leftPx).coerceAtLeast(width * 0.3f)
        val roomH = height * 0.5f
        val k = minOf(scale, room * 0.85f / (r.width() + 40f), roomH * 0.9f / (r.height() + 40f)).coerceAtLeast(minScale())
        val cx = p.rect.left + r.centerX(); val cy = p.rect.top + r.centerY()
        val onScreen = RectF(toScreenX(p.rect.left + r.left), toScreenY(p.rect.top + r.top), toScreenX(p.rect.left + r.right), toScreenY(p.rect.top + r.bottom))
        // Already in view beside the panels at this size: leave it.
        if (k == scale && onScreen.left >= leftPx && onScreen.right <= leftPx + room && onScreen.top >= 0f && onScreen.bottom <= height * 0.55f) return
        moveTo(k, leftPx + room / 2f - cx * k, height * 0.3f - cy * k, animate = true)
    }

    /** Where the view stands (zoom, shift), to come back to. */
    fun camera(): FloatArray = floatArrayOf(scale, tx, ty)

    fun setCamera(c: FloatArray, animate: Boolean = true) = moveTo(c[0], c[1], c[2], animate)

    /** Zoomed so the box [r] of a page fills [area] (screen pixels), centred in it. */
    fun fitBox(sheetId: String, r: RectF, area: RectF) {
        val p = model?.placed?.firstOrNull { it.sheetId == sheetId } ?: board?.takeIf { it.sheetId == sheetId } ?: return
        if (area.width() <= 0f || area.height() <= 0f || r.width() <= 0f || r.height() <= 0f) return
        val k = minOf(area.width() / r.width(), area.height() / r.height()).coerceIn(minScale(), maxScale())
        val cx = p.rect.left + r.centerX(); val cy = p.rect.top + r.centerY()
        moveTo(k, area.centerX() - cx * k, area.centerY() - cy * k, animate = true)
    }

    /** The view [c] (as it was), moved sideways only as far as the box [r] needs to clear [left] px. */
    fun showAt(c: FloatArray, sheetId: String, r: RectF, left: Float) {
        val p = model?.placed?.firstOrNull { it.sheetId == sheetId } ?: board?.takeIf { it.sheetId == sheetId } ?: return
        var tx1 = c[1]
        val sx = (p.rect.left + r.left) * c[0] + tx1
        if (sx < left) tx1 += left - sx
        moveTo(c[0], tx1, c[2], animate = true)
    }

    /** The coordinate system that is selected (alone), with its page. */
    fun selectedPlot(): Pair<String, uniffi.n0te_ffi.FfiPlot>? {
        val sel = selection?.takeIf { !it.multi && it.ids.size == 1 } ?: return null
        val plot = model?.sheets?.get(sel.sheetId)?.images?.firstOrNull { it.id in sel.ids }?.plot ?: return null
        return sel.sheetId to plot
    }

    /**
     * Puts a new coordinate system on the page under screen (sx, sy) (null: the page in view,
     * in the middle of the screen) and selects it; [then] gets its page and the plot to edit.
     */
    fun insertPlot(sx: Float?, sy: Float?, then: (String, uniffi.n0te_ffi.FfiPlot) -> Unit) {
        val m = model ?: return
        val wx = toWorldX(sx ?: (width / 2f)); val wy = toWorldY(sy ?: (height / 2f))
        val p = board ?: (if (sx != null) m.placed.firstOrNull { it.rect.contains(wx, wy) } else null) ?: focusedSheet() ?: return
        val cx = (if (p.isBoard) wx else wx.coerceIn(p.rect.left, p.rect.right)) - p.rect.left
        val cy = (if (p.isBoard) wy else wy.coerceIn(p.rect.top, p.rect.bottom)) - p.rect.top
        val plot = Plots.fresh(cx, cy, if (p.isBoard) 0f else p.rect.width(), if (p.isBoard) 0f else p.rect.height(), p.spec.background, p.spec.spacing)
        m.addPlot(p.sheetId, plot) { id ->
            selection = Selection(p.sheetId, setOf(id), RectF(plot.x, plot.y, plot.x + plot.w, plot.y + plot.h))
            selDx = 0f; selDy = 0f; selScale = 1f
            listener?.onSelectionChanged(selection)
            invalidate()
            then(p.sheetId, plot.copy(id = id))
        }
    }

    private fun selectStrokes(sheetId: String, strokes: List<InkStroke>) {
        if (strokes.isEmpty()) { clearSelection(); return }
        val b = RectF(strokes[0].bounds)
        strokes.forEach { b.union(it.bounds) }
        selection = Selection(sheetId, strokes.map { it.id }.toSet(), b)
        selDx = 0f; selDy = 0f; selScale = 1f
        listener?.onSelectionChanged(selection)
        invalidate()
    }

    /**
     * Deletes [sel] (the selection now, or the one a cut copied: a lasso drawn while the copy was
     * on its way is not deleted instead of it).
     */
    fun deleteSelection(sel: Selection? = selection) {
        sel ?: return
        if (selection === sel) clearSelection()
        for (part in sel.parts) model?.deleteStrokes(part.sheetId, part.ids)
    }

    fun recolorSelection(color: Int) {
        val sel = selection ?: return
        for (part in sel.parts) model?.recolor(part.sheetId, part.ids, color)
    }

    /**
     * The selection rendered as a PNG (white background), for the laptop clipboard: all of it,
     * scaled down to fit when it is big ([SelectionPicture]).
     */
    fun selectionPng(done: (ByteArray?) -> Unit) {
        val bmp = selectionBitmap() ?: return done(null)
        // Compressed on the I/O thread: PNG at full quality of up to 16 MP held the main thread for long.
        N0te.background({
            val out = java.io.ByteArrayOutputStream()
            bmp.compress(android.graphics.Bitmap.CompressFormat.PNG, 100, out)
            bmp.recycle()
            out.toByteArray()
        }) { r -> done(r.getOrNull()) }
    }

    /** The selection drawn on white (drawn here: it reads the page's state). */
    private fun selectionBitmap(): android.graphics.Bitmap? {
        val sel = selection ?: return null
        val m = model ?: return null
        // What is there now (the selection's box can lag behind a move or resize), in world points.
        val b = RectF()
        var widest = 0f
        for (part in sel.parts) {
            val p = placedFor(part.sheetId) ?: continue
            val st = m.sheets[part.sheetId] ?: continue
            val r = RectF()
            for (s in st.strokes) if (s.id in part.ids) { if (r.isEmpty) r.set(s.bounds) else r.union(s.bounds); widest = max(widest, s.width) }
            for (im in st.images) if (im.id in part.ids) { if (r.isEmpty) r.set(im.x, im.y, im.x + im.w, im.y + im.h) else r.union(im.x, im.y, im.x + im.w, im.y + im.h) }
            if (r.isEmpty) r.set(part.bounds)
            r.offset(p.rect.left, p.rect.top)
            if (b.isEmpty) b.set(r) else b.union(r)
        }
        if (b.isEmpty) b.set(selUnion(sel))
        val pad = SelectionPicture.margin(widest)
        b.inset(-pad, -pad)
        var size = SelectionPicture.size(b.width(), b.height())
        val bmp = try {
            android.graphics.Bitmap.createBitmap(size.width, size.height, android.graphics.Bitmap.Config.ARGB_8888)
        } catch (_: OutOfMemoryError) {
            size = SelectionPicture.size(b.width(), b.height(), maxPixels = SelectionPicture.MAX_PIXELS / 4)
            android.graphics.Bitmap.createBitmap(size.width, size.height, android.graphics.Bitmap.Config.ARGB_8888)
        }
        val k = size.scale
        val c = Canvas(bmp)
        c.drawColor(Color.WHITE) // the laptop gets a white background whatever the theme
        c.scale(k, k)
        c.translate(-b.left, -b.top)
        for (part in sel.parts) {
            val p = placedFor(part.sheetId) ?: continue
            val st = m.sheets[part.sheetId] ?: continue
            c.save(); c.translate(p.rect.left, p.rect.top)
            // Graphs and pictures in the selection too (under the ink, as on the page).
            for (im in st.images) if (im.id in part.ids) drawBox(c, im)
            for (s in st.strokes) if (s.id in part.ids) drawStroke(c, s)
            c.restore()
        }
        return bmp
    }

    /** The first page as a small picture (library preview), [w] pixels wide; null if empty. */
    fun renderThumbnail(w: Int): android.graphics.Bitmap? {
        val m = model ?: return null
        val p = m.placed.firstOrNull { it.side == 0 } ?: return null
        // Its ink not held (never in view, or let go long out of view): no picture rather than
        // blank paper over the cover kept from before.
        val st = m.sheets[p.sheetId]
        if (st?.handle == null || st.trimmed || st.loading) return null
        return renderPage(m, p, w)
    }

    /**
     * A page of the outline as a small picture, [w] pixels wide (its strokes are loaded if they
     * aren't yet: the picture fills in once they are). Null for a page that isn't laid out (in a
     * folded group) and for boards.
     */
    fun pageThumb(sheetId: String, w: Int): android.graphics.Bitmap? {
        val m = model ?: return null
        val p = m.placed.firstOrNull { it.sheetId == sheetId && !it.isBoard } ?: return null
        m.ensureLoaded(sheetId, p.rect.width())
        return renderPage(m, p, w)
    }

    /** A page as a picture [w] pixels wide: paper, PDF, pictures and graphs, text, ink (a board: its first screen). */
    private fun renderPage(m: NotebookModel, p: Placed, w: Int): android.graphics.Bitmap {
        val r = if (p.isBoard) RectF(0f, 0f, NotebookModel.BOARD_W, NotebookModel.BOARD_H).apply { offset(p.rect.left, p.rect.top) } else p.rect
        val k = w / r.width()
        val bmp = android.graphics.Bitmap.createBitmap(w, (r.height() * k).toInt().coerceIn(1, w * 6), android.graphics.Bitmap.Config.ARGB_8888)
        val c = Canvas(bmp)
        val keep = scale
        scale = k // the paper pattern and grain choose their detail for this size
        try {
            c.scale(k, k)
            c.translate(-r.left, -r.top)
            c.clipRect(r)
            c.drawColor(paperColor)
            drawPaper(c, p, r)
            if (p.spec.pdfBlob != null) PdfTiles.draw(c, p, k, r, ArrayList(), thumbSlot)
            m.sheets[p.sheetId]?.let { st ->
                // Ink and boxes lie in the page's own coordinates.
                c.save()
                c.translate(p.rect.left, p.rect.top)
                for (im in st.images) drawBox(c, im)
                drawStrokes(c, st)
                c.restore()
            }
        } finally {
            scale = keep
        }
        return bmp
    }

    /** Where a sheet is drawn now: the board itself in board mode, else its place in the column. */
    private fun placedFor(sheetId: String): Placed? = board?.takeIf { it.sheetId == sheetId } ?: model?.placed?.firstOrNull { it.sheetId == sheetId }

    fun toWorldX(sx: Float) = (sx - tx) / scale
    fun toWorldY(sy: Float) = (sy - ty) / scale
    fun toScreenX(wx: Float) = wx * scale + tx
    fun toScreenY(wy: Float) = wy * scale + ty

    override fun onSizeChanged(w: Int, h: Int, oldw: Int, oldh: Int) {
        super.onSizeChanged(w, h, oldw, oldh)
        if (!fitted || oldw == 0) fitWidth()
        else {
            // Turned, or the panes resized: the page stays as big as it was, with more or less
            // room around it; a page that fitted still fits (narrower: it shrinks to the width).
            // What was in the middle stays there, what was near the top stays near the top.
            val anchor = 80f * density
            val wx = (oldw / 2f - tx) / scale
            val wy = (anchor - ty) / scale
            val column = NotebookModel.A4_W + 2 * MARGIN_PT
            if (board == null && scale <= oldw / column * 1.02f) scale = minOf(scale, w / column)
            scale = scale.coerceIn(minScale(), maxScale())
            tx = w / 2f - wx * scale
            ty = anchor - wy * scale
            clampTransform()
        }
        // The zoom shown and the text editor over the page follow (rotated, or the panes resized).
        post { listener?.onViewState() }
    }

    // ========================================================================================
    // Drawing
    // ========================================================================================

    /**
     * While a selection is carried (nothing else on the screen moves): everything but it as one
     * picture at screen resolution, recorded as the drag starts, so a frame is that picture and
     * the selection's own (see [SelInk]) however much ink lies around.
     */
    private var still: RenderNode? = null
    private var stillAt = FloatArray(0)
    private var recordingStill = false

    /** Draws a frame of a selection being carried from [still]; false when this frame is drawn as usual. */
    private fun drawCarried(c: Canvas, m: NotebookModel): Boolean {
        if (action != Action.MOVE_SEL || board != null || !c.isHardwareAccelerated || recordingStill) {
            if (!recordingStill) still?.takeIf { it.hasDisplayList() }?.discardDisplayList()
            return false
        }
        val n = still ?: RenderNode("carried").also { still = it }
        val at = floatArrayOf(tx, ty, scale, width.toFloat(), height.toFloat())
        if (!n.hasDisplayList() || !at.contentEquals(stillAt)) {
            n.setPosition(0, 0, width, height)
            n.setUseCompositingLayer(true, null)
            val rc = n.beginRecording(width, height)
            recordingStill = true
            try { onDraw(rc) } finally { recordingStill = false }
            n.endRecording()
            stillAt = at
        }
        c.drawRenderNode(n)
        c.save()
        c.translate(tx, ty)
        c.scale(scale, scale)
        drawLassoAndSelection(c, m)
        c.restore()
        return true
    }

    override fun onDraw(c: Canvas) {
        if (!PerfBench.on) { drawFrame(c); return }
        val t = System.nanoTime()
        drawFrame(c)
        PerfBench.drawNs += System.nanoTime() - t
    }

    private fun drawFrame(c: Canvas) {
        val m = model
        if (m != null && drawCarried(c, m)) return
        c.drawColor(bgColor)
        if (m == null) return
        val texel = Pencil.texelFor(scale)
        if (texel != pencilTexel || Pencil.generation != pencilGeneration) {
            // The pencil grain is baked into the recorded ink: its chunks with pencil strokes are
            // recorded anew as they show (InkChunks compares the grain they were recorded with).
            pencilTexel = texel
            pencilGeneration = Pencil.generation
        }
        frames++
        val now = SystemClock.uptimeMillis()
        if (now - trimAt > 2000) {
            // Pages long out of view let go of their strokes once very many are held.
            trimAt = now
            val keep = HashSet<String>()
            selection?.parts?.forEach { keep += it.sheetId }
            editingText?.let { keep += it.first }
            board?.let { keep += it.sheetId }
            m.trim(keep, now)
        }
        if (scale != lastScale) {
            // Zooming: ink tiles at steps of zoom meanwhile; once it rests, drawn at the zoom itself.
            lastScale = scale
            scaleAt = SystemClock.uptimeMillis()
            postInvalidateDelayed(ZOOM_REST_MS + 20)
        }
        val vis = RectF(toWorldX(0f), toWorldY(0f), toWorldX(width.toFloat()), toWorldY(height.toFloat()))
        val prefetch = RectF(vis).apply { inset(0f, -vis.height()) }
        val b = board
        if (b != null) {
            drawBoard(c, m, b, vis)
            drawCursor(c)
            return
        }
        c.save()
        c.translate(tx, ty)
        c.scale(scale, scale)
        // Right after a structural change (fold, unfold, move, add, delete) pages glide from
        // where they were; new ones fade in from their group's title, gone ones fade into it.
        val af = ((SystemClock.uptimeMillis() - m.layoutAt) / LAYOUT_ANIM_MS).coerceIn(0f, 1f)
        val animating = af < 1f
        val e = 1f - (1f - af) * (1f - af) * (1f - af)
        fun lerp(a: RectF, b: RectF) = RectF(a.left + (b.left - a.left) * e, a.top + (b.top - a.top) * e, a.right + (b.right - a.right) * e, a.bottom + (b.bottom - a.bottom) * e)
        fun headerAbove(r: RectF, hs: Collection<RectF>) = hs.filter { it.top <= r.top }.maxByOrNull { it.top }
        if (Settings.mainColumn && !animating) drawMainColumn(c, m, vis)
        val sd = space
        /** While space goes into a page, everything further down moves with its bottom. */
        fun spaceShift(r: RectF) = if (sd != null && sd.grow != 0f && r.top >= sd.page.rect.bottom - 0.5f) sd.grow else 0f
        /** While space goes into a page sideways, the other pages of its row move with its edges. */
        fun spaceShiftX(p: Placed) = sd?.rowShift(p) ?: 0f
        for (h in m.headers) {
            val r0 = if (animating) m.prevHeaders[h.node]?.let { lerp(it, h.rect) } ?: h.rect else h.rect
            val r = spaceShift(r0).let { if (it != 0f) RectF(r0).apply { offset(0f, it) } else r0 }
            if (RectF.intersects(r, vis)) drawHeader(c, if (r === h.rect) h else Header(h.node, h.title, h.depth, h.collapsed, h.count, r))
        }
        tileRequests.clear()
        for (p0 in m.placed) {
            if (!RectF.intersects(p0.rect, prefetch)) continue
            m.ensureLoaded(p0.sheetId, p0.rect.width())
            var alpha = 1f
            val p = if (!animating) p0 else {
                val from = m.prevRects[p0.sheetId]
                if (from != null) p0.at(lerp(from, p0.rect)) else {
                    // New: slides down out of the title above it.
                    alpha = e
                    val h = headerAbove(p0.rect, m.headers.map { it.rect })
                    val start = RectF(p0.rect).apply { offsetTo(left, (h?.bottom ?: top) - height() * 0.15f) }
                    p0.at(lerp(start, p0.rect))
                }
            }
            if (sd != null && p0.sheetId == sd.page.sheetId) { if (sd.sideways == true) drawSpacedX(c, p, vis, sd) else drawSpaced(c, p, vis, sd); continue }
            val off = spaceShift(p.rect)
            val offX = spaceShiftX(p0)
            val v = if (off != 0f || offX != 0f) RectF(vis).apply { offset(-offX, -off) } else vis
            if (!RectF.intersects(p.rect, v)) continue
            c.save()
            if (off != 0f || offX != 0f) c.translate(offX, off)
            if (alpha < 1f) c.saveLayerAlpha(p.rect.left - 2f, p.rect.top - 2f, p.rect.right + 2f, p.rect.bottom + 2f, (alpha * 255).toInt())
            drawPaper(c, p, v)
            if (p.spec.pdfBlob != null) PdfTiles.draw(c, p, scale, v, tileRequests, pdfSlot)
            m.sheets[p.sheetId]?.let { drawImages(c, p, it); drawInk(c, p, it, v) }
            if (alpha < 1f) c.restore()
            c.restore()
        }
        if (animating) {
            for (g in m.vanished) {
                // Folded away / deleted: slides up into the title above it and fades.
                val h = headerAbove(g.rect, m.prevHeaders.values)
                val end = RectF(g.rect).apply { offsetTo(left, (h?.bottom ?: top) - height() * 0.15f) }
                val p = g.at(lerp(g.rect, end))
                if (!RectF.intersects(p.rect, vis)) continue
                c.saveLayerAlpha(p.rect.left - 2f, p.rect.top - 2f, p.rect.right + 2f, p.rect.bottom + 2f, ((1f - e) * 255).toInt())
                drawPaper(c, p, vis)
                m.sheets[p.sheetId]?.let { drawInk(c, p, it, vis) }
                c.restore()
            }
            postInvalidateOnAnimation()
        }
        PdfTiles.flush(tileRequests.toList())
        // Live stroke when the low-latency layer is unavailable.
        if (action == Action.INK && inkLayer?.active != true) {
            target?.let { t ->
                c.save(); c.translate(t.rect.left, t.rect.top)
                drawLive(c)
                c.restore()
            }
        }
        // Carried, the selection is drawn over the still picture of the rest.
        if (!recordingStill) drawLassoAndSelection(c, m)
        drawFlash(c)
        drawPlotBox(c)
        drawTextFrames(c, m)
        drawRemote(c, m)
        drawRuler(c)
        handle?.let { h ->
            // Preview of the page a dragged "+" creates (while holding: it firms up with the ring).
            val hold = Settings.handleHoldMs
            val a = if (hold > 0) ((SystemClock.uptimeMillis() - handleDownAt) / hold).coerceIn(0.15f, 1f) else (handleDrag / (60f * density)).coerceIn(0.25f, 1f)
            ghostFill.alpha = (0x33 * a).toInt(); ghostLine.alpha = (255 * a).toInt()
            ghostLine.strokeWidth = 1.5f / scale
            ghostLine.pathEffect = DashPathEffect(floatArrayOf(10f / scale, 7f / scale), 0f)
            c.drawRect(h.ghost, ghostFill)
            c.drawRect(h.ghost, ghostLine)
        }
        drawLoupeFrame(c)
        c.restore()
        drawFlowBand(c)
        drawHandles(c)
        drawLoupe(c, m)
        drawLaser(c)
        drawCursor(c)
    }

    private val columnPaint = Paint()

    /**
     * A page while space goes in or out: above the line as it is, below it moved by the space
     * (paper and PDF with it), the page as long as it will be, the new strip tinted with its
     * size beside it.
     */
    private fun drawSpaced(c: Canvas, p: Placed, vis: RectF, sd: SpaceDrag) {
        val m = model ?: return
        val r = p.rect
        if (sd.sideways == null) {
            // Not yet clear which way: the page as it is, a small cross where the pen is.
            drawPaper(c, p, vis)
            m.sheets[p.sheetId]?.let { st -> if (p.spec.pdfBlob != null) PdfTiles.draw(c, p, scale, vis, tileRequests, pdfSlot); drawImages(c, p, st); drawInk(c, p, st, vis) }
            spaceLine.color = Ui.theme.accent.toInt()
            spaceLine.strokeWidth = 1.5f / scale
            spaceLine.pathEffect = DashPathEffect(floatArrayOf(6f / scale, 5f / scale), 0f)
            val arm = 26f * density / scale
            val cx = r.left + sd.x; val cy = r.top + sd.y
            c.drawLine(cx - arm, cy, cx + arm, cy, spaceLine)
            c.drawLine(cx, cy - arm, cx, cy + arm, spaceLine)
            return
        }
        val dy = sd.dy
        val bottom = r.top + (sd.plan?.height ?: p.spec.height)
        val line = r.top + sd.y
        drawPaper(c, p.at(RectF(r.left, r.top, r.right, bottom)), vis)
        val st = m.sheets[p.sheetId]
        fun content(v: RectF) {
            if (p.spec.pdfBlob != null) PdfTiles.draw(c, p, scale, v, tileRequests, pdfSlot)
            st?.let { drawImages(c, p, it); drawInk(c, p, it, v) }
        }
        c.save(); c.clipRect(r.left, r.top, r.right, line); content(vis); c.restore()
        c.save(); c.clipRect(r.left, line + max(dy, 0f), r.right, bottom); c.translate(0f, dy); content(RectF(vis).apply { offset(0f, -dy) }); c.restore()
        val accent = Ui.theme.accent.toInt()
        if (dy > 0f) { spaceFill.color = (accent and 0x00FFFFFF) or 0x1A000000; c.drawRect(r.left, line, r.right, line + dy, spaceFill) }
        spaceLine.color = accent
        spaceLine.strokeWidth = 1.5f / scale
        spaceLine.pathEffect = DashPathEffect(floatArrayOf(8f / scale, 6f / scale), 0f)
        val reach = 14f / scale
        c.drawLine(r.left - reach, line, r.right + reach, line, spaceLine)
        if (dy > 0f) c.drawLine(r.left - reach, line + dy, r.right + reach, line + dy, spaceLine)
        val text = spaceLabel(sd)
        if (text.isNotEmpty()) {
            spaceText.textSize = 13f * density / scale
            val pad = 6f * density / scale
            val tw = spaceText.measureText(text)
            val mid = if (dy > 0f) line + dy / 2f else line - spaceText.textSize
            val box = RectF(r.right - tw - 3 * pad, mid - spaceText.textSize * 0.75f, r.right - pad, mid + spaceText.textSize * 0.75f)
            spaceFill.color = accent
            c.drawRoundRect(box, pad, pad, spaceFill)
            spaceText.color = 0xFFFFFFFF.toInt()
            c.drawText(text, box.left + pad, mid + spaceText.textSize * 0.35f, spaceText)
        }
    }

    /**
     * A page while space goes in or out sideways: left of the line as it is, right of it moved by
     * the space, the page as wide as it will be (and where the layout will put it), the new strip
     * tinted with its size above it.
     */
    private fun drawSpacedX(c: Canvas, p: Placed, vis: RectF, sd: SpaceDrag) {
        val m = model ?: return
        val r = p.rect
        val dx = sd.dx
        val own = sd.ownShift
        val left = r.left + own
        val right = left + (sd.planX?.width ?: p.spec.width)
        val line = left + sd.x
        drawPaper(c, p.at(RectF(left, r.top, right, r.bottom)), vis)
        val st = m.sheets[p.sheetId]
        fun content(shift: Float) {
            c.translate(shift, 0f)
            st?.let { drawImages(c, p, it); drawInk(c, p, it, RectF(vis).apply { offset(-shift, 0f) }) }
        }
        c.save(); c.clipRect(left, r.top, line, r.bottom); content(own); c.restore()
        c.save(); c.clipRect(line + max(dx, 0f), r.top, right, r.bottom); content(own + dx); c.restore()
        val accent = Ui.theme.accent.toInt()
        if (dx > 0f) { spaceFill.color = (accent and 0x00FFFFFF) or 0x1A000000; c.drawRect(line, r.top, line + dx, r.bottom, spaceFill) }
        spaceLine.color = accent
        spaceLine.strokeWidth = 1.5f / scale
        spaceLine.pathEffect = DashPathEffect(floatArrayOf(8f / scale, 6f / scale), 0f)
        val reach = 14f / scale
        c.drawLine(line, r.top - reach, line, r.bottom + reach, spaceLine)
        if (dx > 0f) c.drawLine(line + dx, r.top - reach, line + dx, r.bottom + reach, spaceLine)
        val text = spaceLabel(sd)
        if (text.isNotEmpty()) {
            spaceText.textSize = 13f * density / scale
            val pad = 6f * density / scale
            val tw = spaceText.measureText(text)
            // Over the new strip (or the line), a little above where the pen is.
            val mid = if (dx > 0f) line + dx / 2f else line
            val h = spaceText.textSize * 1.5f
            val top = clampTo(r.top + sd.y - 40f * density / scale - h, r.top + pad, r.bottom - h - pad)
            val box = RectF(mid - tw / 2f - pad, top, mid + tw / 2f + pad, top + h)
            spaceFill.color = accent
            c.drawRoundRect(box, pad, pad, spaceFill)
            spaceText.color = 0xFFFFFFFF.toInt()
            c.drawText(text, box.left + pad, box.centerY() + spaceText.textSize * 0.35f, spaceText)
        }
    }

    /** "+3 Zeilen · Seite +12 mm", "−8 mm", "nichts zu entfernen"; sideways "+2 Kästchen · Seite +10 mm breiter". */
    private fun spaceLabel(sd: SpaceDrag): String {
        if (sd.sideways == true) {
            val dx = sd.dx
            if (dx == 0f) return if (sd.asked < 0f) "nichts zu entfernen" else ""
            val sign = if (dx > 0f) "+" else "−"
            val amount = if (sd.stepX != MM) {
                val n = Math.round(abs(dx) / sd.stepX)
                if (sd.page.spec.background == FfiBackground.GRID) "$n Kästchen" else if (n == 1) "1 Spalte" else "$n Spalten"
            } else "%.0f mm".format(abs(dx) / MM)
            val w = sd.widen
            return "$sign$amount" + if (w > 0.5f) " · Seite +%.0f mm breiter".format(w / MM) else ""
        }
        val dy = sd.dy
        if (dy == 0f) return if (sd.asked < 0f) "nichts zu entfernen" else ""
        val sign = if (dy > 0f) "+" else "−"
        val amount = if (sd.step != MM) {
            val n = Math.round(abs(dy) / sd.step)
            when (sd.page.spec.background) {
                FfiBackground.GRID -> "$n Kästchen"
                FfiBackground.DOTS -> if (n == 1) "1 Reihe" else "$n Reihen"
                else -> if (n == 1) "1 Zeile" else "$n Zeilen"
            }
        } else "%.0f mm".format(abs(dy) / MM)
        val grow = sd.grow
        val page = when { grow > 0.5f -> " · Seite +%.0f mm".format(grow / MM); grow < -0.5f -> " · Seite −%.0f mm".format(-grow / MM); else -> "" }
        return "$sign$amount$page"
    }

    /** The pen lifted after dragging space: it goes in (or out) for good, as one undo step. */
    private fun finishSpace(m: NotebookModel?, commit: Boolean) {
        val sd = space ?: return
        if (sd.sideways != false) { finishSpaceX(m, commit, sd); return }
        val plan = sd.plan
        if (m == null || !commit || plan == null || (plan.dy == 0f && sd.grow == 0f)) {
            if (commit && plan != null && sd.asked < 0f && plan.dy == 0f)
                listener?.onMessage(if (sd.page.spec.pdfBlob != null) "Hier ist kein eingefügter Platz zum Entfernen" else "Darunter ist nichts, das hochrutschen könnte")
            space = null
            invalidate()
            return
        }
        val text = spaceLabel(sd)
        m.insertSpace(sd.page, sd.y, plan) { changed ->
            if (space === sd) space = null
            if (changed) listener?.onMessage((if (plan.dy > 0f) "Platz eingefügt: " else "Platz entfernt: ") + text.drop(1))
            invalidate()
        }
    }

    /** The pen lifted after dragging space sideways (or before it was clear which way). */
    private fun finishSpaceX(m: NotebookModel?, commit: Boolean, sd: SpaceDrag) {
        val plan = sd.planX
        if (m == null || !commit || sd.sideways != true || plan == null || (plan.dx == 0f && sd.widen == 0f)) {
            if (commit && sd.sideways == true && plan != null && sd.asked < 0f && plan.dx == 0f) listener?.onMessage("Rechts davon ist nichts, das herüberrutschen könnte")
            space = null
            invalidate()
            return
        }
        val text = spaceLabel(sd)
        m.insertSpaceX(sd.page, sd.x, plan) { changed ->
            if (space === sd) space = null
            if (changed) listener?.onMessage((if (plan.dx > 0f) "Platz eingefügt: " else "Platz entfernt: ") + text.drop(1))
            invalidate()
        }
    }

    /**
     * The main column, clearly: a soft band behind the main pages (only in notebooks with side
     * pages), so the pages beside it read as side pages.
     */
    private fun drawMainColumn(c: Canvas, m: NotebookModel, vis: RectF) {
        if (m.placed.none { it.side != 0 }) return
        val mains = m.placed.filter { it.side == 0 && !it.isBoard }
        if (mains.isEmpty()) return
        columnPaint.color = Ui.theme.fg.toInt()
        columnPaint.alpha = if (Ui.theme.dark) 0x10 else 0x0B
        val pad = NotebookModel.SIDE_GAP * 0.45f
        val gap = NotebookModel.ROW_GAP / 2f
        for (p in mains) {
            val band = RectF(p.rect.left - pad, p.rect.top - gap, p.rect.right + pad, p.rect.bottom + gap)
            if (RectF.intersects(band, vis)) c.drawRect(band, columnPaint)
        }
    }

    /**
     * The laser trail (kept in page coordinates, so it stays on the content while scrolling):
     * short — the last moment of the pen's path; long — seconds; keep — all of it while the pen
     * is down. Each pen-down is a segment that fades after the pen lifts.
     */
    private fun drawLaser(c: Canvas) {
        if (laser.isEmpty()) return
        val now = since(SystemClock.uptimeMillis())
        val (trail, fadeMs) = when (Settings.laserTrail) {
            "long" -> 4000f to 1500f
            "keep" -> Float.MAX_VALUE to 2000f
            else -> LASER_TRAIL_MS to LASER_FADE_MS
        }
        val active = if (action == Action.LASER) laserSeg else -1
        fun alphaOf(seg: Int): Float = if (seg == active) 1f else laserEnded[seg]?.let { (1f - (now - since(it)) / fadeMs).coerceIn(0f, 1f) } ?: 0f
        laser.removeAll { now - it[2] > trail || alphaOf(it[3].toInt()) <= 0f }
        laserEnded.keys.retainAll(laser.map { it[3].toInt() }.toSet())
        if (laser.isEmpty()) return
        var i = 0
        while (i < laser.size) {
            val seg = laser[i][3].toInt()
            val path = Path()
            var j = i
            while (j < laser.size && laser[j][3].toInt() == seg) {
                val sx = laser[j][0] * scale + tx; val sy = laser[j][1] * scale + ty
                if (j == i) path.moveTo(sx, sy) else path.lineTo(sx, sy)
                j++
            }
            val fade = alphaOf(seg)
            laserGlow.strokeWidth = 14f * density; laserGlow.alpha = (0x55 * fade).toInt()
            laserCore.strokeWidth = 4f * density; laserCore.alpha = (255 * fade).toInt()
            c.drawPath(path, laserGlow)
            c.drawPath(path, laserCore)
            if (seg == active) {
                val last = laser[j - 1]
                laserCore.style = Paint.Style.FILL
                c.drawCircle(last[0] * scale + tx, last[1] * scale + ty, 5f * density, laserCore)
                laserCore.style = Paint.Style.STROKE
            }
            i = j
        }
        postInvalidateOnAnimation()
    }

    /** The ring around a held "+" closed: add the page now (pen still down), with a tick. */
    private fun confirmHandle() {
        val h = handle ?: return
        if (action != Action.HANDLE) return
        handle = null
        performHapticFeedback(if (android.os.Build.VERSION.SDK_INT >= 30) android.view.HapticFeedbackConstants.CONFIRM else android.view.HapticFeedbackConstants.LONG_PRESS)
        listener?.onAddPage(h.dir, h.mainNode)
        invalidate()
    }

    private fun drawHandles(c: Canvas) {
        val r = HANDLE_R * density
        handlePlus.strokeWidth = 2f * density
        // Quiet, in the design's colours: they are there when needed, not a black dot on every page.
        handleFill.color = Ui.theme.surface.toInt()
        handleRing.color = Ui.theme.border.toInt()
        handlePlus.color = Ui.theme.muted.toInt()
        val hold = Settings.handleHoldMs
        for (h in handles()) {
            val sx = h.wx * scale + tx; val sy = h.wy * scale + ty
            if (sx < -r || sx > width + r || sy < -r || sy > height + r) continue
            val active = handle?.dir == h.dir
            if (active && hold > 0) {
                // The confirmation ring: fills clockwise while held.
                val p = ((SystemClock.uptimeMillis() - handleDownAt) / hold).coerceIn(0f, 1f)
                val rr = r + 6f * density
                holdTrack.strokeWidth = 4f * density; holdTrack.color = Ui.theme.accent.toInt(); holdTrack.alpha = 0x40
                holdRing.strokeWidth = 4f * density; holdRing.color = Ui.theme.accent.toInt()
                c.drawCircle(sx, sy, rr, holdTrack)
                c.drawArc(sx - rr, sy - rr, sx + rr, sy + rr, -90f, 360f * p, false, holdRing)
                if (p < 1f) postInvalidateOnAnimation()
            }
            c.drawCircle(sx, sy, r, if (active && hold <= 0) handlePaint else handleFill)
            c.drawCircle(sx, sy, r, handleRing)
            val k = r * 0.42f
            c.drawLine(sx - k, sy, sx + k, sy, handlePlus)
            c.drawLine(sx, sy - k, sx, sy + k, handlePlus)
        }
    }

    /** Full-screen board: white paper everywhere, dots in view, ink unclipped, minimap. */
    private fun drawBoard(c: Canvas, m: NotebookModel, b: Placed, vis: RectF) {
        c.drawColor(paperColor)
        c.save()
        c.translate(tx, ty)
        c.scale(scale, scale)
        val step = b.spec.spacing.takeIf { it >= 4f } ?: 24f
        drawPattern(c, FfiBackground.DOTS, RectF(0f, 0f, 0f, 0f), vis, step)
        m.ensureLoaded(b.sheetId)
        // Pictures and graphs under the ink, as in the column (a board shown alone had none: they
        // showed only while selected, and touches on the invisible graphs were taken).
        m.sheets[b.sheetId]?.let { drawImages(c, b, it); drawInk(c, b, it, vis) }
        if (action == Action.INK && inkLayer?.active != true) {
            drawLive(c)
        }
        drawLassoAndSelection(c, m)
        drawPlotBox(c)
        drawRuler(c)
        c.restore()
        drawLaser(c)
        if (Settings.minimap && SystemClock.uptimeMillis() < miniUntil) m.sheets[b.sheetId]?.let { drawMinimap(c, it, vis) }
    }

    /** The board's ink as the minimap shows it (recorded once while the minimap stays the same), its bounds. */
    private val miniNode = RenderNode("minimap").apply { setClipToBounds(false) }
    /** The minimap's scale when its rectangles were recorded (0: not yet). */
    private var miniK = 0f
    private val miniContent = RectF()
    private var miniOf: SheetState? = null
    private var miniVersion = -1

    private fun drawMinimap(c: Canvas, st: SheetState, vis: RectF) {
        if (miniOf !== st || miniVersion != st.version) {
            miniContent.setEmpty()
            for (s in st.strokes) if (miniContent.isEmpty) miniContent.set(s.bounds) else miniContent.union(s.bounds)
            miniOf = st; miniVersion = st.version; miniK = 0f
        }
        val content = miniContent
        val all = RectF(vis).apply { if (!content.isEmpty) union(content) }
        val mw = 200f * density; val mh = 140f * density
        val k = min(mw / all.width(), mh / all.height())
        val ox = width - mw - 16f * density; val oy = 16f * density + 64f * density
        c.drawRoundRect(ox - 8f, oy - 8f, ox + mw + 8f, oy + mh + 8f, 14f, 14f, miniBg)
        // Page points to minimap pixels: mx(x) = x · k + dx.
        val dx = ox + (mw - all.width() * k) / 2 - all.left * k
        val dy = oy + (mh - all.height() * k) / 2 - all.top * k
        fun mx(x: Float) = x * k + dx
        fun my(y: Float) = y * k + dy
        // A rectangle per stroke, at least 1.5 px a side; recorded in page points once and then
        // only moved and scaled (again when the scale changed by a tenth or the ink changed), not
        // drawn stroke by stroke on every frame of a pan or zoom.
        if (c.isHardwareAccelerated) {
            if (!miniNode.hasDisplayList() || miniK == 0f || abs(k / miniK - 1f) > 0.1f) {
                val rc = miniNode.beginRecording()
                val m = 1.5f / k
                for (s in st.strokes) rc.drawRect(s.bounds.left, s.bounds.top, max(s.bounds.right, s.bounds.left + m), max(s.bounds.bottom, s.bounds.top + m), miniInk)
                miniNode.endRecording()
                miniK = k
            }
            c.save()
            c.translate(dx, dy)
            c.scale(k, k)
            c.drawRenderNode(miniNode)
            c.restore()
        } else {
            for (s in st.strokes) c.drawRect(mx(s.bounds.left), my(s.bounds.top), max(mx(s.bounds.right), mx(s.bounds.left) + 1.5f), max(my(s.bounds.bottom), my(s.bounds.top) + 1.5f), miniInk)
        }
        c.drawRect(mx(vis.left), my(vis.top), mx(vis.right), my(vis.bottom), miniView)
    }

    private fun drawRuler(c: Canvas) {
        if (!rulerOn) return
        c.save()
        c.translate(rulerCx, rulerCy)
        c.rotate(Math.toDegrees(rulerAngle.toDouble()).toFloat())
        c.drawRect(-RULER_LEN / 2, -RULER_W / 2, RULER_LEN / 2, RULER_W / 2, rulerPaint)
        rulerEdge.strokeWidth = 1.5f / scale
        c.drawRect(-RULER_LEN / 2, -RULER_W / 2, RULER_LEN / 2, RULER_W / 2, rulerEdge)
        // Ticks every 1 mm (2.835 pt), longer at 5 mm and 1 cm.
        rulerTick.strokeWidth = max(0.5f, 1f / scale)
        val mm = 72f / 25.4f
        var i = 0
        var x = -RULER_LEN / 2
        while (x <= RULER_LEN / 2) {
            val len = when { i % 10 == 0 -> 14f; i % 5 == 0 -> 9f; else -> 5f }
            if (mm * scale >= 3f || i % 10 == 0) {
                c.drawLine(x, -RULER_W / 2, x, -RULER_W / 2 + len, rulerTick)
                c.drawLine(x, RULER_W / 2, x, RULER_W / 2 - len, rulerTick)
            }
            x += mm; i++
        }
        labelPaint.textSize = 9f
        labelPaint.color = 0xE6F2F2F4.toInt()
        c.drawText("%.0f°".format(((Math.toDegrees(rulerAngle.toDouble()) % 360 + 360) % 360)), -10f, 4f, labelPaint)
        labelPaint.color = 0xFF787880.toInt()
        c.restore()
    }

    /** Group title: fold button (left), title and page count, "⋯" for the group menu (right). */
    private fun drawHeader(c: Canvas, h: Header) {
        val cy = h.rect.centerY()
        val bx = h.rect.left + FOLD_W / 2
        c.drawCircle(bx, cy, 11f, foldFill)
        // Chevron: right when folded, down when open.
        val k = 4f
        val chevron = Path().apply {
            if (h.collapsed) { moveTo(bx - k * 0.6f, cy - k); lineTo(bx + k * 0.6f, cy); lineTo(bx - k * 0.6f, cy + k) }
            else { moveTo(bx - k, cy - k * 0.5f); lineTo(bx, cy + k * 0.6f); lineTo(bx + k, cy - k * 0.5f) }
        }
        c.drawPath(chevron, foldStroke)
        headerPaint.textSize = 15f
        labelPaint.textSize = 11f
        val title = h.title.ifBlank { "Gruppe" }
        val tx0 = h.rect.left + FOLD_W + 6f
        c.drawText(title, tx0, cy + 5f, headerPaint)
        c.drawText("${h.count}", tx0 + headerPaint.measureText(title) + 10f, cy + 4f, labelPaint)
        for (i in -1..1) c.drawCircle(h.rect.right - 14f + i * 6f, cy, 1.8f, labelPaint)
        c.drawRect(h.rect.left, h.rect.bottom - 1f, h.rect.right, h.rect.bottom, headerLine)
    }

    /** Fold button folds; anywhere else on the title opens the group menu. */
    private fun headerTap(h: Header, wx: Float, sx: Float, sy: Float) {
        if (readOnly) return
        if (wx < h.rect.left + FOLD_W + 4f) model?.setCollapsed(h.node, !h.collapsed)
        else listener?.onGroupMenu(h, sx, sy)
    }

    /**
     * A paper ruling inside [area] (world), aligned to [page]. Batched into one drawLines /
     * drawPoints call, with fewer dots when zoomed out, so it stays cheap at any zoom.
     */
    private fun drawPattern(c: Canvas, kind: FfiBackground, page: RectF, area: RectF, step0: Float) {
        // In the magnifier: hairlines at its own scale (the page view's would be drawn several px wide).
        val k = if (loupeDrawScale > 0f) loupeDrawScale else scale
        val lw = if (loupeDrawScale > 0f) 1.2f / k else max(0.5f, 1f / scale)
        patternPaint.strokeWidth = lw
        fun need(n: Int) { if (patternBuf.size < n) patternBuf = FloatArray(n + n / 2) }
        when (kind) {
            FfiBackground.LINES, FfiBackground.CORNELL -> {
                if (step0 * k < 4f) return
                val first = if (kind == FfiBackground.CORNELL) page.top + 90f else page.top + step0 * 2
                val last = if (kind == FfiBackground.CORNELL) page.bottom - 150f else page.bottom
                var y = first + kotlin.math.ceil(((area.top - first) / step0).coerceAtLeast(0f)) * step0
                var n = 0
                need(((area.height() / step0).toInt() + 2) * 4)
                while (y <= area.bottom && y < last) { patternBuf[n++] = area.left; patternBuf[n++] = y; patternBuf[n++] = area.right; patternBuf[n++] = y; y += step0 }
                c.drawLines(patternBuf, 0, n, patternPaint)
                if (kind == FfiBackground.CORNELL) {
                    marginPaint.strokeWidth = lw * 1.5f
                    c.drawLine(page.left + 150f, page.top + 70f, page.left + 150f, page.bottom - 150f, marginPaint)
                    c.drawLine(page.left, page.bottom - 150f, page.right, page.bottom - 150f, marginPaint)
                    c.drawLine(page.left, page.top + 70f, page.right, page.top + 70f, marginPaint)
                }
            }
            FfiBackground.GRID -> {
                if (step0 * k < 4f) return
                var n = 0
                need(((area.width() / step0).toInt() + (area.height() / step0).toInt() + 4) * 4)
                var x = page.left + kotlin.math.ceil(((area.left - page.left) / step0).coerceAtLeast(0f)) * step0
                if (x <= page.left) x += step0
                while (x <= area.right) { patternBuf[n++] = x; patternBuf[n++] = area.top; patternBuf[n++] = x; patternBuf[n++] = area.bottom; x += step0 }
                var y = page.top + kotlin.math.ceil(((area.top - page.top) / step0).coerceAtLeast(0f)) * step0
                if (y <= page.top) y += step0
                while (y <= area.bottom) { patternBuf[n++] = area.left; patternBuf[n++] = y; patternBuf[n++] = area.right; patternBuf[n++] = y; y += step0 }
                c.drawLines(patternBuf, 0, n, patternPaint)
            }
            FfiBackground.DOTS -> {
                var step = step0
                while (step * scale < 16f) step *= 2f // zoomed out: every 2nd/4th… dot
                dotPaint.strokeWidth = max(1.8f, 2.6f / scale)
                val ox = if (page.left.isInfinite()) 0f else page.left
                val oy = if (page.top.isInfinite()) 0f else page.top
                val x0 = ox + kotlin.math.floor((area.left - ox) / step) * step
                val y0 = oy + kotlin.math.floor((area.top - oy) / step) * step
                val nx = ((area.right - x0) / step).toInt() + 2
                val ny = ((area.bottom - y0) / step).toInt() + 2
                need(nx * ny * 2)
                var n = 0
                for (i in 0 until nx) {
                    val x = x0 + i * step
                    if (x < area.left || x > area.right) continue
                    for (j in 0 until ny) {
                        val y = y0 + j * step
                        if (y < area.top || y > area.bottom) continue
                        patternBuf[n++] = x; patternBuf[n++] = y
                    }
                }
                c.drawPoints(patternBuf, 0, n, dotPaint)
            }
            FfiBackground.BLANK, FfiBackground.TEXT -> {}
        }
    }

    private fun drawPaper(c: Canvas, p: Placed, vis: RectF) {
        val r = p.rect
        c.drawRect(r.left - 1f, r.top - 1f, r.right + 1f, r.bottom + 1f, shadowPaint)
        c.drawRect(r, paperPaint)
        val step = p.spec.spacing.takeIf { it >= 4f } ?: 20f
        val kind = if (p.isBoard) FfiBackground.DOTS else if (p.spec.pdfBlob != null) FfiBackground.BLANK else p.spec.background
        val area = RectF(max(r.left, vis.left), max(r.top, vis.top), min(r.right, vis.right), min(r.bottom, vis.bottom))
        if (area.width() > 0 && area.height() > 0) drawPattern(c, kind, r, area, step)
        labelPaint.textSize = 10f
        if (Settings.pageNumbers) {
            // Main pages: number left of the page; side pages: "3 L1" above their corner.
            if (p.side == 0) c.drawText(p.label, r.left - 6f - labelPaint.measureText(p.label), r.top + 10f, labelPaint)
            else c.drawText(p.label, if (p.side < 0) r.left else r.right - labelPaint.measureText(p.label), r.top - 5f, labelPaint)
            if (p.isBoard) c.drawText("BOARD", r.right - 6f - labelPaint.measureText("BOARD"), r.top - 5f, labelPaint)
        }
    }

    /**
     * A page's text boxes and ink (canvas in world coordinates). With [vis] (world) only the ink
     * in view is drawn, with [tiles] (the page view itself, not the magnifier) from tiles when
     * very much is in view: see [InkChunks].
     */
    private fun drawInk(c: Canvas, p: Placed, st: SheetState, vis: RectF? = null, tiles: Boolean = true) {
        val w = p.rect.width()
        val h = p.rect.height()
        val unbounded = board?.sheetId == p.sheetId
        c.save()
        c.translate(p.rect.left, p.rect.top)
        if (!unbounded) c.clipRect(0f, 0f, w, h)
        // Area eraser: hide what it has covered so far until the core has cut the strokes.
        eraseClip?.let { if (eraseSheet == st.id) c.clipOutPath(it) }
        val local = vis?.let { v ->
            RectF(v).apply { offset(-p.rect.left, -p.rect.top); if (!unbounded && !intersect(0f, 0f, w, h)) setEmpty() }
        }
        if (c.isHardwareAccelerated) {
            val t0 = if (PerfBench.on) System.nanoTime() else 0L
            val texel = pencilTexel.takeIf { it > 0f } ?: Pencil.texelFor(scale)
            st.seenAt = SystemClock.uptimeMillis()
            val ink = st.inkFor(this)
            val zooming = SystemClock.uptimeMillis() - scaleAt < ZOOM_REST_MS
            val more = ink.draw(c, st, local, texel, Pencil.generation, editingText?.takeIf { it.first == st.id }?.second, tiles, frames, zooming) { rc, s -> drawStroke(rc, s) }
            if (more) postInvalidateOnAnimation()
            if (PerfBench.on && ink.recorded > 0) { PerfBench.recordNs += System.nanoTime() - t0; PerfBench.recorded += ink.recorded }
        } else {
            drawStrokes(c, st, local)
        }
        c.restore()
    }

    /** Text boxes and ink straight onto [c] (pictures of pages); with [vis] (page points) only what is in it. */
    private fun drawStrokes(c: Canvas, st: SheetState, vis: RectF? = null) {
        if (st.texts.isNotEmpty()) TextBoxes.draw(c, st.texts, skip = editingText?.takeIf { it.first == st.id }?.second)
        for (s in st.strokes) {
            if (s.id in st.hidden || s.id in st.floating) continue
            if (vis != null && !RectF.intersects(s.bounds, vis)) continue
            drawStroke(c, s)
        }
    }

    private fun drawStroke(c: Canvas, s: InkStroke) {
        if (s.kind == KIND_PENCIL) {
            Pencil.draw(c, Pencil.paints(pencilTexel.takeIf { it > 0f } ?: Pencil.texelFor(scale)), s.path, s.layerPaths, s.color)
        } else {
            inkPaint.color = s.color
            c.drawPath(s.path, inkPaint)
        }
    }

    /** The live stroke without the low-latency layer (canvas in sheet coordinates). */
    private fun drawLive(c: Canvas) {
        if (brush.kind == KIND_PENCIL.toUByte()) {
            Pencil.draw(c, Pencil.paints(Pencil.texelFor(scale)), livePath, liveLayerPaths, liveColor)
        } else {
            livePaint.color = liveColor
            c.drawPath(livePath, livePaint)
        }
    }

    private fun drawLassoAndSelection(c: Canvas, m: NotebookModel) {
        if (action == Action.LASSO && lasso.size >= 4) {
            val path = Path()
            path.moveTo(lasso[0], lasso[1])
            var i = 2
            while (i + 1 < lasso.size) { path.lineTo(lasso[i], lasso[i + 1]); i += 2 }
            if (Settings.selectMode != "free") path.close()
            lassoPaint.strokeWidth = (if (Settings.selectMode == "snip") 2f else 1.5f) / scale
            lassoPaint.pathEffect = DashPathEffect(floatArrayOf(8f / scale, 6f / scale), 0f)
            c.drawPath(path, lassoPaint)
        }
        val sel = selection ?: return
        if (sel.multi) { drawMultiSelection(c, m, sel); return }
        val p = placedFor(sel.sheetId) ?: return
        val st = m.sheets[sel.sheetId]
        // One stroke: fetch its ends once (the handles to change length and angle).
        if (sel.ids.size == 1 && selEndsFor !== sel) {
            selEndsFor = sel; selEnds = null
            // Only a straight line gets handles at its ends (length and angle); anything else,
            // a character say, is resized at its box and moved from inside it.
            val len = st?.strokes?.firstOrNull { it.id == sel.ids.first() }?.ends?.getOrNull(8)
            m.strokeEnds(sel.sheetId, sel.ids.first()) { e ->
                val chord = e?.let { hypot(it[2] - it[0], it[3] - it[1]) } ?: 0f
                if (selection === sel && e != null && LineGrip.straight(chord, len)) { selEnds = e; invalidate() }
            }
        }
        if (sel.ids.size != 1) selEnds = null
        if (action == Action.END_SEL && st != null) {
            val e = selEnds
            if (e != null) {
                // Preview: the stroke turned and stretched about the other end.
                val ax = if (endWhich == 0) e[2] else e[0]; val ay = if (endWhich == 0) e[3] else e[1]
                val ox = (if (endWhich == 0) e[0] else e[2]) - ax; val oy = (if (endWhich == 0) e[1] else e[3]) - ay
                val nx = endX - ax; val ny = endY - ay
                val k = hypot(nx, ny) / hypot(ox, oy).coerceAtLeast(0.01f)
                val deg = Math.toDegrees((kotlin.math.atan2(ny, nx) - kotlin.math.atan2(oy, ox)).toDouble()).toFloat()
                c.save()
                c.translate(p.rect.left + ax, p.rect.top + ay)
                c.rotate(deg); c.scale(k, k)
                c.translate(-ax, -ay)
                for (s in st.strokes) if (s.id in sel.ids) drawStroke(c, s)
                c.restore()
                c.drawCircle(p.rect.left + ax, p.rect.top + ay, HANDLE_PX / scale, handlePaint)
                c.drawCircle(p.rect.left + endX, p.rect.top + endY, HANDLE_PX * 1.3f / scale, handlePaint)
                return
            }
        }
        if (st != null) {
            // Exactly the selected strokes get a red outline; while moving they are drawn here,
            // unclipped, so they can be carried over to another page.
            c.save()
            c.translate(p.rect.left, p.rect.top)
            c.concat(selMatrix(sel))
            drawSelectedInk(c, st, sel.ids, 4.5f / scale / selScale)
            c.restore()
        }
        selPaint.strokeWidth = 1f / scale
        selPaint.alpha = 0x70
        selPaint.pathEffect = DashPathEffect(floatArrayOf(6f / scale, 6f / scale), 0f)
        if (action == Action.GRIP_SEL) {
            // The box as it turns or stretches.
            val r = RectF(sel.bounds).apply { inset(-selPad(), -selPad()) }
            val q = floatArrayOf(r.left, r.top, r.right, r.top, r.right, r.bottom, r.left, r.bottom)
            selMatrix(sel).mapPoints(q)
            val path = Path().apply { moveTo(q[0], q[1]); lineTo(q[2], q[3]); lineTo(q[4], q[5]); lineTo(q[6], q[7]); close(); offset(p.rect.left, p.rect.top) }
            c.drawPath(path, selPaint)
            selPaint.alpha = 0xFF
            if (selGrip == 8) c.drawCircle(p.rect.left + gripAx, p.rect.top + gripAy, HANDLE_PX * 0.5f / scale, handlePaint)
            return
        }
        val b = selectionRectWorld(sel)
        c.drawRect(b, selPaint)
        selPaint.alpha = 0xFF
        if (selEnds == null && action != Action.MOVE_SEL) {
            // Corners scale evenly, sides stretch one way, the knob above turns.
            val gs = grips(sel, p)
            gs[8]?.let { g -> c.drawLine(b.centerX(), b.top, g.x, g.y, selPaint) }
            for ((i, g) in gs.withIndex()) {
                if (g == null) continue
                if (i == 8) { c.drawCircle(g.x, g.y, HANDLE_PX * 1.05f / scale, endRing); c.drawCircle(g.x, g.y, HANDLE_PX * 0.55f / scale, handlePaint) }
                else c.drawCircle(g.x, g.y, (if (i < 4) HANDLE_PX else HANDLE_PX * 0.75f) / scale, handlePaint)
            }
        }
        // Start and end of a single stroke: drag one to change length and angle.
        val e = selEnds
        if (e != null && action != Action.MOVE_SEL && action != Action.SCALE_SEL) {
            for (i in 0..1) {
                val hx = p.rect.left + e[2 * i]; val hy = p.rect.top + e[2 * i + 1]
                c.drawCircle(hx, hy, HANDLE_PX * 1.15f / scale, endRing)
                c.drawCircle(hx, hy, HANDLE_PX * 0.55f / scale, handlePaint)
            }
        }
    }

    private val endRing = Paint(Paint.ANTI_ALIAS_FLAG).apply { color = 0xFFFFFFFF.toInt(); style = Paint.Style.FILL; setShadowLayer(3f, 0f, 1f, 0x55000000) }

    /**
     * A selection's look on one page: the red outline of what is selected and, while it moves,
     * the strokes and pictures themselves. Recorded once (per selection, zoom and change of the
     * page) at screen resolution into a layer of its own, which a frame then only moves, scales
     * or turns: thousands of selected strokes cost a frame what one picture does.
     */
    private class SelInk(val node: RenderNode) {
        var version = -1
        var ids: Set<String>? = null
        var floating: Set<String>? = null
        var halo = 0f
        var zoom = 0f
        var texel = 0f
    }
    private val selInk = HashMap<String, SelInk>()

    /** [ids] of [st] drawn as selected (canvas in page points, the selection's move applied). */
    private fun drawSelectedInk(c: Canvas, st: SheetState, ids: Set<String>, halo: Float) {
        if (!c.isHardwareAccelerated) { recordSelectedInk(c, st, ids, halo); return }
        val e = selInk.getOrPut(st.id) { SelInk(RenderNode("selection-${st.id}")) }
        // Recorded in screen pixels (a layer drawn at the page's zoom stays sharp).
        val z = scale
        if (!e.node.hasDisplayList() || e.version != st.version || e.ids !== ids || e.floating !== st.floating || e.halo != halo || e.zoom != z || e.texel != pencilTexel) {
            val b = RectF()
            for (s in st.strokes) if (s.id in ids) { if (b.isEmpty) b.set(s.bounds) else b.union(s.bounds) }
            for (im in st.images) if (im.id in ids) { if (b.isEmpty) b.set(im.x, im.y, im.x + im.w, im.y + im.h) else b.union(im.x, im.y, im.x + im.w, im.y + im.h) }
            b.inset(-halo - 2f, -halo - 2f)
            val l = kotlin.math.floor(b.left * z).toInt(); val t = kotlin.math.floor(b.top * z).toInt()
            val r = kotlin.math.ceil(b.right * z).toInt(); val bt = kotlin.math.ceil(b.bottom * z).toInt()
            e.node.setPosition(l, t, r, bt)
            // A layer only up to a size the GPU takes easily (else the strokes are drawn each frame).
            e.node.setUseCompositingLayer(r - l <= SEL_LAYER_MAX && bt - t <= SEL_LAYER_MAX && (r - l).toLong() * (bt - t) <= SEL_LAYER_PIXELS, null)
            val rc = e.node.beginRecording()
            rc.translate(-l.toFloat(), -t.toFloat())
            rc.scale(z, z)
            recordSelectedInk(rc, st, ids, halo)
            e.node.endRecording()
            e.version = st.version; e.ids = ids; e.floating = st.floating; e.halo = halo; e.zoom = z; e.texel = pencilTexel
        }
        c.save()
        c.scale(1f / z, 1f / z)
        c.drawRenderNode(e.node)
        c.restore()
    }

    private fun recordSelectedInk(c: Canvas, st: SheetState, ids: Set<String>, halo: Float) {
        haloPaint.strokeWidth = halo
        for (im in st.images) if (im.id in st.floating) drawBox(c, im)
        for (im in st.images) if (im.id in ids) c.drawRect(im.x, im.y, im.x + im.w, im.y + im.h, haloPaint)
        for (s in st.strokes) if (s.id in ids) c.drawPath(s.path, haloPaint)
        if (st.floating.isNotEmpty()) for (s in st.strokes) if (s.id in st.floating) drawStroke(c, s)
    }

    /** The selected parts' bounds together (world coordinates, without a running move). */
    private fun selUnion(sel: Selection): RectF {
        val u = RectF()
        for (part in sel.parts) {
            val p = placedFor(part.sheetId) ?: continue
            val r = RectF(part.bounds).apply { offset(p.rect.left, p.rect.top) }
            if (u.isEmpty) u.set(r) else u.union(r)
        }
        return u
    }

    /** A selection over several pages: each part outlined (and drawn while moving), one box around all. */
    private fun drawMultiSelection(c: Canvas, m: NotebookModel, sel: Selection) {
        val u = selUnion(sel)
        for (part in sel.parts) {
            val p = placedFor(part.sheetId) ?: continue
            val st = m.sheets[part.sheetId] ?: continue
            c.save()
            c.translate(p.rect.left + selDx, p.rect.top + selDy)
            // Scaled about the box's top-left, the same point for every page.
            c.scale(selScale, selScale, u.left - p.rect.left, u.top - p.rect.top)
            drawSelectedInk(c, st, part.ids, 4.5f / scale / selScale)
            c.restore()
        }
        val b = selectionRectWorld(sel)
        selPaint.strokeWidth = 1f / scale
        selPaint.alpha = 0x70
        selPaint.pathEffect = DashPathEffect(floatArrayOf(6f / scale, 6f / scale), 0f)
        c.drawRect(b, selPaint)
        selPaint.alpha = 0xFF
        c.drawCircle(b.right, b.bottom, HANDLE_PX / scale, handlePaint)
    }

    /**
     * The grips of a one-page selection (world coordinates): corners, sides, and above the top a
     * knob to turn it (only when it is all strokes: images don't turn). Null where none is.
     */
    private fun grips(sel: Selection, p: Placed): List<PointF?> {
        val b = RectF(sel.bounds).apply { inset(-selPad(), -selPad()); offset(p.rect.left, p.rect.top) }
        val st = model?.sheets?.get(sel.sheetId)
        val turns = st != null && st.images.none { it.id in sel.ids }
        // Sides only where the box is big enough on screen to grab them apart from the corners.
        val wide = b.width() * scale > 70f * density; val tall = b.height() * scale > 70f * density
        return listOf(
            PointF(b.left, b.top), PointF(b.right, b.top), PointF(b.right, b.bottom), PointF(b.left, b.bottom),
            if (wide) PointF(b.centerX(), b.top) else null, if (tall) PointF(b.right, b.centerY()) else null,
            if (wide) PointF(b.centerX(), b.bottom) else null, if (tall) PointF(b.left, b.centerY()) else null,
            if (turns) PointF(b.centerX(), b.top - 30f * density / scale) else null,
        )
    }

    /** A grip taken: what stays where it is while it moves. */
    private fun startGrip(sel: Selection, p: Placed, g: Int) {
        val b = sel.bounds
        selGrip = g; selKx = 1f; selKy = 1f; selAng = 0f; selTurnStep = 0
        when (g) {
            0 -> { gripAx = b.right; gripAy = b.bottom }
            1 -> { gripAx = b.left; gripAy = b.bottom }
            2 -> { gripAx = b.left; gripAy = b.top }
            3 -> { gripAx = b.right; gripAy = b.top }
            4 -> { gripAx = b.centerX(); gripAy = b.bottom }
            5 -> { gripAx = b.left; gripAy = b.centerY() }
            6 -> { gripAx = b.centerX(); gripAy = b.top }
            7 -> { gripAx = b.right; gripAy = b.centerY() }
            else -> {
                gripAx = b.centerX(); gripAy = b.centerY()
                val top = b.top - selPad() - 30f * density / scale
                gripFrom = kotlin.math.atan2(top - gripAy, 0f)
            }
        }
    }

    /** The grip at (px, py) on the page: the stretch, scale or turn it makes. */
    private fun moveGrip(sel: Selection, px: Float, py: Float) {
        val b = sel.bounds
        val w = b.width().coerceAtLeast(1f); val h = b.height().coerceAtLeast(1f)
        when (selGrip) {
            in 0..3 -> {
                val cx = if (selGrip == 0 || selGrip == 3) b.left else b.right
                val cy = if (selGrip < 2) b.top else b.bottom
                val vx = cx - gripAx; val vy = cy - gripAy
                val k = (((px - gripAx) * vx + (py - gripAy) * vy) / (vx * vx + vy * vy).coerceAtLeast(1f)).coerceIn(0.1f, 8f)
                selKx = k; selKy = k
            }
            4 -> selKy = ((gripAy - py) / h).coerceIn(0.05f, 20f)
            5 -> selKx = ((px - gripAx) / w).coerceIn(0.05f, 20f)
            6 -> selKy = ((py - gripAy) / h).coerceIn(0.05f, 20f)
            7 -> selKx = ((gripAx - px) / w).coerceIn(0.05f, 20f)
            8 -> {
                var deg = Math.toDegrees((kotlin.math.atan2(py - gripAy, px - gripAx) - gripFrom).toDouble())
                while (deg > 180) deg -= 360.0
                while (deg < -180) deg += 360.0
                // Close to a multiple of 15°: snaps (with a tick), so level and square are easy.
                val near = Math.round(deg / 15.0) * 15.0
                val step = if (abs(deg - near) < 3.0) (near / 15.0).toInt() else null
                if (step != null) deg = near
                if (step != selTurnStep) { if (step != null) performHapticFeedback(android.view.HapticFeedbackConstants.CLOCK_TICK); selTurnStep = step }
                selAng = deg.toFloat()
            }
        }
    }

    /** The running move, scale, stretch or turn of a selection (page coordinates). */
    private fun selMatrix(sel: Selection): android.graphics.Matrix = android.graphics.Matrix().apply {
        if (action == Action.GRIP_SEL && selGrip >= 0) {
            if (selGrip == 8) setRotate(selAng, gripAx, gripAy) else setScale(selKx, selKy, gripAx, gripAy)
        } else {
            setScale(selScale, selScale, sel.bounds.left, sel.bounds.top)
            postTranslate(selDx, selDy)
        }
    }

    /** A grip let go: the selection turns, scales or stretches for good (one undo step). */
    private fun finishGrip() {
        val sel = selection ?: return
        val m = model ?: return
        val g = selGrip
        selGrip = -1
        val after = {
            val st = m.state(sel.sheetId)
            st.floating = emptySet(); st.invalidateNode()
            // The selection follows: its new box from what it now covers.
            val b = RectF()
            for (s in st.strokes) if (s.id in sel.ids) { if (b.isEmpty) b.set(s.bounds) else b.union(s.bounds) }
            for (im in st.images) if (im.id in sel.ids) { val r = RectF(im.x, im.y, im.x + im.w, im.y + im.h); if (b.isEmpty) b.set(r) else b.union(r) }
            if (selection === sel) {
                selection = Selection(sel.sheetId, sel.ids, if (b.isEmpty) sel.bounds else b)
                listener?.onSelectionChanged(selection)
            }
            invalidate()
        }
        val unchanged = when (g) { 8 -> abs(selAng) < 0.5f; else -> abs(selKx - 1f) < 0.005f && abs(selKy - 1f) < 0.005f }
        if (g < 0 || unchanged) { after(); return }
        when (g) {
            8 -> { val r = Math.toRadians(selAng.toDouble()); m.spin(sel.sheetId, sel.ids, gripAx, gripAy, kotlin.math.cos(r).toFloat(), kotlin.math.sin(r).toFloat()) { after() } }
            in 0..3 -> m.transform(sel.sheetId, sel.ids, 0f, 0f, selKx, gripAx, gripAy) { after() }
            else -> m.stretch(sel.sheetId, sel.ids, gripAx, gripAy, selKx, selKy) { after() }
        }
    }

    /** The selection's box in world coordinates (all its pages), including a running move/scale. */
    private fun selectionRectWorld(sel: Selection): RectF {
        val b = selUnion(sel)
        val l = b.left + selDx
        val t = b.top + selDy
        return RectF(l, t, l + b.width() * selScale, t + b.height() * selScale).apply { inset(-selPad(), -selPad()) }
    }

    /** Room around a selection's box (page points): at least 12 dp on screen, so the grips sit outside even a small one. */
    private fun selPad() = max(6f, 12f * density / scale)

    private fun drawCursor(c: Canvas) {
        if (hoverX.isNaN() || action != Action.NONE || !Settings.hoverCursor) return
        if (hoverErase || tool == Tool.ERASER) {
            cursorPaint.color = 0xAA787880.toInt()
            c.drawCircle(hoverX, hoverY, max(3f, eraserBaseRadius() * scale), cursorPaint)
        } else if (tool == Tool.LASER) {
            laserCore.style = Paint.Style.FILL; laserCore.alpha = 200
            c.drawCircle(hoverX, hoverY, 5f * density, laserCore)
            laserCore.style = Paint.Style.STROKE
        } else if (tool == Tool.PEN || tool == Tool.MARKER) {
            cursorPaint.color = Tools.inks[colorIdx]
            val k = if (loupeOpen && loupeStrip().contains(hoverX, hoverY)) loupeScale() else scale
            c.drawCircle(hoverX, hoverY, max(2f, brush.width * k / 2f), cursorPaint)
        }
    }

    override fun computeScroll() {
        if (scroller.computeScrollOffset()) {
            tx += scroller.currX - lastFlingX
            ty += scroller.currY - lastFlingY
            lastFlingX = scroller.currX
            lastFlingY = scroller.currY
            transformed()
            postInvalidateOnAnimation()
        }
    }

    // ========================================================================================
    // Input
    // ========================================================================================

    private fun isStylus(tool: Int) = tool == MotionEvent.TOOL_TYPE_STYLUS || tool == MotionEvent.TOOL_TYPE_ERASER
    private fun isPalm(ev: MotionEvent, idx: Int) = ev.getTouchMajor(idx) > palmPx
    private fun penNear(now: Long) = penId != -1 || hovering || now - hoverExitAt < HOVER_GRACE_MS || now - penUpAt < HOVER_GRACE_MS

    override fun onHoverEvent(ev: MotionEvent): Boolean {
        if (!isStylus(ev.getToolType(0))) return super.onHoverEvent(ev)
        when (ev.actionMasked) {
            MotionEvent.ACTION_HOVER_ENTER, MotionEvent.ACTION_HOVER_MOVE -> {
                if (!hovering) { hovering = true; rejectAllFingers() }
                hoverX = ev.x; hoverY = ev.y
                penButton = eraseButton(ev)
                hoverErase = penButton || ev.getToolType(0) == MotionEvent.TOOL_TYPE_ERASER
                invalidate()
            }
            MotionEvent.ACTION_HOVER_EXIT -> {
                if (hovering) hoverExitAt = SystemClock.uptimeMillis()
                hovering = false
                hoverX = Float.NaN
                invalidate()
            }
        }
        return true
    }

    /** Pen button press/release while hovering arrives here on some devices. */
    override fun onGenericMotionEvent(ev: MotionEvent): Boolean {
        if (isStylus(ev.getToolType(0)) && (ev.actionMasked == MotionEvent.ACTION_BUTTON_PRESS || ev.actionMasked == MotionEvent.ACTION_BUTTON_RELEASE)) {
            penButton = eraseButton(ev)
            hoverErase = penButton
            invalidate()
            return true
        }
        return super.onGenericMotionEvent(ev)
    }

    /** Any pen button that should erase (pens report it differently: primary, secondary, …). */
    private fun eraseButton(ev: MotionEvent): Boolean =
        (ev.buttonState and (MotionEvent.BUTTON_STYLUS_PRIMARY or MotionEvent.BUTTON_STYLUS_SECONDARY or MotionEvent.BUTTON_SECONDARY or MotionEvent.BUTTON_TERTIARY)) != 0

    @SuppressLint("ClickableViewAccessibility")
    // --- a graph being edited: touches in its box go to its tools -----------------------------

    /** The graph being edited (its sheet, box, and what to do with touches in it). */
    interface GraphTouch {
        fun sheet(): String
        fun box(): RectF
        fun down(x: Float, y: Float, tol: Float)
        fun move(x: Float, y: Float)
        fun up(x: Float, y: Float, tap: Boolean)
        fun pinch(k: Float, cx: Float, cy: Float)
        /** All fingers let go (a moved view may snap to the squares). */
        fun end() {}
    }
    var graph: GraphTouch? = null
    private var graphCapture = false
    private var graphPinch = 0f
    private var graphDownX = 0f
    private var graphDownY = 0f
    private var graphDownAt = 0L
    private var graphMoved = false

    /** Touches for the graph being edited (true: taken). Outside its box the page acts as usual. */
    private fun graphTouch(ev: MotionEvent): Boolean {
        val g = graph ?: return false
        val p = model?.placed?.firstOrNull { it.sheetId == g.sheet() } ?: board?.takeIf { it.sheetId == g.sheet() } ?: return false
        fun page(i: Int) = (toWorldX(ev.getX(i)) - p.rect.left) to (toWorldY(ev.getY(i)) - p.rect.top)
        val tol = 14f * density / scale
        when (ev.actionMasked) {
            MotionEvent.ACTION_DOWN -> {
                val (x, y) = page(0)
                val b = RectF(g.box()).apply { inset(-tol, -tol) }
                if (!b.contains(x, y)) return false
                graphCapture = true; graphPinch = 0f; graphMoved = false
                graphDownX = ev.getX(0); graphDownY = ev.getY(0); graphDownAt = ev.eventTime
                g.down(x, y, tol)
                return true
            }
            MotionEvent.ACTION_POINTER_DOWN -> if (graphCapture && ev.pointerCount == 2) {
                graphPinch = hypot(ev.getX(0) - ev.getX(1), ev.getY(0) - ev.getY(1))
                graphMoved = true
                val (x, y) = page(0)
                g.up(x, y, false)
            }
            MotionEvent.ACTION_MOVE -> if (graphCapture) {
                if (ev.pointerCount >= 2 && graphPinch > 0f) {
                    val d = hypot(ev.getX(0) - ev.getX(1), ev.getY(0) - ev.getY(1))
                    val cx = (toWorldX((ev.getX(0) + ev.getX(1)) / 2f) - p.rect.left); val cy = (toWorldY((ev.getY(0) + ev.getY(1)) / 2f) - p.rect.top)
                    if (d > 1f) g.pinch(d / graphPinch, cx, cy)
                    graphPinch = d
                } else if (graphPinch == 0f) {
                    if (hypot(ev.getX(0) - graphDownX, ev.getY(0) - graphDownY) > touchSlop) graphMoved = true
                    val (x, y) = page(0)
                    g.move(x, y)
                }
                return true
            }
            MotionEvent.ACTION_POINTER_UP -> if (graphCapture) return true
            MotionEvent.ACTION_UP, MotionEvent.ACTION_CANCEL -> if (graphCapture) {
                graphCapture = false
                val (x, y) = page(0)
                if (graphPinch == 0f) g.up(x, y, !graphMoved && ev.eventTime - graphDownAt < 500 && ev.actionMasked == MotionEvent.ACTION_UP)
                g.end()
                graphPinch = 0f
                return true
            }
        }
        return graphCapture
    }

    /** A point or slider of an interactive graph being dragged on the page: its sheet, the plot as it is now, the object. */
    private var liveDrag: Triple<String, uniffi.n0te_ffi.FfiPlot, String>? = null

    /**
     * An axis of an interactive graph being stretched on the page: its sheet, the plot when the
     * drag began, which axis (1 x, 2 y), where it was grabbed (page points), the page's squares
     * (0: none), the plot as it is now.
     */
    private class AxisDrag(val sheet: String, val start: uniffi.n0te_ffi.FfiPlot, val axis: Int, val downX: Float, val downY: Float, val squares: Float, val pointer: Int) {
        var now = start
    }
    private var axisDrag: AxisDrag? = null

    /**
     * Interactive graphs (switched on per graph): a touch on one of their points or sliders drags
     * it right on the page; on an axis (the pen, or a finger where fingers write) it stretches
     * only that axis, as in the editor. Letting go stores it (one step to undo). Elsewhere
     * nothing changes.
     */
    private fun interactiveTouch(ev: MotionEvent): Boolean {
        if (graph != null || readOnly) return false
        val m = model ?: return false
        if (axisDrag != null) return axisTouch(ev, m)
        when (ev.actionMasked) {
            MotionEvent.ACTION_DOWN -> {
                val wx = toWorldX(ev.getX(0)); val wy = toWorldY(ev.getY(0))
                val p = board ?: m.placed.firstOrNull { it.rect.contains(wx, wy) } ?: return false
                val x = wx - p.rect.left; val y = wy - p.rect.top
                val st = m.sheets[p.sheetId] ?: return false
                val tol = 14f * density / scale
                val stylus = isStylus(ev.getToolType(0))
                val erasing = stylus && (ev.getToolType(0) == MotionEvent.TOOL_TYPE_ERASER || (Settings.sideButtonErases && (eraseButton(ev) || penButton)))
                val axes = eyedropper == null && plotBox == null &&
                    PageAxis.allowed(stylus, erasing, tool, penOnly, navigateOnly, palm = !stylus && isPalm(ev, 0), penNear = !stylus && penNear(ev.eventTime))
                val axisTol = PageAxis.tolerance(stylus, density, scale)
                for (im in st.images.reversed()) {
                    val plot = im.plot ?: continue
                    if (!plot.interactive || !RectF(im.x, im.y, im.x + im.w, im.y + im.h).apply { inset(-tol, -tol) }.contains(x, y)) continue
                    val id = uniffi.n0te_ffi.plotHit(plot, x, y, tol)
                    val info = id?.let { h -> uniffi.n0te_ffi.plotInfo(plot).firstOrNull { it.id == h } }
                    if (id != null && info != null && (info.draggable || info.sliderValue != null)) {
                        liveDrag = Triple(p.sheetId, plot, id)
                        return true
                    }
                    // Not on a point or slider: an axis grabbed stretches alone.
                    val axis = if (axes && info == null) uniffi.n0te_ffi.plotAxisAt(plot, x, y, axisTol).toInt() else 0
                    if (axis != 0) {
                        axisDrag = AxisDrag(p.sheetId, plot, axis, x, y, if (p.isBoard) 0f else PageAxis.squares(p.spec), ev.getPointerId(0))
                        if (stylus) { requestUnbufferedDispatch(ev); parent?.requestDisallowInterceptTouchEvent(true) }
                        return true
                    }
                }
                return false
            }
            MotionEvent.ACTION_MOVE -> {
                val (sheet, plot, id) = liveDrag ?: return false
                val p = board ?: m.placed.firstOrNull { it.sheetId == sheet } ?: return true
                val x = toWorldX(ev.getX(0)) - p.rect.left; val y = toWorldY(ev.getY(0)) - p.rect.top
                val slider = plot.objs.firstOrNull { it.id == id }?.style?.slider
                val next = if (slider != null) {
                    val track = uniffi.n0te_ffi.plotSliderTrack(plot, id)
                    if (track.size == 3) uniffi.n0te_ffi.plotSetSlider(plot, id, slider.min + (slider.max - slider.min) * ((x - track[0]) / (track[1] - track[0])).coerceIn(0f, 1f)) else null
                } else {
                    val q = uniffi.n0te_ffi.plotToPlot(plot, x, y)
                    uniffi.n0te_ffi.plotDrag(plot, id, q[0], q[1])
                }
                if (next != null) { liveDrag = Triple(sheet, next, id); plotDraft = next }
                return true
            }
            MotionEvent.ACTION_UP, MotionEvent.ACTION_CANCEL -> {
                val (sheet, plot, _) = liveDrag ?: return false
                liveDrag = null
                // Stored in its box as it is now (one step to undo), then drawn from the page again.
                val stored = m.sheets[sheet]?.images?.firstOrNull { it.id == plot.id }?.plot
                if (ev.actionMasked == MotionEvent.ACTION_UP && stored != null) {
                    val d = plot.copy(x = stored.x, y = stored.y, w = stored.w, h = stored.h)
                    if (d != stored) m.setPlot(sheet, d)
                }
                plotDraft = null
                return true
            }
        }
        return liveDrag != null
    }

    /** The touch that grabbed an axis: stretches it while it moves, stores it (snapped to the squares) when it lifts. */
    private fun axisTouch(ev: MotionEvent, m: NotebookModel): Boolean {
        val d = axisDrag ?: return false
        val i = ev.findPointerIndex(d.pointer)
        fun pagePoint(): Pair<Float, Float>? {
            val p = board?.takeIf { it.sheetId == d.sheet } ?: m.placed.firstOrNull { it.sheetId == d.sheet } ?: return null
            if (i < 0) return null
            return (toWorldX(ev.getX(i)) - p.rect.left) to (toWorldY(ev.getY(i)) - p.rect.top)
        }
        // Another finger coming or going changes nothing; the one that grabbed lifting ends it.
        val action = if (ev.actionMasked == MotionEvent.ACTION_POINTER_UP && ev.actionIndex == i) MotionEvent.ACTION_UP else ev.actionMasked
        /** Stretched to where the pen is, always from the plot as it was when grabbed (the number grabbed stays under the pen). */
        fun follow() {
            val (x, y) = pagePoint() ?: return
            val s = if (d.axis == 1) uniffi.n0te_ffi.plotAxisDragged(d.start, 1.toUByte(), d.downX, x) else uniffi.n0te_ffi.plotAxisDragged(d.start, 2.toUByte(), d.downY, y)
            d.now = s.copy(x = d.start.x, y = d.start.y, w = d.start.w, h = d.start.h)
        }
        when (action) {
            MotionEvent.ACTION_MOVE -> {
                follow()
                plotDraft = d.now
            }
            MotionEvent.ACTION_UP, MotionEvent.ACTION_CANCEL -> {
                axisDrag = null
                // Where it was let go (the last move can lie a little before it).
                if (action == MotionEvent.ACTION_UP) follow()
                val stored = m.sheets[d.sheet]?.images?.firstOrNull { it.id == d.start.id }?.plot
                if (action == MotionEvent.ACTION_UP && stored != null && d.now != d.start) {
                    // In its box as stored now; one step to undo.
                    val put = PageAxis.settled(d.now.copy(x = stored.x, y = stored.y, w = stored.w, h = stored.h), d.squares)
                    if (put != stored) {
                        // Shown as put down until the page has it.
                        plotDraft = put
                        m.setPlot(d.sheet, put) { if (plotDraft === put) plotDraft = null }
                        return true
                    }
                }
                plotDraft = null
            }
        }
        return true
    }

    override fun onTouchEvent(ev: MotionEvent): Boolean {
        if (graphTouch(ev)) return true
        if (interactiveTouch(ev)) return true
        if (ev.actionMasked == MotionEvent.ACTION_BUTTON_PRESS || ev.actionMasked == MotionEvent.ACTION_BUTTON_RELEASE) {
            if (isStylus(ev.getToolType(0))) penButton = eraseButton(ev)
            return true
        }
        if (ev.actionMasked == MotionEvent.ACTION_DOWN && (fingers.isNotEmpty() || penId != -1)) cancelAll()
        when (ev.actionMasked) {
            MotionEvent.ACTION_DOWN, MotionEvent.ACTION_POINTER_DOWN -> pointerDown(ev, ev.actionIndex)
            MotionEvent.ACTION_MOVE -> move(ev)
            MotionEvent.ACTION_UP, MotionEvent.ACTION_POINTER_UP ->
                pointerUp(ev, ev.actionIndex, canceled = (ev.flags and MotionEvent.FLAG_CANCELED) != 0)
            MotionEvent.ACTION_CANCEL -> cancelAll()
        }
        return true
    }

    private fun pointerDown(ev: MotionEvent, idx: Int) {
        val id = ev.getPointerId(idx)
        val now = ev.eventTime
        scroller.forceFinished(true)
        anim?.cancel()
        if (isStylus(ev.getToolType(idx)) && !navigateOnly && !readOnly) {
            rejectAllFingers()
            if (penId != -1) return
            // The pen on the frame's corner resizes it (and doesn't write).
            if (loupeOpen && grabLoupe(id, ev.getX(idx), ev.getY(idx), pen = true)) { penId = id; return }
            penId = id
            hovering = false
            requestUnbufferedDispatch(ev)
            parent?.requestDisallowInterceptTouchEvent(true)
            val button = eraseButton(ev) || penButton
            val erasing = tool == Tool.ERASER || ev.getToolType(idx) == MotionEvent.TOOL_TYPE_ERASER || (Settings.sideButtonErases && button)
            beginAction(id, stylus = true, ev.getX(idx), ev.getY(idx), ev.getPressure(idx), now, erasing)
            return
        }
        if (loupeOpen && !penNear(now) && !isPalm(ev, idx) && (padGrip().contains(ev.getX(idx), ev.getY(idx)) || padSizer().contains(ev.getX(idx), ev.getY(idx))) &&
            grabLoupe(id, ev.getX(idx), ev.getY(idx), pen = false)) return
        if (loupeOpen && loupeGrabId == -1 && !penNear(now) && !isPalm(ev, idx)) {
            val s = loupeStrip()
            val bar = when { barTouchH(s).contains(ev.getX(idx), ev.getY(idx)) -> Grab.BAR_H; barTouchV(s).contains(ev.getX(idx), ev.getY(idx)) -> Grab.BAR_V; else -> null }
            if (bar != null) {
                removeCallbacks(loupeAdvance)
                loupeGrabId = id; loupeGrab = bar; loupeGrabX = ev.getX(idx); loupeGrabY = ev.getY(idx); loupeGrabDy = 0f
                scrubTo(ev.getX(idx), ev.getY(idx))
                return
            }
        }
        if (loupeOpen && loupeStrip().contains(ev.getX(idx), ev.getY(idx))) {
            // A finger in the magnifier slides its frame, two pinch it (a resting palm while writing doesn't).
            if (!penNear(now) && !isPalm(ev, idx)) {
                if (loupeGrabId == -1) {
                    loupeGrabId = id; loupeGrab = Grab.PAD; loupeGrabX = ev.getX(idx); loupeGrabY = ev.getY(idx); loupeGrabDy = 0f
                    loupeGrabAt = now; loupeGrabDownX = ev.getX(idx); loupeGrabMoved = 0f
                }
                else if (loupeGrab == Grab.PAD && loupePinchId == -1) {
                    loupePinchId = id
                    val i = ev.findPointerIndex(loupeGrabId)
                    loupePinchDist = if (i >= 0) hypot(ev.getX(idx) - ev.getX(i), ev.getY(idx) - ev.getY(i)).coerceAtLeast(1f) else 1f
                }
            }
            return
        }
        // A finger on the frame moves it, on its corner resizes it.
        if (loupeOpen && !penNear(now) && !isPalm(ev, idx) && grabLoupe(id, ev.getX(idx), ev.getY(idx), pen = false)) return
        val f = Finger(id, ev.getX(idx), ev.getY(idx), ev.getX(idx), ev.getY(idx), rejected = !navigateOnly && !readOnly && (penNear(now) || isPalm(ev, idx)), downAt = now)
        fingers[id] = f
        if (velocity == null) velocity = VelocityTracker.obtain()
        velocity?.addMovement(ev)
        if (!f.rejected) activeFingersChanged(now)
    }

    private fun activeFingers() = fingers.values.filter { !it.rejected }

    private fun activeFingersChanged(now: Long) {
        val active = activeFingers()
        when {
            active.isEmpty() -> gesture = Gesture.NONE
            active.size == 1 && rulerOn && action == Action.NONE && rulerContains(toWorldX(active[0].x), toWorldY(active[0].y)) -> {
                val f = active[0]
                gesture = Gesture.RULER
                rulerA = f.id; rulerB = -1
                lastRulerX = f.x; lastRulerY = f.y
            }
            active.size == 2 && gesture == Gesture.RULER && active.any { it.id == rulerA } -> {
                val a = active.first { it.id == rulerA }
                val b = active.first { it.id != rulerA }
                rulerB = b.id
                lastRulerAngle = kotlin.math.atan2(b.y - a.y, b.x - a.x)
                lastRulerX = (a.x + b.x) / 2; lastRulerY = (a.y + b.y) / 2
            }
            active.size == 1 -> {
                val f = active[0]
                val onSelection = selection != null && hitSelection(f.x, f.y) != Action.NONE
                val onHandle = handleAt(f.x, f.y) != null
                if (!navigateOnly && !readOnly && (!penOnly || onSelection || onHandle) && !afterPinch && action == Action.NONE && gesture == Gesture.NONE) {
                    beginAction(f.id, stylus = false, f.x, f.y, 0.5f, now, erasing = tool == Tool.ERASER)
                } else if (action == Action.NONE) {
                    gesture = if (afterPinch) Gesture.PAN else Gesture.PAN_PENDING
                    panId = f.id
                    lastPanX = f.x; lastPanY = f.y
                    removeCallbacks(fingerHold)
                    if (gesture == Gesture.PAN_PENDING && !navigateOnly) { holdId = f.id; postDelayed(fingerHold, HOLD_MS) }
                }
            }
            else -> {
                // Two fingers zoom/move. A finger stroke that just began was the first finger.
                if (action != Action.NONE && !actionStylus) {
                    if (action == Action.INK && (SystemClock.uptimeMillis() - strokeStart < 300 || count < 6)) cancelInk() else endAction(now, commit = true)
                }
                val a = active[0]; val b = active[1]
                pinchA = a.id; pinchB = b.id
                lastPinchDist = hypot(a.x - b.x, a.y - b.y).coerceAtLeast(1f)
                lastPinchCx = (a.x + b.x) / 2; lastPinchCy = (a.y + b.y) / 2
                gesture = Gesture.PINCH
                afterPinch = true
                breakFollow()
            }
        }
    }

    /** Pen or drawing finger went down at screen (x, y). */
    private fun beginAction(id: Int, stylus: Boolean, x: Float, y: Float, pressure: Float, now: Long, erasing: Boolean) {
        val m = model ?: return
        actionId = id
        actionStylus = stylus
        actionDownX = x; actionDownY = y
        actionDownAt = SystemClock.uptimeMillis()
        // The pen in the magnifier's strip writes into its frame.
        mapLoupe = loupeOpen && stylus && eyedropper == null && loupeStrip().contains(x, y)
        if (mapLoupe) { beginLoupeAction(x, y, pressure, now, erasing); return }
        val wx = toWorldX(x); val wy = toWorldY(y)
        eyedropper?.let { pick ->
            eyedropper = null
            action = Action.TAP; target = null
            pickColorAt(x.toInt(), y.toInt())?.let(pick)
            return
        }
        if (tool == Tool.LASER && !erasing) {
            // Laser pointer: anywhere, doesn't touch the selection or the pages.
            action = Action.LASER; target = null
            laserSeg++
            if (Settings.laserTrail == "short") { laser.clear(); laserEnded.clear() }
            laser += floatArrayOf(wx, wy, since(SystemClock.uptimeMillis()), laserSeg.toFloat())
            invalidate()
            return
        }
        // A coordinate system to put down: the pen draws its box.
        plotBox?.let { pb ->
            val p = board ?: m.placed.firstOrNull { it.rect.contains(wx, wy) }
            action = Action.TAP; target = null
            if (p == null || p.spec.background == FfiBackground.TEXT) { listener?.onMessage(if (p == null) "Das Koordinatensystem auf einer Seite aufziehen" else "Auf Textseiten geht kein Koordinatensystem"); return }
            clearSelection()
            pb.page = p
            pb.x0 = wx - p.rect.left; pb.y0 = wy - p.rect.top; pb.x1 = pb.x0; pb.y1 = pb.y0
            action = Action.PLOT_BOX
            invalidate(); return
        }
        handleAt(x, y)?.let { h ->
            action = Action.HANDLE; handle = h; handleDrag = 0f; target = null
            handleDownAt = SystemClock.uptimeMillis()
            removeCallbacks(handleConfirm)
            // Held long enough (the ring closes): the page is added while the pen is still down.
            val hold = Settings.handleHoldMs.toLong()
            if (hold > 0) postDelayed(handleConfirm, hold)
            invalidate(); return
        }
        // A tap on a group title: fold button or group menu (decided at pen-up, short taps only).
        m.headers.firstOrNull { it.rect.contains(wx, wy) }?.let { action = Action.TAP; target = null; return }
        val sel = selection
        if (sel != null) {
            when (hitSelection(x, y)) {
                Action.MOVE_SEL -> { action = Action.MOVE_SEL; startSelDrag(); return }
                Action.SCALE_SEL -> { action = Action.SCALE_SEL; startSelDrag(); return }
                Action.GRIP_SEL -> { action = Action.GRIP_SEL; startSelDrag(); return }
                Action.END_SEL -> { action = Action.END_SEL; model?.state(sel.sheetId)?.let { it.floating = sel.ids; it.invalidateNode() }; invalidate(); return }
                else -> clearSelection()
            }
        }
        if (tool == Tool.LASSO && !erasing && Settings.selectMode == "space" && !readOnly) {
            // Space: a line across the page here; dragging down pushes what is below further down.
            val p = board ?: m.placed.firstOrNull { it.rect.contains(wx, wy) }
            target = null
            if (p == null || p.isBoard || p.spec.background == FfiBackground.TEXT) {
                action = Action.NONE
                if (p != null) listener?.onMessage(if (p.isBoard) "Auf dem Board ist überall Platz" else "Auf Textseiten fließt der Text: dort Leerzeilen tippen")
                return
            }
            val y = wy - p.rect.top
            val x = wx - p.rect.left
            val st = m.state(p.sheetId)
            val below = runCatching { st.handle?.below(y) }.getOrNull() ?: uniffi.n0te_ffi.FfiBelow(null, null)
            val right = runCatching { st.handle?.rightOf(x) }.getOrNull() ?: uniffi.n0te_ffi.FfiBelow(null, null)
            // On lined, squared or dotted paper whole steps, so writing stays on the lines (sideways:
            // whole boxes on squared and dotted paper, else millimetres).
            val ruled = p.spec.pdfBlob == null && !p.isBoard && p.spec.background in listOf(FfiBackground.LINES, FfiBackground.GRID, FfiBackground.DOTS, FfiBackground.CORNELL)
            val step = if (ruled) p.spec.spacing.takeIf { it >= 4f } ?: 20f else MM
            val columns = p.spec.pdfBlob == null && p.spec.background in listOf(FfiBackground.GRID, FfiBackground.DOTS)
            val stepX = if (columns) p.spec.spacing.takeIf { it >= 4f } ?: 20f else MM
            space = SpaceDrag(p, y, wy, below, step, x, wx, right, stepX)
            action = Action.SPACE
            invalidate()
            return
        }
        if (tool == Tool.LASSO && !erasing) {
            // The lasso goes anywhere, also between pages and across several (world coordinates).
            action = Action.LASSO
            target = board ?: m.placed.firstOrNull { it.rect.contains(wx, wy) }
            lasso.clear(); lasso += wx; lasso += wy
            return
        }
        val p = board ?: m.placed.firstOrNull { it.rect.contains(wx, wy) }
        target = p
        if (p == null) {
            // Off the pages: a stroke (or the eraser) may start out here; it belongs to the page it reaches.
            if (!readOnly && (erasing || tool == Tool.PEN || tool == Tool.MARKER)) {
                action = Action.PRE_INK
                preErase = erasing
                preInk.clear(); preInk += floatArrayOf(wx, wy, pressure, since(now))
            } else action = Action.NONE
            return
        }
        var lx = wx - p.rect.left; var ly = wy - p.rect.top
        if (tool == Tool.TEXT && !erasing) {
            // Text tool: a tap edits / creates, a drag on a box moves it (decided while moving).
            action = Action.TEXT
            textHit = textAt(p, lx, ly)
            textDownLx = lx; textDownLy = ly; textMoved = false
            return
        }
        snapEdge = if (stylus || !penOnly) rulerEdgeAt(wx, wy) else 0
        if (snapEdge != 0 && !erasing && tool != Tool.LASSO) {
            val q = snapToRuler(wx, wy, snapEdge); lx = q[0] - p.rect.left; ly = q[1] - p.rect.top
        }
        when {
            erasing -> { action = Action.ERASE; beginErase(p, lx, ly, now) }
            else -> startInk(lx, ly, pressure, now, stylus)
        }
    }

    /** A stroke that began off the pages, so far (world x, y, raw pressure, time). */
    private val preInk = ArrayList<FloatArray>()
    private var preErase = false

    /**
     * The pen moving after it went down off the pages: once it reaches one, the stroke begins
     * on that page's edge where the pen crossed it, so a line can start outside and run from
     * the very edge. The eraser starts erasing there.
     */
    private fun preInkMove(wx: Float, wy: Float, pressure: Float, t: Long, batch: Boolean) {
        preInk += floatArrayOf(wx, wy, pressure, since(t))
        val q = model?.placed?.firstOrNull { it.rect.contains(wx, wy) } ?: return
        target = q
        if (preErase) { preInk.clear(); action = Action.ERASE; beginErase(q, wx - q.rect.left, wy - q.rect.top, t); return }
        // It begins right on the page's edge, where the pen crossed it (the way out there is no ink).
        val out = preInk[preInk.size - 2]; val cur = preInk.last()
        var lo = 0f; var hi = 1f
        repeat(14) { val mid = (lo + hi) / 2; if (q.rect.contains(out[0] + (cur[0] - out[0]) * mid, out[1] + (cur[1] - out[1]) * mid)) hi = mid else lo = mid }
        val ex = (out[0] + (cur[0] - out[0]) * hi).coerceIn(q.rect.left, q.rect.right)
        val ey = (out[1] + (cur[1] - out[1]) * hi).coerceIn(q.rect.top, q.rect.bottom)
        startInk(ex - q.rect.left, ey - q.rect.top, cur[2], preInk[0][3].toLong() + timeBase, actionStylus)
        appendInk(cur[0] - q.rect.left, cur[1] - q.rect.top, lastPressure, t)
        preInk.clear()
        stillSince = t; stillX = wx - q.rect.left; stillY = wy - q.rect.top
        if (!batch) updateLive()
    }

    /** A stroke begins at (lx, ly) on [target]. */
    private fun startInk(lx: Float, ly: Float, pressure: Float, now: Long, stylus: Boolean) {
        action = Action.INK
        count = 0
        snapped = false
        lineHeld = false; liveHold = false; lineSnapDeg = null; lineFree = false
        shapeHeld = null
        removeCallbacks(holdCheck); removeCallbacks(arrowCheck)
        liveSent = 0; liveSeq = 0; liveStrokeId = System.nanoTime()
        splitStroke = false
        strokeStart = now
        lastPressure = normPressure(pressure, stylus)
        stillSince = now; stillX = lx; stillY = ly
        liveColor = brush.color.toInt()
        removeCallbacks(flowCheck)
        removeCallbacks(loupeAdvance)
        fountainOn = tool == Tool.PEN && Tools.pen == PenType.FOUNTAIN
        if (fountainOn) { fountainState = Fountain.State(); fountainOpts = Fountain.options() }
        appendInk(lx, ly, lastPressure, now)
        updateLive()
    }

    private fun move(ev: MotionEvent) {
        val now = ev.eventTime
        velocity?.addMovement(ev)
        // The active action's pointer.
        if (action != Action.NONE && actionId != -1) {
            val idx = ev.findPointerIndex(actionId)
            if (idx >= 0) {
                for (h in 0 until ev.historySize) actionMove(ev.getHistoricalX(idx, h), ev.getHistoricalY(idx, h), ev.getHistoricalPressure(idx, h), ev.getHistoricalEventTime(h), batch = true)
                actionMove(ev.getX(idx), ev.getY(idx), ev.getPressure(idx), now, batch = false)
            }
        }
        if (loupeGrabId != -1) moveLoupeGrab(ev)
        // Fingers: positions and late palm detection.
        var activeChanged = false
        for (i in 0 until ev.pointerCount) {
            val f = fingers[ev.getPointerId(i)] ?: continue
            f.x = ev.getX(i); f.y = ev.getY(i)
            if (!f.rejected && isPalm(ev, i)) {
                f.rejected = true
                activeChanged = true
                if (f.id == actionId && !actionStylus) {
                    if (action == Action.INK) cancelInk() else endAction(now, commit = false)
                }
            }
        }
        if (activeChanged) { gesture = Gesture.NONE; activeFingersChanged(now) }
        when (gesture) {
            Gesture.PAN_PENDING -> {
                val f = fingers[panId]
                if (f == null || f.rejected) gesture = Gesture.NONE
                else if (hypot(f.x - f.downX, f.y - f.downY) > touchSlop) {
                    gesture = Gesture.PAN
                    lastPanX = f.x; lastPanY = f.y
                    breakFollow()
                }
            }
            Gesture.PAN -> {
                val f = fingers[panId]
                if (f == null || f.rejected) gesture = Gesture.NONE
                else {
                    if (lock != Lock.NONE && board == null) {
                        // Locked: sideways only as far as the lock allows, beyond that a rubber-band pull.
                        if (!panTracking) { panTracking = true; panBaseTx = tx; freeDx = 0f }
                        ty += f.y - lastPanY
                        freeDx += f.x - lastPanX
                        val free = panBaseTx + freeDx
                        val base = if (zoomLocked) width / 2f - lockCenterX * scale else pageTx(free, scale)
                        stretch = free - base
                        tx = base + stretch * 0.35f
                    } else {
                        tx += f.x - lastPanX; ty += f.y - lastPanY
                    }
                    lastPanX = f.x; lastPanY = f.y
                    transformed()
                }
            }
            Gesture.PINCH -> {
                val a = fingers[pinchA]; val b = fingers[pinchB]
                if (a == null || b == null || a.rejected || b.rejected) gesture = Gesture.NONE
                else if (zoomLocked && board == null) {
                    // Zoom locked: two fingers only scroll vertically.
                    val cy = (a.y + b.y) / 2
                    ty += cy - lastPinchCy
                    lastPinchCy = cy
                    transformed()
                } else {
                    val dist = hypot(a.x - b.x, a.y - b.y).coerceAtLeast(1f)
                    val cx = (a.x + b.x) / 2; val cy = (a.y + b.y) / 2
                    val newScale = (scale * dist / lastPinchDist).coerceIn(minScale(), maxScale())
                    val k = newScale / scale
                    tx = cx - (lastPinchCx - tx) * k
                    ty = cy - (lastPinchCy - ty) * k
                    scale = newScale
                    if (pageLocked && board == null) tx = pageTx(tx, scale)
                    lastPinchDist = dist; lastPinchCx = cx; lastPinchCy = cy
                    panTracking = false
                    transformed()
                }
            }
            Gesture.RULER -> {
                val a = fingers[rulerA]
                val b = if (rulerB != -1) fingers[rulerB] else null
                if (a == null || a.rejected) gesture = Gesture.NONE
                else if (b != null && !b.rejected) {
                    val ang = kotlin.math.atan2(b.y - a.y, b.x - a.x)
                    val cx = (a.x + b.x) / 2; val cy = (a.y + b.y) / 2
                    rulerAngle += ang - lastRulerAngle
                    // Snap to 0/45/90… within 2°.
                    val deg = Math.toDegrees(rulerAngle.toDouble())
                    val near = Math.round(deg / 45.0) * 45.0
                    val shown = if (abs(deg - near) < 2.0) Math.toRadians(near).toFloat() else rulerAngle
                    rulerCx += (cx - lastRulerX) / scale; rulerCy += (cy - lastRulerY) / scale
                    lastRulerAngle = ang; lastRulerX = cx; lastRulerY = cy
                    if (shown != rulerAngle) { rulerAngle = shown; lastRulerAngle = ang }
                    invalidate()
                } else {
                    rulerCx += (a.x - lastRulerX) / scale; rulerCy += (a.y - lastRulerY) / scale
                    lastRulerX = a.x; lastRulerY = a.y
                    invalidate()
                }
            }
            Gesture.NONE -> {}
        }
    }

    private fun actionMove(x: Float, y: Float, pressure: Float, t: Long, batch: Boolean) {
        val wx = aWorldX(x); val wy = aWorldY(y)
        when (action) {
            Action.PRE_INK -> preInkMove(wx, wy, pressure, t, batch)
            Action.INK -> {
                if (snapped) {
                    // A held line: its end follows the pen; other shapes stay until the pen lifts.
                    if (lineHeld) target?.let { p -> moveHeldLine(wx - p.rect.left, wy - p.rect.top, t, batch) }
                    else shapeHeld?.let { h -> target?.let { p -> moveHeldShape(h, wx - p.rect.left, wy - p.rect.top, t, batch) } }
                    return
                }
                var p = target ?: return
                // Crossing onto another page: the stroke ends here and continues there.
                if (board == null && !mapLoupe && snapEdge == 0 && count >= 2 && !p.rect.contains(wx, wy)) {
                    model?.placed?.firstOrNull { it.rect.contains(wx, wy) && it.sheetId != p.sheetId }?.let { q -> splitInk(p, q, t); p = q }
                }
                var lx = wx - p.rect.left; var ly = wy - p.rect.top
                if (snapEdge != 0) { val q = snapToRuler(wx, wy, snapEdge); lx = q[0] - p.rect.left; ly = q[1] - p.rect.top }
                val pr = lastPressure * 0.6f + normPressure(pressure, actionStylus) * 0.4f
                lastPressure = pr
                if (hypot(lx - stillX, ly - stillY) > 6f / aScale) {
                    stillSince = t; stillX = lx; stillY = ly
                    // Held still for half a second from here: snap to a shape (while drawing).
                    if (Settings.shapes) {
                        removeCallbacks(holdCheck); postDelayed(holdCheck, 500)
                        removeCallbacks(arrowCheck); if (Settings.arrows) postDelayed(arrowCheck, ARROW_HOLD_MS)
                    }
                }
                val bx = samples[(count - 1) * 4]; val by = samples[(count - 1) * 4 + 1]
                if (hypot(lx - bx, ly - by) >= 0.4f / aScale) appendInk(lx, ly, pr, t)
                if (!batch) updateLive()
            }
            Action.ERASE -> target?.let { eraseMove(it, wx - it.rect.left, wy - it.rect.top, t) }
            Action.TEXT -> {
                val p = target ?: return
                val hit = textHit?.takeIf { it.id != BODY_ID } ?: return
                val lx = wx - p.rect.left; val ly = wy - p.rect.top
                if (!textMoved && hypot((lx - textDownLx) * scale, (ly - textDownLy) * scale) < touchSlop) return
                textMoved = true
                val st = model?.sheets?.get(p.sheetId) ?: return
                // The texts' recording follows st.texts itself; no version bump per move (that
                // re-planned the page's ink and redrew its outline picture on every pen sample;
                // placing the box at the end counts as the change).
                st.texts = st.texts.map { if (it.id == hit.id) it.copy(x = hit.x + lx - textDownLx, y = hit.y + ly - textDownLy) else it }
                if (!batch) invalidate()
            }
            Action.LASER -> {
                laser += floatArrayOf(wx, wy, since(SystemClock.uptimeMillis()), laserSeg.toFloat())
                if (!batch) { invalidate(); sendPresence(false, PointF(x, y)) }
            }
            Action.SPACE -> space?.let { sd ->
                // Which way: from where the pen first goes (a PDF page only gets space downwards).
                if (sd.sideways == null) {
                    val mx = (wx - sd.downX) * scale
                    val my = (wy - sd.downY) * scale
                    if (hypot(mx, my) >= 12f * density) {
                        val across = abs(mx) > abs(my)
                        if (across && sd.page.spec.pdfBlob != null) listener?.onMessage("Auf PDF-Seiten geht Platz nur nach unten")
                        sd.sideways = across && sd.page.spec.pdfBlob == null
                    }
                }
                when (sd.sideways) {
                    true -> {
                        val dx = Math.round((wx - sd.downX) / sd.stepX) * sd.stepX
                        if (dx != sd.asked || sd.planX == null) {
                            sd.asked = dx
                            sd.planX = runCatching { uniffi.n0te_ffi.planSpaceX(sd.page.spec, sd.x, dx, sd.right) }.getOrNull()
                            if (!batch) invalidate()
                        }
                    }
                    false -> {
                        val dy = Math.round((wy - sd.downY) / sd.step) * sd.step
                        if (dy != sd.asked || sd.plan == null) {
                            sd.asked = dy
                            sd.plan = runCatching { uniffi.n0te_ffi.planSpace(sd.page.spec, sd.y, dy, sd.below) }.getOrNull()
                            if (!batch) invalidate()
                        }
                    }
                    null -> {}
                }
            }
            Action.LASSO -> {
                val n = lasso.size
                if (Settings.selectMode != "free" && n >= 2) {
                    // Box and screenshot: a rectangle from where the pen went down.
                    val x0 = lasso[0]; val y0 = lasso[1]
                    lasso.clear(); lasso.addAll(listOf(x0, y0, wx, y0, wx, wy, x0, wy))
                    if (!batch) invalidate()
                } else if (n < 2 || hypot(wx - lasso[n - 2], wy - lasso[n - 1]) > 3f / scale) { lasso += wx; lasso += wy; if (!batch) invalidate() }
            }
            Action.MOVE_SEL -> {
                selDx = (x - actionDownX) / scale
                selDy = (y - actionDownY) / scale
                if (!batch) invalidate()
            }
            Action.PLOT_BOX -> {
                val pb = plotBox ?: return
                val p = pb.page ?: return
                pb.x1 = if (p.isBoard) wx - p.rect.left else (wx - p.rect.left).coerceIn(0f, p.rect.width())
                pb.y1 = if (p.isBoard) wy - p.rect.top else (wy - p.rect.top).coerceIn(0f, p.rect.height())
                if (!batch) invalidate()
            }
            Action.END_SEL -> {
                val sel = selection ?: return
                val p = placedFor(sel.sheetId) ?: return
                val e = selEnds ?: return
                // Along the line only its length changes; pulled clearly sideways it turns,
                // snapping like a held line.
                val ax = if (endWhich == 0) e[2] else e[0]; val ay = if (endWhich == 0) e[3] else e[1]
                val ox = (if (endWhich == 0) e[0] else e[2]) - ax; val oy = (if (endWhich == 0) e[1] else e[3]) - ay
                val d = LineGrip.drag(ox, oy, wx - p.rect.left - ax, wy - p.rect.top - ay, endTurning, END_TURN_DP * density / scale)
                endTurning = d.turning
                if (d.snapDeg != endSnapDeg) {
                    if (d.snapDeg != null) performHapticFeedback(android.view.HapticFeedbackConstants.CLOCK_TICK)
                    endSnapDeg = d.snapDeg
                }
                endX = ax + d.x; endY = ay + d.y
                if (!batch) invalidate()
            }
            Action.GRIP_SEL -> {
                val sel = selection ?: return
                val p = placedFor(sel.sheetId) ?: return
                moveGrip(sel, toWorldX(x) - p.rect.left, toWorldY(y) - p.rect.top)
                if (!batch) invalidate()
            }
            Action.SCALE_SEL -> {
                val sel = selection ?: return
                // The box around every page's part (it scales about that box; the first part
                // alone made the handle run ahead of the pen).
                val w = selUnion(sel).width().coerceAtLeast(4f)
                selScale = ((w + (x - actionDownX) / scale) / w).coerceIn(0.2f, 6f)
                if (!batch) invalidate()
            }
            Action.HANDLE -> {
                val h = handle ?: return
                val dx = x - actionDownX; val dy = y - actionDownY
                handleDrag = when (h.dir) { -1 -> -dx; 1 -> dx; else -> dy }
                // Holding: moving off the "+" (a stroke that merely started on it) cancels.
                if (Settings.handleHoldMs > 0 && hypot(dx, dy) > 28f * density) { removeCallbacks(handleConfirm); handle = null; invalidate(); return }
                if (!batch) invalidate()
            }
            Action.TAP, Action.NONE -> {}
        }
    }

    private fun pointerUp(ev: MotionEvent, idx: Int, canceled: Boolean) {
        val id = ev.getPointerId(idx)
        val now = ev.eventTime
        if (id == actionId) {
            if (action == Action.INK && !canceled) {
                actionMove(ev.getX(idx), ev.getY(idx), ev.getPressure(idx), now, batch = false)
            }
            if (action == Action.TAP && now - actionDownAt < 450 && hypot(ev.getX(idx) - actionDownX, ev.getY(idx) - actionDownY) < touchSlop * 2) {
                val wx = toWorldX(ev.getX(idx)); val wy = toWorldY(ev.getY(idx))
                model?.headers?.firstOrNull { it.rect.contains(wx, wy) }?.let { headerTap(it, wx, ev.getX(idx), ev.getY(idx)) }
            }
            endAction(now, commit = !canceled || actionStylus)
        }
        if (id == penId) {
            penId = -1
            penUpAt = SystemClock.uptimeMillis()
            if (id == loupeGrabId) endLoupeGrab()
            return
        }
        if (id == loupePinchId) { loupePinchId = -1; return }
        // Cancelled (a palm Android took back): no tap in the strip.
        if (id == loupeGrabId) { if (canceled) { loupeGrabId = -1; loupeGrab = Grab.NONE } else endLoupeGrab(); return }
        val f = fingers.remove(id) ?: return
        // Nor a tap on the page (it cleared the selection, folded a group, fitted the page).
        if (!canceled && !f.rejected && gesture == Gesture.PAN_PENDING && id == panId && now - f.downAt < 400 &&
            hypot(f.x - f.downX, f.y - f.downY) < touchSlop
        ) onTap(f.x, f.y)
        if (!f.rejected) {
            if (gesture == Gesture.PAN && id == panId) {
                if (lock != Lock.NONE && board == null) endStretch(id) else startFling(id)
                panTracking = false
            }
            if (gesture == Gesture.PINCH || id == panId || (gesture == Gesture.RULER && (id == rulerA || id == rulerB))) gesture = Gesture.NONE
            if (activeFingers().isNotEmpty()) {
                activeFingersChanged(now)
                if (gesture == Gesture.PAN_PENDING) gesture = Gesture.PAN
            }
        }
        if (fingers.isEmpty()) {
            afterPinch = false
            gesture = Gesture.NONE
            velocity?.recycle(); velocity = null
        }
    }

    /** A finger tap that did not move (pen-only mode): fold a group, or drop the selection. */
    private fun onTap(x: Float, y: Float) {
        val m = model ?: return
        // A finger tap in the Mitwandern band moves on, like a pen tap there.
        flowPage()?.let { fp ->
            if (flowBand(fp).contains(x, y)) {
                flowSheet = fp.sheetId; flowLiftX = toWorldX(x)
                if (flowTop.isNaN()) { flowTop = toWorldY(y) - 6f; flowBottom = toWorldY(y) + 6f }
                writeFlow()
                return
            }
        }
        eyedropper?.let { pick -> eyedropper = null; pickColorAt(x.toInt(), y.toInt())?.let(pick); return }
        val wx = toWorldX(x); val wy = toWorldY(y)
        val now = SystemClock.uptimeMillis()
        val double = now - lastTapAt < DOUBLE_TAP_MS && hypot(x - lastTapX, y - lastTapY) < 60f * density
        lastTapAt = if (double) 0L else now
        lastTapX = x; lastTapY = y
        // Text tool with fingers that only scroll (pen-only mode): a tap edits or creates text.
        if (tool == Tool.TEXT) {
            val p = m.placed.firstOrNull { it.rect.contains(wx, wy) }
            if (p != null && !p.isBoard) {
                val lx = wx - p.rect.left; val ly = wy - p.rect.top
                listener?.onTextTap(p, textAt(p, lx, ly)?.id, lx, ly)
                return
            }
        }
        if (double && Settings.doubleTapFit) {
            // Double tap: fit the page to the screen (a board opens full screen).
            if (board != null) { board?.let { b -> animateTo(scale, width / 2f - b.rect.centerX() * scale, height / 2f - b.rect.centerY() * scale) }; return }
            val p = m.placed.firstOrNull { it.rect.contains(wx, wy) } ?: return
            if (p.isBoard) enterBoard(p) else fitSheet(p) // with the zoom lock on, the lock follows the new fit
            return
        }
        m.headers.firstOrNull { it.rect.contains(wx, wy) }?.let { headerTap(it, wx, x, y); return }
        if (selection != null) clearSelection()
    }

    /** End of a locked pan: a long enough sideways pull moves to the side page there. */
    private fun endStretch(id: Int) {
        val pull = stretch * 0.35f
        stretch = 0f
        panTracking = false
        val m = model
        val cur = currentSheet()
        var switched = 0
        if (m != null && cur != null && abs(pull) > width * 0.12f) {
            val row = m.placed.filter { it.mainNode == cur.node }.sortedBy { it.rect.centerX() }
            val i = row.indexOfFirst { abs(it.rect.centerX() - lockCenterX) < 1f }.coerceAtLeast(0)
            val next = row.getOrNull(if (pull > 0) i - 1 else i + 1)
            if (next != null) { lockCenterX = next.rect.centerX(); switched = if (pull > 0) -1 else 1 }
        }
        // Vertical fling stays; horizontally it glides inside a zoomed-in page (bounded by its
        // edges, Settings.lockMomentum) and snaps otherwise.
        val r0 = lockSheet()?.rect
        val margin0 = 12f * density
        val glide = switched == 0 && !zoomLocked && Settings.lockMomentum && r0 != null && r0.width() * scale + 2 * margin0 > width &&
            tx in (width - margin0 - r0.right * scale)..(margin0 - r0.left * scale)
        velocity?.let { v ->
            v.computeCurrentVelocity(1000, maxFling)
            val vy = v.getYVelocity(id)
            val vx = if (glide) v.getXVelocity(id) else 0f
            if (abs(vy) > 400f || abs(vx) > 400f) {
                lastFlingX = 0; lastFlingY = 0
                if (glide && r0 != null) {
                    val lo = (width - margin0 - r0.right * scale - tx).toInt(); val hi = (margin0 - r0.left * scale - tx).toInt()
                    scroller.fling(0, 0, vx.toInt(), vy.toInt(), lo, hi, Int.MIN_VALUE, Int.MAX_VALUE)
                } else scroller.fling(0, 0, 0, vy.toInt(), 0, 0, Int.MIN_VALUE, Int.MAX_VALUE)
            }
        }
        if (glide) { postInvalidateOnAnimation(); return }
        animateTx(if (zoomLocked) width / 2f - lockCenterX * scale else {
            // Page lock: arriving from the right shows the new page's right edge, and vice versa.
            val r = lockSheet()?.rect
            when {
                r == null -> tx
                switched == 1 -> pageTx(Float.MAX_VALUE, scale)
                switched == -1 -> pageTx(-Float.MAX_VALUE, scale)
                else -> pageTx(tx, scale)
            }
        })
    }

    /** Animates only the horizontal position (a running vertical fling continues). */
    private fun animateTx(tx1: Float) {
        anim?.cancel()
        val x0 = tx
        anim = android.animation.ValueAnimator.ofFloat(0f, 1f).setDuration(220).apply {
            interpolator = android.view.animation.DecelerateInterpolator(1.6f)
            addUpdateListener { tx = x0 + (tx1 - x0) * (it.animatedValue as Float); changed() }
            start()
        }
        postInvalidateOnAnimation()
    }

    /** The colour on screen at (x, y) (the view drawn into a bitmap). */
    private fun pickColorAt(x: Int, y: Int): Int? = runCatching {
        val bmp = android.graphics.Bitmap.createBitmap(width, height, android.graphics.Bitmap.Config.ARGB_8888)
        draw(Canvas(bmp))
        val c = bmp.getPixel(x.coerceIn(0, width - 1), y.coerceIn(0, height - 1))
        bmp.recycle()
        c
    }.getOrNull()

    private fun startFling(id: Int) {
        val v = velocity ?: return
        v.computeCurrentVelocity(1000, maxFling)
        val vx = v.getXVelocity(id); val vy = v.getYVelocity(id)
        if (abs(vx) + abs(vy) < 400f) return
        lastFlingX = 0; lastFlingY = 0
        scroller.fling(0, 0, vx.toInt(), vy.toInt(), Int.MIN_VALUE, Int.MAX_VALUE, Int.MIN_VALUE, Int.MAX_VALUE)
        postInvalidateOnAnimation()
    }

    private fun endAction(now: Long, commit: Boolean) {
        val m = model
        val p = target
        when (action) {
            Action.INK -> if (commit && m != null && p != null && count > 0) commitInk(m, p, now) else cancelInk()
            // Not committed (a palm found out late): what it erased comes back.
            Action.ERASE -> if (m != null && p != null) { if (commit) endErase(m, p) else cancelErase(m, p) }
            Action.LASSO -> if (m != null) finishLasso(m)
            Action.SPACE -> finishSpace(m, commit)
            Action.PRE_INK -> preInk.clear()
            Action.MOVE_SEL, Action.SCALE_SEL -> finishSelDrag()
            Action.PLOT_BOX -> finishPlotBox(commit)
            Action.GRIP_SEL -> finishGrip()
            Action.END_SEL -> finishEndDrag()
            Action.HANDLE -> {
                val h = handle
                handle = null
                removeCallbacks(handleConfirm)
                // Instant: a tap or a drag outward creates the page; dragging back cancels. With a
                // hold time, letting go before the ring closed cancels (the page came at the close).
                if (Settings.handleHoldMs <= 0 && h != null && commit && handleDrag > -40f * density) listener?.onAddPage(h.dir, h.mainNode)
            }
            Action.LASER -> { laserUpAt = SystemClock.uptimeMillis(); laserEnded[laserSeg] = laserUpAt; postInvalidateOnAnimation() }
            Action.TEXT -> if (m != null && p != null) {
                val hit = textHit
                if (textMoved && hit != null) {
                    m.sheets[p.sheetId]?.texts?.firstOrNull { it.id == hit.id }?.let { b -> m.placeTextBox(p.sheetId, b.id, b.x, b.y, b.w, b.size) }
                } else if (commit) {
                    listener?.onTextTap(p, hit?.id, textDownLx, textDownLy)
                }
                textHit = null
            }
            Action.TAP, Action.NONE -> {}
        }
        if (action != Action.INK) { action = Action.NONE; actionId = -1 }
        if (action == Action.NONE) mapLoupe = false
        invalidate()
    }

    private fun cancelAll() {
        if (action == Action.INK && !actionStylus) cancelInk()
        else if (action != Action.NONE) endAction(SystemClock.uptimeMillis(), commit = actionStylus)
        action = Action.NONE
        actionId = -1
        penId = -1
        fingers.clear()
        gesture = Gesture.NONE
        afterPinch = false
        // A magnifier grab or a locked pan cut off (Android cancels a palm): nothing of it left
        // over, else the next gesture's first pointer (often the pen) slid the magnifier.
        loupeGrabId = -1; loupeGrab = Grab.NONE; loupePinchId = -1
        panTracking = false
        stretch = 0f
        invalidate()
    }

    private fun rejectAllFingers() {
        var any = false
        for (f in fingers.values) if (!f.rejected) { f.rejected = true; any = true }
        if (action != Action.NONE && !actionStylus) {
            // A finger stroke that started just before the pen arrived was the palm.
            if (action == Action.INK && SystemClock.uptimeMillis() - strokeStart < 1500) cancelInk() else endAction(SystemClock.uptimeMillis(), commit = false)
        }
        if (any) gesture = Gesture.NONE
    }

    // --- ink -----------------------------------------------------------------------------------

    // --- writing flow ("Mitwandern") ----------------------------------------------------------
    //
    // Predictable: while writing zoomed in, a band at the right edge shows where it acts. Only
    // lifting the pen inside it moves the view (a pause alone never does):
    // - more page to the right: the page slides left, the writing ends up at a third of the screen;
    // - at the page's right edge: one line down (lined paper: one rule; grid and dots: whole boxes
    //   for the height written) and back to the page's left edge.
    // Blank paper has no lines to keep to: there it only slides.

    /** The line being written (world coordinates): its top and bottom, and where the pen last lifted. */
    private var flowSheet: String? = null
    private var flowTop = Float.NaN
    private var flowBottom = Float.NaN
    private var flowLiftX = 0f
    private val flowCheck = Runnable { writeFlow() }

    /** The page "Mitwandern" acts on right now: the last written one if in view, else the one in the middle. */
    private fun flowPage(): Placed? {
        if (!Settings.writeFlow || board != null || loupeOpen || tool != Tool.PEN || navigateOnly || readOnly) return null
        val m = model ?: return null
        val vis = RectF(toWorldX(0f), toWorldY(0f), toWorldX(width.toFloat()), toWorldY(height.toFloat()))
        val p = m.placed.firstOrNull { it.sheetId == flowSheet && RectF.intersects(it.rect, vis) } ?: currentRowSheetAtCenter() ?: return null
        // Only zoomed in: a page that fits needs no moving.
        return p.takeIf { !it.isBoard && it.rect.width() * scale > width * 1.08f }
    }

    /** The band (screen pixels) at the right edge of the visible page where a pen lift moves on. */
    private fun flowBand(p: Placed): RectF {
        val right = min(width.toFloat(), p.rect.right * scale + tx)
        val w = max(width * 0.13f, 72f * density)
        return RectF(right - w, max(0f, p.rect.top * scale + ty), right, min(height.toFloat(), p.rect.bottom * scale + ty))
    }

    /** How far the next line is: one rule on lined paper, whole boxes on grid and dots, null on blank paper. */
    private fun lineStep(p: Placed): Float? {
        val sp = p.spec.spacing.takeIf { it >= 4f } ?: 20f
        val h = if (flowTop.isNaN()) 0f else max(flowBottom - flowTop, 0f)
        return when (p.spec.background) {
            FfiBackground.LINES, FfiBackground.CORNELL -> sp * max(1f, kotlin.math.floor(h / sp + 0.35f))
            FfiBackground.GRID, FfiBackground.DOTS -> sp * max(1f, kotlin.math.ceil(h * 1.15f / sp))
            else -> null
        }
    }

    /** After a stroke: extend the line (or start a new one); a lift inside the band moves on shortly after. */
    private fun trackFlow(p: Placed, n: Int) {
        removeCallbacks(flowCheck)
        if (n == 0) return
        var t = Float.MAX_VALUE; var b = -Float.MAX_VALUE
        for (i in 0 until n) { val y = samples[i * 4 + 1] + p.rect.top; t = min(t, y); b = max(b, y) }
        val lineH = if (flowTop.isNaN()) 12f else max(flowBottom - flowTop, 12f)
        val sameLine = flowSheet == p.sheetId && !flowTop.isNaN() && t < flowBottom + lineH * 0.6f && b > flowTop - lineH * 0.6f
        if (sameLine) { flowTop = min(flowTop, t); flowBottom = max(flowBottom, b) } else { flowSheet = p.sheetId; flowTop = t; flowBottom = b }
        val lx = samples[(n - 1) * 4] + p.rect.left; val ly = samples[(n - 1) * 4 + 1] + p.rect.top
        flowLiftX = lx
        val page = flowPage()?.takeIf { it.sheetId == p.sheetId } ?: return
        // A short wait: a dot on the i or a crossed t right after starts the next stroke and cancels it.
        if (flowBand(page).contains(lx * scale + tx, ly * scale + ty)) postDelayed(flowCheck, 380)
    }

    private fun writeFlow() {
        if (action != Action.NONE || gesture != Gesture.NONE) return
        val p = flowPage()?.takeIf { it.sheetId == flowSheet } ?: return
        val edge = 12f * density
        if (p.rect.right * scale + tx > width + 2f) {
            // More page to the right: slide it left under the pen.
            val nx = max(tx - (flowLiftX * scale + tx - width * 0.33f), width - edge - p.rect.right * scale)
            if (nx < tx - 2f) animateTo(scale, nx, ty)
        } else {
            val step = lineStep(p) ?: return
            flowTop += step; flowBottom += step
            animateTo(scale, edge - p.rect.left * scale, ty - step * scale)
        }
    }

    private val flowFill = Paint()
    private val flowEdge = Paint(Paint.ANTI_ALIAS_FLAG).apply { style = Paint.Style.STROKE }
    private val flowGlyph = Paint(Paint.ANTI_ALIAS_FLAG).apply { textAlign = Paint.Align.CENTER; typeface = android.graphics.Typeface.DEFAULT_BOLD }

    /** The band, faint, while the pen is near: "→" slides on, "↵" goes to the next line. */
    private fun drawFlowBand(c: Canvas) {
        if (!penNear(SystemClock.uptimeMillis()) && action != Action.INK) return
        val p = flowPage() ?: return
        // Shown during the grace after the pen left: look again once it is over, to hide it.
        if (!hovering && penId == -1 && action != Action.INK) postInvalidateDelayed(HOVER_GRACE_MS + 60)
        val band = flowBand(p)
        if (band.height() <= 0f) return
        val accent = Ui.theme.accent.toInt()
        flowFill.color = accent; flowFill.alpha = 0x12
        c.drawRect(band, flowFill)
        flowEdge.color = accent; flowEdge.alpha = 0x50; flowEdge.strokeWidth = 1.5f * density
        flowEdge.pathEffect = DashPathEffect(floatArrayOf(6f * density, 5f * density), 0f)
        c.drawLine(band.left, band.top, band.left, band.bottom, flowEdge)
        val more = p.rect.right * scale + tx > width + 2f
        val glyph = if (more) "→" else if (lineStep(p) != null) "↵" else return
        val y = if (!flowTop.isNaN() && flowSheet == p.sheetId) ((flowTop + flowBottom) / 2f) * scale + ty else band.centerY()
        flowGlyph.color = accent; flowGlyph.alpha = 0xB0; flowGlyph.textSize = 22f * density
        c.drawText(glyph, band.centerX(), clampTo(y, band.top + 30f * density, band.bottom - 10f * density), flowGlyph)
    }

    // --- the writing magnifier ("Schreiblupe") ---------------------------------------------
    //
    // A strip at the bottom shows a frame of one page large; writing in the strip lands in the
    // frame, small. The page never moves under the pen. Lifting the pen in the strip's right part
    // moves the frame on (at the page's right edge: next line, back to where the line began). A
    // finger in the strip slides the frame; a pen tap on the page above puts the frame there.

    var loupeOpen = false
        private set
    private var loupeSheet: String? = null
    /** The frame (page points, top-left and width; its height follows the strip's shape). */
    private var loupeX = 0f
    private var loupeY = 0f
    private var loupeW = 200f
    /** Where the frame's line began (a new line goes back there), the ink written on it. */
    private var loupeLineX = 0f
    private var loupeInkTop = Float.NaN
    private var loupeInkBottom = Float.NaN
    /** The running action writes through the magnifier. */
    private var mapLoupe = false
    /** A finger or pen holding part of the magnifier: what it holds and where it was. */
    private var loupeGrabId = -1
    private var loupeGrab = Grab.NONE
    private var loupeGrabX = 0f
    private var loupeGrabY = 0f
    private var loupeGrabDy = 0f
    /** A finger in the strip: when and where it went down, how far it moved (a tap moves on). */
    private var loupeGrabAt = 0L
    private var loupeGrabDownX = 0f
    private var loupeGrabMoved = 0f
    /** A second finger in the strip: pinching sets how large one writes. */
    private var loupePinchId = -1
    private var loupePinchDist = 0f
    /** The strip's place while it is being moved or resized (px); NaN: from the settings. */
    private var padTopPx = Float.NaN
    private var padHPx = Float.NaN
    /** Where the last stroke in the strip ended (page points): past the middle, the move-on part shows stronger. */
    private var loupeLastX = Float.NaN

    private enum class Grab { NONE, PAD, FRAME, CORNER, PAD_MOVE, PAD_SIZE, BAR_H, BAR_V }

    /**
     * Scroll bars in the strip: where the frame is across the page (bottom) and down it (left).
     * A finger on one takes the frame anywhere on the page. Tracks, then the touch areas.
     */
    private fun barTrackH(s: RectF) = RectF(s.left + 34f * density, s.bottom - 11f * density, s.right - 22f * density, s.bottom - 7f * density)
    private fun barTrackV(s: RectF) = RectF(s.left + 7f * density, s.top + 24f * density, s.left + 11f * density, s.bottom - 30f * density)
    private fun barTouchH(s: RectF) = RectF(s.left + 30f * density, s.bottom - 30f * density, s.right, s.bottom)
    private fun barTouchV(s: RectF) = RectF(s.left, s.top + 20f * density, s.left + 30f * density, s.bottom - 30f * density)

    /** The strip's grip (a pill on its top edge: moves it) and height handle (its top-right corner). */
    private fun padGrip(): RectF { val s = loupeStrip(); val w = 34f * density; return RectF(s.centerX() - w, s.top - 16f * density, s.centerX() + w, s.top + 16f * density) }
    private fun padSizer(): RectF { val s = loupeStrip(); return RectF(s.right - 52f * density, s.top - 18f * density, s.right - 12f * density, s.top + 18f * density) }
    private val loupeAdvance = Runnable { loupeMoveOn() }
    /** While the strip draws: its scale (paper hairlines); 0 otherwise. */
    private var loupeDrawScale = 0f

    /** The strip (screen pixels): where it was put (default: at the bottom), 30 % high, inset 16 dp. */
    fun loupeStrip(): RectF {
        val m = 16f * density
        val minTop = 110f * density
        val h = (if (padHPx.isNaN()) height * Settings.loupePadH else padHPx).coerceIn(140f * density, max(140f * density, height * 0.6f))
        val wanted = if (!padTopPx.isNaN()) padTopPx else if (Settings.loupePadTop < 0f) height - m - h else height * Settings.loupePadTop
        val top = wanted.coerceIn(minTop, max(minTop, height - m - h))
        return RectF(m, top, width - m, top + h)
    }

    /** The strip moved by [dy] px (its grip). */
    fun movePad(dy: Float) {
        val s = loupeStrip()
        padTopPx = s.top + dy; padHPx = s.height()
        loupePage()?.let { clampFrame(it) }
        invalidate()
    }

    /** The strip's top edge moved by [dy] px (its height handle); the bottom stays. */
    fun resizePad(dy: Float) {
        val s = loupeStrip()
        val h = clampTo(s.height() - dy, 140f * density, height * 0.6f)
        padHPx = h; padTopPx = s.bottom - h
        loupePage()?.let { clampFrame(it); snapFrameY(it) }
        invalidate()
    }

    /** The strip let go: remembered for next time. */
    fun padDone() {
        val s = loupeStrip()
        Settings.set("loupe_pad_top", s.top / height)
        Settings.set("loupe_pad_h", s.height() / height)
        padTopPx = Float.NaN; padHPx = Float.NaN
        followFrame()
        invalidate()
    }

    private fun loupePage(): Placed? = loupeSheet?.let { id -> model?.placed?.firstOrNull { it.sheetId == id && !it.isBoard } }
    private fun loupeH(s: RectF = loupeStrip()) = loupeW * s.height() / s.width()
    private fun loupeScale(s: RectF = loupeStrip()) = s.width() / loupeW
    private fun loupeTx(p: Placed, s: RectF = loupeStrip()) = s.left - (p.rect.left + loupeX) * loupeScale(s)
    private fun loupeTy(p: Placed, s: RectF = loupeStrip()) = s.top - (p.rect.top + loupeY) * loupeScale(s)

    /** The mapping of the running action: through the magnifier or the page view. */
    private val aScale get() = if (mapLoupe) loupeScale() else scale
    // The magnifier's page looked up only when mapping through it (not for every pen sample of every stroke).
    private val aTx get() = if (!mapLoupe) tx else loupePage()?.let { loupeTx(it) } ?: tx
    private val aTy get() = if (!mapLoupe) ty else loupePage()?.let { loupeTy(it) } ?: ty
    private fun aWorldX(sx: Float) = (sx - aTx) / aScale
    private fun aWorldY(sy: Float) = (sy - aTy) / aScale

    /** Opens the magnifier with its frame at screen (x, y) (the finger-hold menu). */
    fun openLoupeAt(x: Float, y: Float): Boolean {
        val (p, pt) = pageAt(x, y) ?: return false
        if (board != null) return false
        loupeSheet = p.sheetId
        loupeW = clampTo(p.rect.width() * Settings.loupeShare, 40f, p.rect.width())
        loupeOpen = true
        moveLoupeTo(p, pt.x, pt.y)
        return true
    }

    /** Opens the magnifier on the page in view, at its left margin, where the screen's middle is. */
    fun openLoupe(): Boolean {
        val p = focusedSheet()?.takeIf { !it.isBoard && board == null } ?: return false
        loupeSheet = p.sheetId
        loupeW = clampTo(p.rect.width() * Settings.loupeShare, 40f, p.rect.width())
        loupeX = p.rect.width() * 0.06f
        val strip = loupeStrip()
        loupeY = (toWorldY(min(strip.top, height.toFloat()) * 0.45f) - p.rect.top).coerceIn(0f, max(0f, p.rect.height() - loupeH(strip)))
        loupeLineX = loupeX
        loupeInkTop = Float.NaN; loupeInkBottom = Float.NaN
        loupeOpen = true
        snapFrameY(p)
        followFrame()
        invalidate()
        return true
    }

    /** The frame back or on along the line (the arrows on the strip). */
    fun loupeStep(forward: Boolean) {
        val p = loupePage() ?: return
        removeCallbacks(loupeAdvance)
        if (forward) { loupeMoveOn(); return }
        loupeX = max(0f, loupeX - loupeW * 0.62f)
        clampFrame(p)
        followFrame()
        invalidate()
    }

    /** Rules of the page's paper (page points: offset, spacing), or null on blank paper. */
    private fun rules(p: Placed): Pair<Float, Float>? {
        val sp = p.spec.spacing.takeIf { it >= 4f } ?: 20f
        return when (p.spec.background) {
            FfiBackground.LINES, FfiBackground.GRID, FfiBackground.DOTS -> 0f to sp
            FfiBackground.CORNELL -> 90f to sp
            else -> null
        }
    }

    /** On ruled, grid and dot paper the frame sits on a line: the rule nearest its writing height at 72 % of the frame. */
    private fun snapFrameY(p: Placed) {
        val (off, sp) = rules(p) ?: return
        val fh = loupeH()
        val base = loupeY + fh * LOUPE_BASELINE
        loupeY = off + kotlin.math.round((base - off) / sp) * sp - fh * LOUPE_BASELINE
        clampFrame(p)
    }

    fun closeLoupe() {
        if (!loupeOpen) return
        loupeOpen = false
        removeCallbacks(loupeAdvance)
        loupeGrabId = -1; loupeGrab = Grab.NONE; loupePinchId = -1
        invalidate()
        listener?.onLoupeClosed()
    }

    /** The frame's next line: one rule (lined), whole boxes for what was written (grid, dots), else a line's height. */
    fun loupeNewLine() {
        val p = loupePage() ?: return
        removeCallbacks(loupeAdvance)
        val sp = p.spec.spacing.takeIf { it >= 4f } ?: 20f
        val h = if (loupeInkTop.isNaN()) 0f else loupeInkBottom - loupeInkTop
        val step = when (p.spec.background) {
            FfiBackground.LINES, FfiBackground.CORNELL -> sp * max(1f, kotlin.math.floor(h / sp + 0.35f))
            FfiBackground.GRID, FfiBackground.DOTS -> sp * max(1f, kotlin.math.ceil(max(h * 1.15f, loupeH() * 0.4f) / sp))
            else -> max(h * 1.35f, loupeH() * 0.45f)
        }
        val maxY = p.rect.height() - loupeH()
        if (loupeY >= maxY - 0.5f) { listener?.onMessage("Ende der Seite"); return }
        loupeY = (loupeY + step).coerceAtMost(maxY)
        loupeX = loupeLineX
        loupeInkTop = Float.NaN; loupeInkBottom = Float.NaN
        loupeLastX = Float.NaN
        clampFrame(p)
        snapFrameY(p)
        followFrame()
        invalidate()
    }

    /** Moves the frame on along the line; at the page's right edge to the next line. */
    private fun loupeMoveOn() {
        val p = loupePage() ?: return
        if (action != Action.NONE) return
        val right = p.rect.width() * 0.97f
        if (loupeX + loupeW >= right - 1f) { loupeNewLine(); return }
        loupeX = min(loupeX + loupeW * 0.62f, right - loupeW)
        loupeLastX = Float.NaN
        clampFrame(p)
        followFrame()
        invalidate()
    }

    private fun clampFrame(p: Placed) {
        loupeX = loupeX.coerceIn(0f, max(0f, p.rect.width() - loupeW))
        loupeY = loupeY.coerceIn(0f, max(0f, p.rect.height() - loupeH()))
    }

    /** A pen tap on the page above the strip: the frame goes there (the tap at its left, a little above the middle). */
    private fun moveLoupeTo(p: Placed, lx: Float, ly: Float) {
        loupeSheet = p.sheetId
        loupeW = loupeW.coerceAtMost(p.rect.width())
        loupeX = lx - loupeW * 0.04f
        loupeY = ly - loupeH() * LOUPE_BASELINE
        clampFrame(p)
        snapFrameY(p)
        loupeLineX = loupeX
        loupeInkTop = Float.NaN; loupeInkBottom = Float.NaN
        followFrame()
        invalidate()
    }

    /** Keeps the frame in view above the strip. */
    private fun followFrame() {
        val p = loupePage() ?: return
        val strip = loupeStrip()
        val l = (p.rect.left + loupeX) * scale + tx; val r = (p.rect.left + loupeX + loupeW) * scale + tx
        val t = (p.rect.top + loupeY) * scale + ty; val b = (p.rect.top + loupeY + loupeH(strip)) * scale + ty
        // The room the strip leaves: above it, or below it when it was moved up.
        val above = strip.top - 90f * density; val below = height - strip.bottom - 12f * density
        val top = if (above >= below) 90f * density else strip.bottom + 12f * density
        val bottom = if (above >= below) strip.top - 12f * density else height - 12f * density
        var nx = tx; var ny = ty
        if (l < 12f * density || r > width - 12f * density) nx = width / 2f - (p.rect.left + loupeX + loupeW / 2f) * scale
        if (t < top || b > bottom) ny = top + (bottom - top) * 0.3f - (p.rect.top + loupeY) * scale
        if (abs(nx - tx) > 1f || abs(ny - ty) > 1f) animateTo(scale, nx, ny)
    }

    /** Pen down in the strip: ink or eraser into the frame's page. */
    private fun beginLoupeAction(x: Float, y: Float, pressure: Float, now: Long, erasing: Boolean) {
        removeCallbacks(loupeAdvance)
        val p = loupePage()
        if (p == null || (!erasing && tool != Tool.PEN && tool != Tool.MARKER)) { action = Action.NONE; target = null; mapLoupe = false; return }
        if (selection != null) clearSelection()
        target = p
        snapEdge = 0
        val lx = aWorldX(x) - p.rect.left; val ly = aWorldY(y) - p.rect.top
        if (erasing) { action = Action.ERASE; beginErase(p, lx, ly, now) } else startInk(lx, ly, pressure, now, stylus = true)
    }

    /** A stroke written in the strip: the line's ink grows; lifted in the right part, the frame moves on shortly after. */
    private fun loupeAfterStroke(p: Placed, n: Int) {
        for (i in 0 until n) {
            val y = samples[i * 4 + 1]
            loupeInkTop = if (loupeInkTop.isNaN()) y else min(loupeInkTop, y)
            loupeInkBottom = if (loupeInkBottom.isNaN()) y else max(loupeInkBottom, y)
        }
        val lastX = samples[(n - 1) * 4]
        loupeLastX = lastX
        if (p.sheetId == loupeSheet && lastX > loupeX + loupeW * (1f - LOUPE_ADVANCE)) postDelayed(loupeAdvance, 450)
    }

    /** A finger or the pen down at screen (x, y) on the frame (move; only fingers) or its corner (resize). */
    private fun grabLoupe(id: Int, x: Float, y: Float, pen: Boolean): Boolean {
        if (loupeGrabId != -1) return false
        val f = frameScreen() ?: return false
        val grab = when {
            padGrip().contains(x, y) -> Grab.PAD_MOVE
            padSizer().contains(x, y) -> Grab.PAD_SIZE
            hypot(x - f.right, y - f.bottom) < 26f * density -> Grab.CORNER
            !pen && RectF(f).apply { inset(-8f * density, -8f * density) }.contains(x, y) -> Grab.FRAME
            else -> return false
        }
        removeCallbacks(loupeAdvance)
        loupeGrabId = id; loupeGrab = grab; loupeGrabX = x; loupeGrabY = y; loupeGrabDy = 0f
        return true
    }

    private fun moveLoupeGrab(ev: MotionEvent) {
        val i = ev.findPointerIndex(loupeGrabId)
        val p = loupePage()
        if (i < 0 || p == null) return
        val x = ev.getX(i); val y = ev.getY(i)
        val dx = x - loupeGrabX; val dy = y - loupeGrabY
        loupeGrabX = x; loupeGrabY = y
        when (loupeGrab) {
            Grab.PAD -> {
                val j = if (loupePinchId != -1) ev.findPointerIndex(loupePinchId) else -1
                if (j >= 0) {
                    // Two fingers: spreading writes larger (the frame narrows), about its left edge.
                    val d = hypot(ev.getX(j) - x, ev.getY(j) - y).coerceAtLeast(1f)
                    loupeW = (loupeW * loupePinchDist / d).coerceIn(p.rect.width() * 0.12f, p.rect.width() * 0.8f)
                    loupePinchDist = d
                } else {
                    // One finger slides the paper under the window.
                    loupeGrabMoved += hypot(dx, dy)
                    if (loupeGrabMoved < touchSlop) return
                    val k = loupeScale()
                    loupeX -= dx / k; loupeY -= dy / k
                    loupeGrabDy -= dy / k
                }
            }
            Grab.FRAME -> { loupeX += dx / scale; loupeY += dy / scale; loupeGrabDy += dy / scale }
            Grab.CORNER -> loupeW = ((p.rect.left + loupeX + loupeW) + dx / scale - (p.rect.left + loupeX)).coerceIn(p.rect.width() * 0.12f, p.rect.width() * 0.8f)
            Grab.BAR_H, Grab.BAR_V -> { scrubTo(x, y); return }
            Grab.PAD_MOVE -> { movePad(dy); listener?.onLoupePadMoved(); return }
            Grab.PAD_SIZE -> { resizePad(dy); listener?.onLoupePadMoved(); return }
            Grab.NONE -> {}
        }
        clampFrame(p)
        invalidate()
    }

    /** A finger on a scroll bar at (x, y): the frame's middle goes there on the page (absolute, like a scroll bar). */
    private fun scrubTo(x: Float, y: Float) {
        val p = loupePage() ?: return
        val s = loupeStrip()
        val y0 = loupeY
        if (loupeGrab == Grab.BAR_H) {
            val t = barTrackH(s)
            loupeX = ((x - t.left) / t.width()).coerceIn(0f, 1f) * p.rect.width() - loupeW / 2f
        } else {
            val t = barTrackV(s)
            loupeY = ((y - t.top) / t.height()).coerceIn(0f, 1f) * p.rect.height() - loupeH(s) / 2f
        }
        clampFrame(p)
        loupeGrabDy += loupeY - y0
        invalidate()
    }

    private fun endLoupeGrab() {
        val p = loupePage()
        val g = loupeGrab
        loupeGrabId = -1; loupeGrab = Grab.NONE; loupePinchId = -1
        if (g == Grab.PAD_MOVE || g == Grab.PAD_SIZE) { padDone(); listener?.onLoupePadMoved(); return }
        if (p == null) return
        // A finger tap in the strip's move-on part moves on (at the line's end: the next line).
        if (g == Grab.PAD && loupeGrabMoved < touchSlop && SystemClock.uptimeMillis() - loupeGrabAt < TAP_MS) {
            val s = loupeStrip()
            if (loupeGrabDownX > s.right - s.width() * LOUPE_ADVANCE) loupeMoveOn()
            return
        }
        if (g == Grab.CORNER || g == Grab.PAD) Settings.set("loupe_share", loupeW / p.rect.width())
        // Moved to another line: lines begin here now; along the same line the start stays.
        if (abs(loupeGrabDy) > loupeH() * 0.5f) { loupeLineX = loupeX; loupeInkTop = Float.NaN; loupeInkBottom = Float.NaN }
        if (loupeX < loupeLineX) loupeLineX = loupeX
        snapFrameY(p)
        followFrame()
        invalidate()
    }

    private val loupeFramePaint = Paint(Paint.ANTI_ALIAS_FLAG).apply { style = Paint.Style.STROKE }
    private val loupeFrameFill = Paint()
    private val loupeBorder = Paint(Paint.ANTI_ALIAS_FLAG).apply { style = Paint.Style.STROKE }
    private val loupeClip = Path()

    /** The frame on the page (canvas in world coordinates). */
    private fun drawLoupeFrame(c: Canvas) {
        if (!loupeOpen) return
        val p = loupePage() ?: return
        val r = RectF(p.rect.left + loupeX, p.rect.top + loupeY, p.rect.left + loupeX + loupeW, p.rect.top + loupeY + loupeH())
        val accent = Ui.theme.accent.toInt()
        loupeFrameFill.color = accent; loupeFrameFill.alpha = 0x14
        c.drawRect(r, loupeFrameFill)
        loupeFramePaint.color = accent; loupeFramePaint.strokeWidth = 2f / scale
        c.drawRect(r, loupeFramePaint)
        // Where new lines begin: a short bar left of the line.
        val mx = p.rect.left + loupeLineX - 5f / scale
        loupeFramePaint.strokeWidth = 3f / scale; loupeFramePaint.alpha = 0x90
        c.drawLine(mx, r.top + r.height() * 0.2f, mx, r.bottom - r.height() * 0.1f, loupeFramePaint)
        loupeFramePaint.alpha = 0xFF
        // The corner: drag to write larger or smaller.
        c.drawCircle(r.right, r.bottom, 11f * density / scale, endRing)
        c.drawCircle(r.right, r.bottom, 6.5f * density / scale, handlePaint)
    }

    /** The frame's corner handle (screen) and the frame itself (screen). */
    private fun frameScreen(): RectF? {
        val p = loupePage() ?: return null
        return RectF((p.rect.left + loupeX) * scale + tx, (p.rect.top + loupeY) * scale + ty, (p.rect.left + loupeX + loupeW) * scale + tx, (p.rect.top + loupeY + loupeH()) * scale + ty)
    }

    /** The strip: the frame's part of the page, large, its "move on" part shaded. */
    private fun drawLoupe(c: Canvas, m: NotebookModel) {
        if (!loupeOpen) return
        // Its page went (folded, deleted, changed elsewhere): closed, not an invisible strip that
        // still takes the touches over a third of the screen.
        val p = loupePage() ?: run {
            post { if (loupeOpen && loupePage() == null && model?.placed?.isNotEmpty() == true) closeLoupe() }
            return
        }
        val s = loupeStrip()
        val radius = minOf(Ui.theme.style.panel, 18f) * density
        loupeClip.rewind()
        loupeClip.addRoundRect(s, radius, radius, Path.Direction.CW)
        c.save()
        c.clipPath(loupeClip)
        c.drawColor(bgColor)
        val ls = loupeScale(s)
        c.translate(loupeTx(p, s), loupeTy(p, s))
        c.scale(ls, ls)
        val vis = RectF(p.rect.left + loupeX, p.rect.top + loupeY, p.rect.left + loupeX + loupeW, p.rect.top + loupeY + loupeH(s))
        m.ensureLoaded(p.sheetId, p.rect.width())
        loupeDrawScale = ls
        drawPaper(c, p, vis)
        loupeDrawScale = 0f
        if (p.spec.pdfBlob != null) { val req = ArrayList<uniffi.n0te_ffi.FfiTileRequest>(); PdfTiles.draw(c, p, scale, vis, req, pdfSlot); PdfTiles.flush(req) }
        m.sheets[p.sheetId]?.let { drawImages(c, p, it); drawInk(c, p, it, vis, tiles = false) }
        if (action == Action.INK && mapLoupe && inkLayer?.active != true) {
            c.save(); c.translate(p.rect.left, p.rect.top); drawLive(c); c.restore()
        }
        c.restore()
        // Lifting the pen in here moves the frame on.
        val accent = Ui.theme.accent.toInt()
        val zone = RectF(s.right - s.width() * LOUPE_ADVANCE, s.top, s.right, s.bottom)
        val past = !loupeLastX.isNaN() && loupeLastX > loupeX + loupeW * 0.5f
        c.save(); c.clipPath(loupeClip)
        flowFill.color = accent; flowFill.alpha = if (past) 0x24 else 0x0E
        c.drawRect(zone, flowFill)
        // Where new lines begin, if it is in view.
        val mx = s.left + (loupeLineX - loupeX) * ls - 6f * density
        if (mx > s.left) {
            flowEdge.color = accent; flowEdge.alpha = 0x80; flowEdge.strokeWidth = 3f * density; flowEdge.pathEffect = null
            c.drawLine(mx, s.top + s.height() * 0.2f, mx, s.bottom - s.height() * 0.12f, flowEdge)
        }
        c.restore()
        flowEdge.color = accent; flowEdge.alpha = 0x60; flowEdge.strokeWidth = 1.5f * density
        flowEdge.pathEffect = DashPathEffect(floatArrayOf(6f * density, 5f * density), 0f)
        c.drawLine(zone.left, zone.top + 10f * density, zone.left, zone.bottom - 10f * density, flowEdge)
        val atEnd = loupeX + loupeW >= p.rect.width() * 0.97f - 1f
        flowGlyph.color = accent; flowGlyph.alpha = 0xB0; flowGlyph.textSize = 24f * density
        c.drawText(if (atEnd) "↵" else "→", zone.centerX(), zone.top + 40f * density, flowGlyph)
        loupeBorder.color = Ui.theme.border.toInt(); loupeBorder.strokeWidth = 1f * density
        c.drawRoundRect(s, radius, radius, loupeBorder)
        // The grip on the top edge (drag: the strip goes up or down) and the height handle.
        val g = padGrip()
        val cap = RectF(g.centerX() - 26f * density, g.centerY() - 9f * density, g.centerX() + 26f * density, g.centerY() + 9f * density)
        loupeFrameFill.color = Ui.theme.surface.toInt(); loupeFrameFill.alpha = 0xFF
        c.drawRoundRect(cap, 9f * density, 9f * density, loupeFrameFill)
        c.drawRoundRect(cap, 9f * density, 9f * density, loupeBorder)
        flowEdge.color = Ui.theme.muted.toInt(); flowEdge.alpha = 0xFF; flowEdge.strokeWidth = 2f * density; flowEdge.pathEffect = null
        for (dy in floatArrayOf(-3f, 3f)) c.drawLine(cap.centerX() - 12f * density, cap.centerY() + dy * density, cap.centerX() + 12f * density, cap.centerY() + dy * density, flowEdge)
        val z = padSizer()
        val r = 13f * density
        c.drawCircle(z.centerX(), z.centerY(), r, loupeFrameFill)
        c.drawCircle(z.centerX(), z.centerY(), r, loupeBorder)
        val ax = z.centerX(); val ay = z.centerY(); val a = 6.5f * density; val h2 = 3.5f * density
        c.drawLine(ax, ay - a, ax, ay + a, flowEdge)
        c.drawLine(ax - h2, ay - a + h2, ax, ay - a, flowEdge); c.drawLine(ax + h2, ay - a + h2, ax, ay - a, flowEdge)
        c.drawLine(ax - h2, ay + a - h2, ax, ay + a, flowEdge); c.drawLine(ax + h2, ay + a - h2, ax, ay + a, flowEdge)
        // Scroll bars: where the frame is on the page (stronger while held).
        val th = barTrackH(s); val tv = barTrackV(s)
        val held = loupeGrab == Grab.BAR_H || loupeGrab == Grab.BAR_V
        loupeFrameFill.color = Ui.theme.border.toInt(); loupeFrameFill.alpha = 0xB0
        c.drawRoundRect(th, 2f * density, 2f * density, loupeFrameFill)
        c.drawRoundRect(tv, 2f * density, 2f * density, loupeFrameFill)
        loupeFrameFill.color = accent; loupeFrameFill.alpha = if (held) 0xFF else 0xB0
        val pw = p.rect.width(); val ph = p.rect.height()
        val hx0 = th.left + th.width() * (loupeX / pw); val hx1 = th.left + th.width() * ((loupeX + loupeW) / pw)
        c.drawRoundRect(RectF(hx0, th.top - 1.5f * density, max(hx1, hx0 + 8f * density), th.bottom + 1.5f * density), 3.5f * density, 3.5f * density, loupeFrameFill)
        val vy0 = tv.top + tv.height() * (loupeY / ph); val vy1 = tv.top + tv.height() * ((loupeY + loupeH(s)) / ph)
        c.drawRoundRect(RectF(tv.left - 1.5f * density, vy0, tv.right + 1.5f * density, max(vy1, vy0 + 8f * density)), 3.5f * density, 3.5f * density, loupeFrameFill)
    }

    /** The fountain pen's character on the running stroke (Fountain.kt). */
    private var fountainOn = false
    private var fountainState = Fountain.State()
    private var fountainOpts = Fountain.Options(0.65f, 0.5f, "round", true)

    /** A pen sample as written: the fountain pen shapes its pressure (speed, nib, taper). */
    private fun appendInk(x: Float, y: Float, p: Float, t: Long) =
        append(x, y, if (fountainOn) Fountain.pressure(fountainState, fountainOpts, x, y, p, (t - strokeStart).toFloat()) else p, t)

    private fun append(x: Float, y: Float, p: Float, t: Long) {
        if ((count + 1) * 4 > samples.size) samples = samples.copyOf(samples.size * 2)
        samples[count * 4] = x
        samples[count * 4 + 1] = y
        samples[count * 4 + 2] = p
        samples[count * 4 + 3] = (t - strokeStart).toFloat()
        count++
    }

    private fun updateLive() {
        val p = target ?: return
        val b = brush
        val bytes = bytesOf(samples, count * 4)
        liveOutline = floatsOf(strokeOutline(b, bytes))
        val pencil = b.kind == KIND_PENCIL.toUByte()
        liveLayers = if (pencil) strokeLayers(b, bytes).map { floatsOf(it) } else emptyList()
        liveLayerPaths = liveLayers.map { InkStroke.pathOf(it) }
        livePath.rewind()
        val n = liveOutline.size / 2
        if (n > 0) {
            livePath.moveTo(liveOutline[0], liveOutline[1])
            for (i in 1 until n) livePath.lineTo(liveOutline[2 * i], liveOutline[2 * i + 1])
            livePath.close()
        }
        sendLiveSamples(false)
        val ov = inkLayer
        if (ov != null && ov.active) {
            val k = aScale; val ox = aTx; val oy = aTy
            val mtx = Matrix().apply { setScale(k, k); postTranslate(ox + p.rect.left * k, oy + p.rect.top * k) }
            val paths = (listOf(livePath) + liveLayerPaths).map { Path(it).apply { transform(mtx) } }
            ov.draw(paths, liveColor, if (pencil) Pencil.texelFor(k) to mtx else null)
        } else {
            invalidate()
        }
    }

    private fun commitInk(m: NotebookModel, p: Placed, now: Long) {
        val tap = now - strokeStart < TAP_MS && inkExtent(count) * aScale < 6f * density
        // A tap in the magnifier's move-on part moves the frame on (no dot).
        if (mapLoupe && tap && samples[0] > loupeX + loupeW * (1f - LOUPE_ADVANCE)) {
            cancelInk()
            loupeMoveOn()
            return
        }
        // A tap in the Mitwandern band moves on right away (no dot).
        if (!mapLoupe && tap && board == null && Settings.writeFlow && tool == Tool.PEN) {
            val fp = flowPage()
            if (fp != null && fp.sheetId == p.sheetId && flowBand(fp).contains((p.rect.left + samples[0]) * scale + tx, (p.rect.top + samples[1]) * scale + ty)) {
                flowSheet = p.sheetId; flowLiftX = p.rect.left + samples[0]
                if (flowTop.isNaN()) { flowTop = p.rect.top + samples[1] - 6f; flowBottom = p.rect.top + samples[1] + 6f }
                cancelInk()
                writeFlow()
                return
            }
        }
        // With the magnifier open, a pen tap on the page above puts its frame there (no dot).
        if (loupeOpen && !mapLoupe && board == null && tap) {
            moveLoupeTo(p, samples[0], samples[1])
            cancelInk()
            return
        }
        sendLiveSamples(true)
        var n = count
        // A quick zigzag over ink deletes what it crosses (and is not kept itself).
        if (Settings.scribbleErase && tool != Tool.MARKER && !splitStroke && snapEdge == 0 && isScribble(samples, n)) {
            val ids = strokesUnder(m, p, samples, n, brush.width / 2f + 1.5f)
            if (ids.isNotEmpty()) {
                m.deleteStrokes(p.sheetId, ids)
                listener?.onMessage(if (ids.size == 1) "1 Strich weggekritzelt" else "${ids.size} Striche weggekritzelt")
                cancelInk()
                return
            }
        }
        removeCallbacks(holdCheck); removeCallbacks(arrowCheck)
        /** The stroke becomes [shape] (a head, a shape, an arrow). */
        fun become(shape: FloatArray) {
            shaped = true
            samples = if (shape.size > samples.size) shape.copyOf(shape.size * 2) else samples
            System.arraycopy(shape, 0, samples, 0, shape.size)
            n = shape.size / 4
            count = n
            liveOutline = floatsOf(strokeOutline(brush, bytesOf(samples, n * 4)))
            liveLayers = if (brush.kind == KIND_PENCIL.toUByte()) strokeLayers(brush, bytesOf(samples, n * 4)).map { floatsOf(it) } else emptyList()
        }
        // Held still at the end (only then: writing never turns into shapes): an arrowhead at
        // a line's end, or a clean line, triangle, rectangle, ellipse, arrow (not along the ruler).
        val held = Settings.shapes && !snapped && !splitStroke && snapEdge == 0 && n > 3 && now - stillSince > 600
        val heldLong = held && Settings.arrows && now - stillSince > ARROW_HOLD_MS
        if (heldLong && tool == Tool.PEN && n <= 600) {
            arrowHead(m, p, n)?.let { become(it) } ?: barbPair(m, p, n)?.let { (head, first) -> m.deleteStrokes(p.sheetId, listOf(first)); become(head) }
        } else if (held && Settings.arrows && tool == Tool.PEN && n <= 600) {
            // A triangle at a line's end: a head, not a triangle turned the way it was drawn.
            arrowHead(m, p, n, triangles = true)?.let { become(it) }
        }
        if (held && !shaped && n > 4) Shapes.recognize(samples, n, still = 6f / aScale, arrows = heldLong)?.let { become(it) }
        // The fountain pen lifts off thin (not for a snapped shape).
        if (fountainOn && !snapped && !shaped) {
            Fountain.taperEnd(samples, n, fountainOpts)
            liveOutline = floatsOf(strokeOutline(brush, bytesOf(samples, n * 4)))
        }
        shaped = false
        m.addStroke(p.sheetId, brush, bytesOf(samples, n * 4), liveOutline, liveLayers)
        if (mapLoupe) loupeAfterStroke(p, n) else if (Settings.writeFlow && board == null && tool == Tool.PEN) trackFlow(p, n)
        count = 0
        action = Action.NONE
        actionId = -1
        invalidate()
        // Clear the low-latency layer once the page itself shows the stroke.
        postOnAnimation { postOnAnimation { inkLayer?.clear() } }
    }

    /**
     * The stroke just drawn ([n] samples) as the head of an arrow: a "V" (or a small triangle)
     * with its point at the end of a line on [p] (straight or curved), opening back along it.
     * A closed triangle at a line's end is a head however it was turned (it then points the way
     * the line does). [triangles]: only such triangles (they need no long hold). Null if no head.
     */
    private fun arrowHead(m: NotebookModel, p: Placed, n: Int, triangles: Boolean = false): FloatArray? {
        val st = m.sheets[p.sheetId] ?: return null
        val r = Shapes.trimmed(samples, n, 6f / aScale) ?: return null
        var x0 = Float.MAX_VALUE; var y0 = Float.MAX_VALUE; var x1 = -Float.MAX_VALUE; var y1 = -Float.MAX_VALUE
        for (i in r) { x0 = min(x0, samples[i * 4]); x1 = max(x1, samples[i * 4]); y0 = min(y0, samples[i * 4 + 1]); y1 = max(y1, samples[i * 4 + 1]) }
        if (max(x1 - x0, y1 - y0) > 120f) return null // a head, not a drawing
        val reach = max(8f, 0.5f * max(x1 - x0, y1 - y0))
        val area = RectF(x0 - reach, y0 - reach, x1 + reach, y1 + reach)
        // The nearest line end first (several may be close).
        val cands = ArrayList<Pair<Float, () -> FloatArray?>>()
        for (s in st.strokes) {
            val e = s.ends
            if (e.size < 9 || s.id in st.hidden || !RectF.intersects(s.bounds, area) || e[8] < 30f) continue
            for (k in 0..1) {
                val o = k * 4
                if (!area.contains(e[o], e[o + 1])) continue
                cands += hypot(e[o] - (x0 + x1) / 2, e[o + 1] - (y0 + y1) / 2) to {
                    Arrows.closedHead(samples, r.first, r.last, e[o], e[o + 1], e[o + 2], e[o + 3], e[8])
                        ?: if (triangles) null else Arrows.head(samples, r.first, r.last, e[o], e[o + 1], e[o + 2], e[o + 3], e[8])
                }
            }
        }
        for ((_, f) in cands.sortedBy { it.first }) f()?.let { return it }
        return null
    }

    /**
     * The stroke just drawn ([n] samples) as the second barb of a head drawn in two strokes: a
     * short straight stroke at a line's end on [p], with an earlier one at the same end on the
     * other side of the line. The head and the earlier barb's id (it goes), or null.
     */
    private fun barbPair(m: NotebookModel, p: Placed, n: Int): Pair<FloatArray, String>? {
        val st = m.sheets[p.sheetId] ?: return null
        val r = Shapes.trimmed(samples, n, 6f / aScale) ?: return null
        var x0 = Float.MAX_VALUE; var y0 = Float.MAX_VALUE; var x1 = -Float.MAX_VALUE; var y1 = -Float.MAX_VALUE
        for (i in r) { x0 = min(x0, samples[i * 4]); x1 = max(x1, samples[i * 4]); y0 = min(y0, samples[i * 4 + 1]); y1 = max(y1, samples[i * 4 + 1]) }
        if (max(x1 - x0, y1 - y0) > 80f) return null
        val reach = max(10f, max(x1 - x0, y1 - y0))
        val area = RectF(x0 - reach, y0 - reach, x1 + reach, y1 + reach)
        val near = st.strokes.filter { it.ends.size >= 9 && it.id !in st.hidden && RectF.intersects(it.bounds, area) }
        for (line in near) {
            val e = line.ends
            if (e[8] < 30f) continue
            for (k in 0..1) {
                val o = k * 4
                val tx = e[o]; val ty = e[o + 1]; val bx = e[o + 2]; val by = e[o + 3]
                if (!area.contains(tx, ty)) continue
                val b2 = Arrows.barb(samples, r.first, r.last, tx, ty, bx, by, e[8]) ?: continue
                // A line, not another barb: well longer than the barbs.
                if (e[8] < max(25f, 2.5f * hypot(b2[0], b2[1]))) continue
                for (other in near) {
                    if (other.id == line.id) continue
                    val f = other.ends
                    val b1 = Arrows.barbFrom(f[0], f[1], f[4], f[5], f[8], tx, ty, bx, by, e[8]) ?: continue
                    val head = Arrows.headFromBarbs(floatArrayOf(b1[0], b1[1], b2[2]), b2, tx, ty, bx, by, e[8], samples[r.first * 4 + 3], samples[r.last * 4 + 3]) ?: continue
                    return head to other.id
                }
            }
        }
        return null
    }

    /** How far the first [n] samples spread (page points). */
    private fun inkExtent(n: Int): Float {
        if (n <= 0) return 0f
        var l = samples[0]; var r = l; var t = samples[1]; var b = t
        for (i in 1 until n) { val x = samples[i * 4]; val y = samples[i * 4 + 1]; l = min(l, x); r = max(r, x); t = min(t, y); b = max(b, y) }
        return max(r - l, b - t)
    }

    /**
     * The held line's end at (lx, ly): straight from its start, snapping to 45° steps (within 6°)
     * and 15° steps (within 3°), with a tick at each new step.
     */
    private fun moveHeldLine(lx: Float, ly: Float, t: Long, batch: Boolean) {
        var dx = lx - lineAX; var dy = ly - lineAY
        wiggle((dx * lineNx + dy * lineNy) * aScale, t)
        val len = hypot(dx, dy)
        if (len < 3f / aScale) return
        val deg = Math.toDegrees(kotlin.math.atan2(dy, dx).toDouble())
        val snap = if (lineFree) null else LineGrip.snap(deg)
        if (snap != null) { dx = (len * kotlin.math.cos(Math.toRadians(snap))).toFloat(); dy = (len * kotlin.math.sin(Math.toRadians(snap))).toFloat() }
        val step = snap?.let { Math.round(it).toInt() }
        if (step != lineSnapDeg) {
            if (step != null) performHapticFeedback(android.view.HapticFeedbackConstants.CLOCK_TICK)
            lineSnapDeg = step
        }
        val line = Shapes.line(lineAX, lineAY, lineAX + dx, lineAY + dy, linePressure, 0f, (t - strokeStart).toFloat())
        samples = if (line.size > samples.size) line.copyOf(line.size * 2) else samples
        System.arraycopy(line, 0, samples, 0, line.size)
        count = line.size / 4
        if (!batch) updateLive()
    }

    /**
     * The pen at (lx, ly) while a snapped shape is held: the corner nearest where it snapped moves
     * with the pen (after a little play, so the pen shifting as it lifts changes nothing), with a
     * tick when a rectangle or ellipse becomes a square or circle.
     */
    private fun moveHeldShape(h: Shapes.HeldShape, lx: Float, ly: Float, t: Long, batch: Boolean) {
        val dx = lx - shapeHeldX; val dy = ly - shapeHeldY
        if (!shapeDragging) { if (hypot(dx, dy) * aScale < 6f) return; shapeDragging = true }
        val out = h.drag(dx, dy, (t - strokeStart).toFloat())
        if (h.square != shapeSquare) {
            if (h.square) performHapticFeedback(android.view.HapticFeedbackConstants.CLOCK_TICK)
            shapeSquare = h.square
        }
        samples = if (out.size > samples.size) out.copyOf(out.size * 2) else samples
        System.arraycopy(out, 0, samples, 0, out.size)
        count = out.size / 4
        if (!batch) updateLive()
    }

    /**
     * A quick wiggle across a held line (three turns back and forth of 6 dp or more within
     * 0.8 s) switches its angle snapping off, another one back on. [p]: the pen across the line
     * (screen px), [t]: its time.
     */
    private fun wiggle(p: Float, t: Long) {
        if (t < wiggleQuietUntil) return
        val amp = 6f * density
        when {
            wiggleDir == 0 -> { wiggleHi = p; wiggleLo = p; wiggleDir = 2 }
            wiggleDir == 2 -> {
                wiggleHi = max(wiggleHi, p); wiggleLo = min(wiggleLo, p)
                if (wiggleHi - p > amp) { wiggleDir = -1; wiggleLo = p } else if (p - wiggleLo > amp) { wiggleDir = 1; wiggleHi = p }
            }
            wiggleDir == 1 -> if (p > wiggleHi) wiggleHi = p else if (wiggleHi - p > amp) { wiggleDir = -1; wiggleLo = p; wiggleTurns.addLast(t) }
            else -> if (p < wiggleLo) wiggleLo = p else if (p - wiggleLo > amp) { wiggleDir = 1; wiggleHi = p; wiggleTurns.addLast(t) }
        }
        while (wiggleTurns.isNotEmpty() && t - wiggleTurns.first() > 800) wiggleTurns.removeFirst()
        if (wiggleTurns.size >= 3) {
            lineFree = !lineFree
            wiggleTurns.clear(); wiggleDir = 0
            wiggleQuietUntil = t + 600
            lineSnapDeg = null
            performHapticFeedback(android.view.HapticFeedbackConstants.LONG_PRESS)
            listener?.onMessage(if (lineFree) "Linie frei: rastet nicht mehr ein" else "Linie rastet wieder ein")
        }
    }

    /** The pen rests mid-stroke: a recognisable shape replaces the stroke right away (with a tick). */
    /**
     * The pen rests: after half a second a shape ([arrows] false), after [ARROW_HOLD_MS] an arrow
     * ([arrows] true: a head at a line's end — a V, a triangle, the second of two barbs — or an
     * arrow drawn in one stroke). A head in the making is left alone by the shapes.
     */
    private fun trySnap(arrows: Boolean) {
        if (action != Action.INK || snapped || !Settings.shapes || splitStroke || snapEdge != 0 || count < 5) return
        val m0 = model; val p0 = target
        val heads = Settings.arrows && tool == Tool.PEN && m0 != null && p0 != null && count <= 600
        val snap: Shapes.Shape = if (!arrows) {
            // A triangle at a line's end becomes its head at once; a V waits for the longer hold.
            val tri = if (heads) arrowHead(m0!!, p0!!, count, triangles = true) else null
            if (tri != null) Shapes.Shape("arrow", tri)
            else {
                if (heads && (arrowHead(m0!!, p0!!, count) != null || barbPair(m0, p0, count) != null)) return
                Shapes.detect(samples, count, still = 6f / aScale, arrows = false) ?: return
            }
        } else {
            if (!Settings.arrows) return
            val head = if (heads) arrowHead(m0!!, p0!!, count) ?: barbPair(m0, p0, count)?.let { (h, first) -> m0.deleteStrokes(p0.sheetId, listOf(first)); h } else null
            head?.let { Shapes.Shape("arrow", it) } ?: Shapes.trimmed(samples, count, 6f / aScale)?.let { r -> Arrows.fromStroke(samples, r.first, r.last) }?.let { Shapes.Shape("arrow", it) } ?: return
        }
        val kind = snap.kind; val shape = snap.samples
        val penX = samples[(count - 1) * 4]; val penY = samples[(count - 1) * 4 + 1]
        // The laptop: the raw stroke ends here, the shape follows as a new one.
        sendLiveSamples(true)
        liveSent = 0; liveSeq = 0; liveStrokeId = System.nanoTime()
        samples = if (shape.size > samples.size) shape.copyOf(shape.size * 2) else samples
        System.arraycopy(shape, 0, samples, 0, shape.size)
        count = shape.size / 4
        snapped = true
        if (kind == "line") {
            // Still holding: moving the pen now changes its length and angle (it stays straight).
            lineHeld = true; liveHold = true
            lineAX = samples[0]; lineAY = samples[1]; linePressure = samples[2]
            // Across the line as snapped: where a wiggle is measured.
            val ex = samples[(count - 1) * 4] - lineAX; val ey = samples[(count - 1) * 4 + 1] - lineAY
            val l = hypot(ex, ey).coerceAtLeast(1e-3f)
            lineNx = -ey / l; lineNy = ex / l
            wiggleDir = 0; wiggleTurns.clear()
        } else if (snap.corners != null || snap.ellipse != null) {
            // Still holding a closed shape: dragging now changes its size (the corner by the pen follows).
            shapeHeld = Shapes.HeldShape(snap, penX, penY); liveHold = true
            shapeHeldX = penX; shapeHeldY = penY; shapeDragging = false; shapeSquare = false
        }
        performHapticFeedback(android.view.HapticFeedbackConstants.CLOCK_TICK)
        updateLive()
    }

    /** Ends the running stroke on [from] and continues it on [to] (the pen crossed pages). */
    private fun splitInk(from: Placed, to: Placed, t: Long) {
        val m = model ?: return
        sendLiveSamples(true)
        liveSent = 0; liveSeq = 0; liveStrokeId = System.nanoTime()
        val b = brush
        val bytes = bytesOf(samples, count * 4)
        val outline = floatsOf(strokeOutline(b, bytes))
        val layers = if (b.kind == KIND_PENCIL.toUByte()) strokeLayers(b, bytes).map { floatsOf(it) } else emptyList()
        m.addStroke(from.sheetId, b, bytes, outline, layers)
        // The new part starts where the old one ended, so the line looks continuous.
        val lx = samples[(count - 1) * 4] + from.rect.left - to.rect.left
        val ly = samples[(count - 1) * 4 + 1] + from.rect.top - to.rect.top
        val pr = samples[(count - 1) * 4 + 2]
        target = to
        count = 0
        splitStroke = true
        append(lx, ly, pr, t)
        stillX = lx; stillY = ly
    }

    /** Strokes on [p]'s sheet that the path (n samples) passes within [r] of. */
    private fun strokesUnder(m: NotebookModel, p: Placed, pts: FloatArray, n: Int, r: Float): List<String> {
        val st = m.sheets[p.sheetId] ?: return emptyList()
        var x0 = Float.MAX_VALUE; var y0 = Float.MAX_VALUE; var x1 = -Float.MAX_VALUE; var y1 = -Float.MAX_VALUE
        for (i in 0 until n) { x0 = min(x0, pts[i * 4]); y0 = min(y0, pts[i * 4 + 1]); x1 = max(x1, pts[i * 4]); y1 = max(y1, pts[i * 4 + 1]) }
        return st.strokes.filter { s ->
            s.bounds.right >= x0 - r && s.bounds.left <= x1 + r && s.bounds.bottom >= y0 - r && s.bounds.top <= y1 + r &&
                (0 until n step 2).any { i -> polygonNear(s.outline, pts[i * 4], pts[i * 4 + 1], r) }
        }.map { it.id }
    }

    private fun cancelInk() {
        removeCallbacks(holdCheck); removeCallbacks(arrowCheck)
        mapLoupe = false
        count = 0
        livePath.rewind()
        action = Action.NONE
        actionId = -1
        inkLayer?.clear()
        invalidate()
    }

    // --- eraser --------------------------------------------------------------------------------

    /** Eraser radius in sheet points when not moving: the set size, or the pen's (automatic). */
    private fun eraserBaseRadius(): Float =
        if (Settings.eraserAuto) (Tools.widths[widthIdx] * Tools.pen.widthMul / 2f + 0.8f).coerceAtLeast(1f) else Settings.eraserSize / 2f

    private fun beginErase(p: Placed, lx: Float, ly: Float, now: Long) {
        eraseR = eraserBaseRadius()
        lastEraseX = lx; lastEraseY = ly; lastEraseT = now
        erasePath.clear()
        eraseSheet = p.sheetId
        if (Settings.eraserArea) { eraseClip = Path(); addEraseDab(lx, ly, eraseR) } else { eraseClip = null; eraseAt(p, lx, ly, eraseR) }
    }

    private fun eraseMove(p: Placed, lx: Float, ly: Float, t: Long) {
        val d = hypot(lx - lastEraseX, ly - lastEraseY)
        if (Settings.eraserAuto) {
            // Wider the faster it moves (pt per ms), smoothed so it doesn't jump.
            val speed = d / (t - lastEraseT).coerceAtLeast(1L)
            val base = eraserBaseRadius()
            val target = (base * (1f + speed * 9f)).coerceAtMost(max(base * 8f, 28f))
            eraseR = eraseR * 0.75f + target * 0.25f
        } else eraseR = eraserBaseRadius()
        // Fill the gap between samples so a fast swipe misses nothing.
        val steps = (d / max(eraseR * 0.5f, 0.5f)).toInt().coerceIn(1, 64)
        for (i in 1..steps) {
            val f = i / steps.toFloat()
            val x = lastEraseX + (lx - lastEraseX) * f; val y = lastEraseY + (ly - lastEraseY) * f
            if (eraseClip != null) addEraseDab(x, y, eraseR) else eraseAt(p, x, y, eraseR)
        }
        lastEraseX = lx; lastEraseY = ly; lastEraseT = t
    }

    private fun addEraseDab(x: Float, y: Float, r: Float) {
        erasePath += x; erasePath += y; erasePath += r
        eraseClip?.addCircle(x, y, r, Path.Direction.CW)
        eraseSheet?.let { queueErase(it, null, floatArrayOf(x, y, r)) }
        invalidate()
    }

    /** Live: what the eraser just did goes to the laptop (batched; [force] at the end). */
    private fun queueErase(sheetId: String, id: String?, dab: FloatArray?, force: Boolean = false) {
        val sink = live ?: return
        if (eraseSendSheet != null && eraseSendSheet != sheetId) flushErase(sink)
        eraseSendSheet = sheetId
        id?.let { eraseIds += it }
        dab?.forEach { eraseDabs += it }
        val now = SystemClock.uptimeMillis()
        if (force || now - eraseSentAt >= 50) flushErase(sink)
    }

    private fun flushErase(sink: LiveSink) {
        val sheet = eraseSendSheet ?: return
        if (eraseIds.isNotEmpty() || eraseDabs.isNotEmpty()) sink.erase(sheet, eraseIds.toList(), eraseDabs.toFloatArray())
        eraseIds.clear(); eraseDabs.clear()
        eraseSentAt = SystemClock.uptimeMillis()
    }

    /** Someone else erasing (a guest): their hidden strokes vanish here at once. */
    fun remoteErase(sheetId: String, ids: List<String>) {
        val st = model?.sheets?.get(sheetId) ?: return
        if (ids.isEmpty()) return
        st.hidden.addAll(ids)
        st.invalidateNode()
        invalidate()
    }

    private fun endErase(m: NotebookModel, p: Placed) {
        live?.let { flushErase(it) }
        if (eraseClip != null) {
            val path = erasePath.toFloatArray()
            erasePath.clear()
            // Keep hiding the erased area until the cut strokes are back — unless the next swipe
            // began meanwhile (the core busy with a dense page): its clip stays, else the rest of
            // it erased whole strokes and its first dabs were never applied.
            val mine = eraseClip
            m.eraseArea(p.sheetId, path) { if (eraseClip === mine) { eraseClip = null; eraseSheet = null }; invalidate() }
        } else {
            m.commitErase(p.sheetId)
            eraseSheet = null
        }
    }

    /** An eraser gesture taken back: nothing it covered is erased. */
    private fun cancelErase(m: NotebookModel, p: Placed) {
        if (eraseClip != null) { eraseClip = null; erasePath.clear() }
        m.sheets[p.sheetId]?.let { st -> if (st.hidden.isNotEmpty()) { st.hidden.clear(); st.invalidateNode() } }
        eraseSheet = null
        invalidate()
    }

    private fun eraseAt(p: Placed, lx: Float, ly: Float, r: Float) {
        val st = model?.sheets?.get(p.sheetId) ?: return
        var any = false
        for (s in st.strokes) {
            if (s.id in st.hidden) continue
            if (lx < s.bounds.left - r || lx > s.bounds.right + r || ly < s.bounds.top - r || ly > s.bounds.bottom + r) continue
            if (polygonNear(s.outline, lx, ly, r)) { st.hidden += s.id; any = true; queueErase(p.sheetId, s.id, null) }
        }
        if (any) { st.invalidateNode(); invalidate() }
    }

    // --- lasso ---------------------------------------------------------------------------------

    /** The lasso (world coordinates) let go: what it caught on every page it crossed. */
    private fun finishLasso(m: NotebookModel) {
        val poly = lasso.toFloatArray()
        lasso.clear()
        fun local(p: Placed) = FloatArray(poly.size) { i -> poly[i] - if (i % 2 == 0) p.rect.left else p.rect.top }
        val pages = board?.let { listOf(it) } ?: m.placed
        // A tap: the image under the pen.
        if (poly.size < 6 || polyExtent(poly) < 4f) {
            val wx = poly.getOrNull(0) ?: return; val wy = poly.getOrNull(1) ?: return
            // A board has no edge: a tap anywhere on it counts (its frame cut off what lay outside).
            val p = board ?: pages.firstOrNull { it.rect.contains(wx, wy) } ?: return
            val st = m.sheets[p.sheetId] ?: return
            val tx0 = wx - p.rect.left; val ty0 = wy - p.rect.top
            // The stroke under it first (a shape, a line, a label written on a graph: ink lies on
            // top): tapped, it is selected with its grips.
            val r = 10f * density / scale
            st.strokes.lastOrNull { it.id !in st.hidden && RectF(it.bounds).apply { inset(-r, -r) }.contains(tx0, ty0) && polygonNear(it.outline, tx0, ty0, r) }?.let { s ->
                selectStrokes(p.sheetId, listOf(s))
                return
            }
            // Else the image or coordinate system there.
            st.images.lastOrNull { tx0 >= it.x && tx0 <= it.x + it.w && ty0 >= it.y && ty0 <= it.y + it.h }?.let { im ->
                selection = Selection(p.sheetId, setOf(im.id), RectF(im.x, im.y, im.x + im.w, im.y + im.h))
                selDx = 0f; selDy = 0f; selScale = 1f
                listener?.onSelectionChanged(selection)
            }
            return
        }
        val area = RectF(poly[0], poly[1], poly[0], poly[1])
        for (i in poly.indices step 2) area.union(poly[i], poly[i + 1])
        if (Settings.selectMode == "snip") {
            // A screenshot comes from one page: the one the box covers most.
            val p = pages.filter { RectF.intersects(it.rect, area) }.maxByOrNull { val r = RectF(it.rect); if (r.intersect(area)) r.width() * r.height() else 0f } ?: return
            val st = m.sheets[p.sheetId] ?: return
            val r = RectF(area).apply { offset(-p.rect.left, -p.rect.top) }
            r.intersect(0f, 0f, p.rect.width(), p.rect.height())
            if (r.width() > 4f && r.height() > 4f) snip(m, p, st, r)
            return
        }
        val parts = ArrayList<Selection>()
        for (p in pages) {
            if (board == null && !RectF.intersects(p.rect, area)) continue
            val st = m.sheets[p.sheetId] ?: continue
            val lp = local(p)
            val ids = HashSet<String>()
            val b = RectF()
            // Images whose middle lies inside; a coordinate system mostly inside (its middle and
            // three corners), so circling a label written on it takes the label, not the graph.
            for (im in st.images) {
                val corners = if (im.plot == null) 4 else listOf(im.x to im.y, im.x + im.w to im.y, im.x to im.y + im.h, im.x + im.w to im.y + im.h).count { (cx, cy) -> pointInPolygon(cx, cy, lp) }
                if (pointInPolygon(im.x + im.w / 2, im.y + im.h / 2, lp) && corners >= 3) {
                    ids += im.id
                    val r = RectF(im.x, im.y, im.x + im.w, im.y + im.h)
                    if (b.isEmpty) b.set(r) else b.union(r)
                }
            }
            // A page full of handwriting is thousands of outline edges: tested on the lasso's grid,
            // strokes beside its box not at all.
            val mask = LassoMask(lp)
            val box = RectF(area).apply { offset(-p.rect.left, -p.rect.top) }
            for (s in st.strokes) {
                val n = s.outline.size / 2
                if (n == 0 || !RectF.intersects(s.bounds, box)) continue
                // Half of it inside, measured along its length (not by its points: a line drawn
                // as a shape has most of them packed at its round ends).
                var inLen = 0f; var allLen = 0f
                for (i in 0 until n) {
                    val j = (i + 1) % n
                    val x0 = s.outline[2 * i]; val y0 = s.outline[2 * i + 1]; val x1 = s.outline[2 * j]; val y1 = s.outline[2 * j + 1]
                    val l = hypot(x1 - x0, y1 - y0)
                    allLen += l
                    if (mask.contains((x0 + x1) / 2, (y0 + y1) / 2)) inLen += l
                }
                val inside = if (allLen > 0.5f) inLen >= allLen * 0.5f else mask.contains(s.outline[0], s.outline[1])
                if (inside) { ids += s.id; if (b.isEmpty) b.set(s.bounds) else b.union(s.bounds) }
            }
            if (ids.isNotEmpty()) parts += Selection(p.sheetId, ids, b)
        }
        if (parts.isEmpty()) return
        selection = Selection(parts[0].sheetId, parts[0].ids, parts[0].bounds, parts.drop(1))
        selDx = 0f; selDy = 0f; selScale = 1f
        listener?.onSelectionChanged(selection)
    }

    private fun hitSelection(x: Float, y: Float): Action {
        val sel = selection ?: return Action.NONE
        val p = placedFor(sel.sheetId) ?: return Action.NONE
        if (!sel.multi) selEnds?.let { e ->
            for (i in 0..1) {
                val sx = (p.rect.left + e[2 * i]) * scale + tx; val sy = (p.rect.top + e[2 * i + 1]) * scale + ty
                if (hypot(x - sx, y - sy) < 22f * density) { endWhich = i; endX = e[2 * i]; endY = e[2 * i + 1]; endTurning = false; endSnapDeg = null; return Action.END_SEL }
            }
        }
        val b = selectionRectWorld(sel)
        // On what is selected: it moves (however small it is, the grips sit outside it).
        val inside = selUnion(sel).apply { offset(selDx, selDy) }
        if (inside.contains(toWorldX(x), toWorldY(y))) return Action.MOVE_SEL
        if (!sel.multi && selEnds == null) {
            // One page: the grips around the box (and the turning one above it).
            val grips = grips(sel, p)
            val near = grips.indices.filter { grips[it] != null }.minByOrNull { val g = grips[it]!!; hypot(x - (g.x * scale + tx), y - (g.y * scale + ty)) }
            if (near != null) {
                val g = grips[near]!!
                if (hypot(x - (g.x * scale + tx), y - (g.y * scale + ty)) < 22f * density) { startGrip(sel, p, near); return Action.GRIP_SEL }
            }
        }
        val sx = b.right * scale + tx; val sy = b.bottom * scale + ty
        if (sel.multi && hypot(x - sx, y - sy) < HANDLE_PX * 2.5f) return Action.SCALE_SEL
        val wx = toWorldX(x); val wy = toWorldY(y)
        return if (b.contains(wx, wy)) Action.MOVE_SEL else Action.NONE
    }

    private fun startSelDrag() {
        val sel = selection ?: return
        selDx = 0f; selDy = 0f; selScale = 1f
        for (part in sel.parts) model?.state(part.sheetId)?.let { it.floating = part.ids; it.invalidateNode() }
    }

    /** A line's end let go: turn and stretch the stroke about its other end (width kept). */
    private fun finishEndDrag() {
        val sel = selection ?: return
        val m = model ?: return
        val e = selEnds ?: return
        val ax = if (endWhich == 0) e[2] else e[0]; val ay = if (endWhich == 0) e[3] else e[1]
        val ox = (if (endWhich == 0) e[0] else e[2]) - ax; val oy = (if (endWhich == 0) e[1] else e[3]) - ay
        val nx = endX - ax; val ny = endY - ay
        val d = ox * ox + oy * oy
        val done = { m.state(sel.sheetId).let { it.floating = emptySet(); it.invalidateNode() }; invalidate() }
        if (d < 0.01f || hypot(nx - ox, ny - oy) < 0.5f || hypot(nx, ny) < 2f) { done(); return }
        val re = (nx * ox + ny * oy) / d
        val im = (ny * ox - nx * oy) / d
        m.spin(sel.sheetId, sel.ids, ax, ay, re, im) {
            m.state(sel.sheetId).floating = emptySet()
            // The selection follows the stroke's new shape (its bounds, its ends).
            val nb = m.sheets[sel.sheetId]?.strokes?.firstOrNull { it.id in sel.ids }?.bounds?.let { RectF(it) } ?: sel.bounds
            if (selection === sel) {
                selection = Selection(sel.sheetId, sel.ids, nb)
                listener?.onSelectionChanged(selection)
            }
            invalidate()
        }
    }

    private fun finishSelDrag() {
        val sel = selection ?: return
        val m = model ?: return
        var dx = selDx; var dy = selDy; val k = selScale
        if (abs(dx) < 0.5f && abs(dy) < 0.5f && abs(k - 1f) < 0.01f) {
            for (part in sel.parts) m.state(part.sheetId).let { it.floating = emptySet(); it.invalidateNode() }
            invalidate()
            return
        }
        if (sel.multi) {
            // Every page's part moves and scales about the same point; each stays on its page.
            val u = selUnion(sel)
            val moved = ArrayList<Selection>()
            var left = sel.parts.size
            for (part in sel.parts) {
                val p = placedFor(part.sheetId)
                if (p == null) { left--; continue }
                val ox = u.left - p.rect.left; val oy = u.top - p.rect.top
                val b = part.bounds
                val nb = RectF(ox + (b.left - ox) * k + dx, oy + (b.top - oy) * k + dy, ox + (b.right - ox) * k + dx, oy + (b.bottom - oy) * k + dy)
                m.transform(part.sheetId, part.ids, dx, dy, k, ox, oy) {
                    m.state(part.sheetId).let { it.floating = emptySet(); it.invalidateNode() }
                    moved += Selection(part.sheetId, part.ids, nb)
                    // Another selection made meanwhile (the core was busy) stays.
                    if (--left == 0 && selection === sel) {
                        val order = sel.parts.map { it.sheetId }
                        val parts = moved.sortedBy { order.indexOf(it.sheetId) }
                        selection = Selection(parts[0].sheetId, parts[0].ids, parts[0].bounds, parts.drop(1))
                        selDx = 0f; selDy = 0f; selScale = 1f
                        listener?.onSelectionChanged(selection)
                        invalidate()
                    }
                }
            }
            return
        }
        // A coordinate system moved on squared paper lands on the squares (its axes on lines).
        val plot = m.sheets[sel.sheetId]?.images?.firstOrNull { it.id in sel.ids && it.plot != null }
        val grid = placedFor(sel.sheetId)?.spec?.takeIf { it.background == uniffi.n0te_ffi.FfiBackground.GRID && it.spacing > 2f }
        if (plot != null && sel.ids.size == 1 && grid != null && abs(k - 1f) < 0.01f) {
            val u = grid.spacing
            dx = Math.round((plot.x + dx) / u) * u - plot.x
            dy = Math.round((plot.y + dy) / u) * u - plot.y
        }
        // Dropped onto another page: the strokes move there.
        val src = placedFor(sel.sheetId)
        val moved = src?.let { selectionRectWorld(sel) }
        val dst = if (board == null && moved != null) m.placed.firstOrNull { it.rect.contains(moved.centerX(), moved.centerY()) } else null
        if (src != null && dst != null && dst.sheetId != sel.sheetId) {
            m.moveStrokes(sel.sheetId, dst.sheetId, sel.ids, dx, dy, k, sel.bounds.left, sel.bounds.top, src.rect.left - dst.rect.left, src.rect.top - dst.rect.top) { added ->
                selectStrokes(dst.sheetId, added)
            }
            return
        }
        m.transform(sel.sheetId, sel.ids, dx, dy, k, sel.bounds.left, sel.bounds.top) {
            if (selection !== sel) return@transform invalidate()
            val nb = RectF(sel.bounds.left + dx, sel.bounds.top + dy, sel.bounds.left + dx + sel.bounds.width() * k, sel.bounds.top + dy + sel.bounds.height() * k)
            selection = Selection(sel.sheetId, sel.ids, nb)
            selDx = 0f; selDy = 0f; selScale = 1f
            listener?.onSelectionChanged(selection)
            invalidate()
        }
    }

    companion object {
        const val PALM_MM = 30f
        const val HOVER_GRACE_MS = 300L
        const val ERASER_PX = 44f
        const val HANDLE_PX = 9f
        const val MARGIN_PT = 40f
        const val HANDLE_R = 15f
        /** A line's end dragged this far (dp) across the line turns it (closer, only its length changes). */
        const val END_TURN_DP = 28f
        /** The biggest layer a selection gets (pixels a side, pixels in all: 32 MB). */
        const val SEL_LAYER_MAX = 4096
        const val SEL_LAYER_PIXELS = 8L shl 20
        const val KIND_PENCIL = 1
        /** The zoom counts as resting this long after it last changed (the ink's tiles are then drawn at it). */
        const val ZOOM_REST_MS = 220L
        const val LAYOUT_ANIM_MS = 300f
        /** The magnifier strip's height (share of the view) and its "move on" part (share of its width). */
        const val LOUPE_H = 0.3f
        const val LOUPE_ADVANCE = 0.22f
        /** A pen down and up within this long without moving is a tap (moves the writing pad on, places its frame). */
        const val TAP_MS = 450L
        /** How long the pen rests before an arrow is tidied (shapes: half a second). */
        const val ARROW_HOLD_MS = 900L
        /** One millimetre in page points. */
        const val MM = 72f / 25.4f
        /** Where the line one writes on sits in the frame (share of its height from the top). */
        const val LOUPE_BASELINE = 0.72f
        const val LASER_TRAIL_MS = 650f
        const val LASER_FADE_MS = 450f
        /** Width of a group title's fold button (points). */
        const val FOLD_W = 30f
        const val DOUBLE_TAP_MS = 300L
        /** A finger resting this long opens the hold menu. */
        const val HOLD_MS = 500L
        const val RULER_LEN = 560f
        const val RULER_W = 56f

        fun normPressure(p: Float, stylus: Boolean) = if (stylus) p.coerceIn(0.05f, 1f) else 0.5f

        /**
         * A scribble: a quick back-and-forth (at least 4 sharp reversals), much longer than the
         * area it covers. Samples are x, y, pressure, t quadruples.
         */
        fun isScribble(pts: FloatArray, n: Int): Boolean {
            if (n < 12) return false
            var x0 = Float.MAX_VALUE; var y0 = Float.MAX_VALUE; var x1 = -Float.MAX_VALUE; var y1 = -Float.MAX_VALUE
            var len = 0f
            for (i in 0 until n) {
                val x = pts[i * 4]; val y = pts[i * 4 + 1]
                x0 = min(x0, x); y0 = min(y0, y); x1 = max(x1, x); y1 = max(y1, y)
                if (i > 0) len += hypot(x - pts[(i - 1) * 4], y - pts[(i - 1) * 4 + 1])
            }
            val diag = hypot(x1 - x0, y1 - y0)
            if (diag < 6f || len < diag * 3f) return false
            if (pts[(n - 1) * 4 + 3] - pts[3] > 2500f) return false // slow: drawing, not scribbling
            // Direction changes over ~2 pt steps.
            val step = max(2f, diag / 40f)
            var reversals = 0
            var ax = pts[0]; var ay = pts[1]
            var pdx = 0f; var pdy = 0f
            for (i in 1 until n) {
                val x = pts[i * 4]; val y = pts[i * 4 + 1]
                val dx = x - ax; val dy = y - ay
                if (hypot(dx, dy) < step) continue
                if (pdx != 0f || pdy != 0f) {
                    val dot = dx * pdx + dy * pdy
                    if (dot < -0.5f * hypot(dx, dy) * hypot(pdx, pdy)) reversals++
                }
                pdx = dx; pdy = dy; ax = x; ay = y
            }
            return reversals >= 4
        }

        fun segDist(px: Float, py: Float, ax: Float, ay: Float, bx: Float, by: Float): Float {
            val dx = bx - ax; val dy = by - ay
            val l2 = dx * dx + dy * dy
            val t = if (l2 < 1e-6f) 0f else (((px - ax) * dx + (py - ay) * dy) / l2).coerceIn(0f, 1f)
            return hypot(px - (ax + t * dx), py - (ay + t * dy))
        }

        fun pointInPolygon(x: Float, y: Float, poly: FloatArray): Boolean {
            val n = poly.size / 2
            var inside = false
            var j = n - 1
            for (i in 0 until n) {
                val xi = poly[2 * i]; val yi = poly[2 * i + 1]; val xj = poly[2 * j]; val yj = poly[2 * j + 1]
                if ((yi > y) != (yj > y) && x < (xj - xi) * (y - yi) / (yj - yi) + xi) inside = !inside
                j = i
            }
            return inside
        }

        /** The eraser circle touches the filled outline polygon. */
        fun polygonNear(poly: FloatArray, x: Float, y: Float, r: Float): Boolean {
            if (pointInPolygon(x, y, poly)) return true
            val n = poly.size / 2
            for (i in 0 until n) {
                val j = (i + 1) % n
                if (segDist(x, y, poly[2 * i], poly[2 * i + 1], poly[2 * j], poly[2 * j + 1]) <= r) return true
            }
            return false
        }

    }
}
