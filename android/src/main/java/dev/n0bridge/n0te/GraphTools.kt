package dev.n0bridge.n0te

import uniffi.n0te_ffi.FfiObjInfo
import uniffi.n0te_ffi.FfiPlot

/**
 * The tools of the graph editor, as in GeoGebra: each collects what it needs (points, objects,
 * places) by tapping in the graph, then makes its object with a command. Names like GeoGebra's
 * German ones (a little shorter), so who knows GeoGebra finds them; [steps] say what to tap next.
 */
enum class GTool(val label: String, val hint: String, val needs: List<GNeed>, val steps: List<String>, val command: (List<String>) -> String?) {
    MOVE("Bewegen", "Antippen wählt aus · Punkte und Schieberegler ziehen · an einer Achse ziehen streckt sie · ins Leere ziehen verschiebt die Ansicht", emptyList(), emptyList(), { null }),
    VIEW("Ansicht verschieben", "Ziehen verschiebt die Ansicht, zwei Finger zoomen", emptyList(), listOf("Ziehen verschiebt die Ansicht, zwei Finger zoomen"), { null }),

    POINT("Punkt", "Ein neuer Punkt, frei oder auf einer Kurve", listOf(GNeed.PLACE), listOf("Tippen setzt einen Punkt · auf einer Kurve liegt er darauf"), { null }),
    INTERSECT("Schneide", "Die Schnittpunkte zweier Objekte", listOf(GNeed.OBJECT, GNeed.OBJECT), listOf("Erstes Objekt antippen", "Zweites Objekt antippen"), { "Schneide(${it[0]}, ${it[1]})" }),
    MIDPOINT("Mittelpunkt", "Zwischen zwei Punkten, oder einer Strecke, eines Kreises", listOf(GNeed.POINT_OR_OBJECT, GNeed.POINT),
        listOf("Ersten Punkt antippen (oder eine Strecke, einen Kreis)", "Zweiten Punkt antippen"), { if (it.size == 1) "Mittelpunkt(${it[0]})" else "Mittelpunkt(${it[0]}, ${it[1]})" }),
    ROOTS("Nullstellen", "Die Nullstellen einer Funktion", listOf(GNeed.OBJECT), listOf("Funktion antippen"), { "Nullstelle(${it[0]})" }),
    EXTREMUM("Extrem\u00ADpunkte", "Hoch- und Tiefpunkte einer Funktion", listOf(GNeed.OBJECT), listOf("Funktion antippen"), { "Extremum(${it[0]})" }),

    LINE("Gerade", "Durch zwei Punkte", listOf(GNeed.POINT, GNeed.POINT), listOf("Ersten Punkt antippen oder setzen", "Zweiten Punkt antippen oder setzen"), { "Gerade(${it[0]}, ${it[1]})" }),
    SEGMENT("Strecke", "Zwischen zwei Punkten", listOf(GNeed.POINT, GNeed.POINT), listOf("Ersten Punkt antippen oder setzen", "Zweiten Punkt antippen oder setzen"), { "Strecke(${it[0]}, ${it[1]})" }),
    RAY("Strahl", "Von einem Punkt durch einen zweiten", listOf(GNeed.POINT, GNeed.POINT), listOf("Anfangspunkt antippen oder setzen", "Einen Punkt auf dem Strahl antippen oder setzen"), { "Strahl(${it[0]}, ${it[1]})" }),
    VECTOR("Vektor", "Von einem Punkt zu einem zweiten", listOf(GNeed.POINT, GNeed.POINT), listOf("Anfangspunkt antippen oder setzen", "Endpunkt antippen oder setzen"), { "Vektor(${it[0]}, ${it[1]})" }),

    PERPENDICULAR("Senkrechte", "Durch einen Punkt, senkrecht zu einer Geraden", listOf(GNeed.POINT, GNeed.OBJECT), listOf("Punkt antippen oder setzen", "Gerade antippen"), { "Senkrechte(${it[0]}, ${it[1]})" }),
    PARALLEL("Parallele", "Durch einen Punkt, parallel zu einer Geraden", listOf(GNeed.POINT, GNeed.OBJECT), listOf("Punkt antippen oder setzen", "Gerade antippen"), { "Gerade(${it[0]}, ${it[1]})" }),
    BISECTOR("Mittel\u00ADsenkrechte", "Senkrecht durch die Mitte zweier Punkte", listOf(GNeed.POINT, GNeed.POINT), listOf("Ersten Punkt antippen oder setzen", "Zweiten Punkt antippen oder setzen"), { "Mittelsenkrechte(${it[0]}, ${it[1]})" }),
    ANGLE_BISECTOR("Winkel\u00ADhalbierende", "Halbiert den Winkel an einem Scheitel", listOf(GNeed.POINT, GNeed.POINT, GNeed.POINT),
        listOf("Ersten Punkt antippen", "Scheitel antippen", "Dritten Punkt antippen"), { "Winkelhalbierende(${it[0]}, ${it[1]}, ${it[2]})" }),
    TANGENT("Tangente", "An eine Funktion oder einen Kreis", listOf(GNeed.POINT, GNeed.OBJECT), listOf("Punkt antippen oder setzen", "Funktion oder Kreis antippen"), { "Tangente(${it[0]}, ${it[1]})" }),

    POLYGON("Vieleck", "Aus Ecken, geschlossen mit der ersten", listOf(GNeed.POLYGON),
        listOf("Erste Ecke antippen oder setzen", "Nächste Ecke", "Nächste Ecke", "Nächste Ecke · zum Schließen die erste antippen"), { if (it.size >= 3) "Vieleck(${it.joinToString(", ")})" else null }),

    CIRCLE("Kreis durch Punkt", "Mit Mittelpunkt, durch einen Punkt", listOf(GNeed.POINT, GNeed.POINT), listOf("Mittelpunkt antippen oder setzen", "Punkt auf dem Kreis antippen oder setzen"), { "Kreis(${it[0]}, ${it[1]})" }),
    CIRCLE_RADIUS("Kreis mit Radius", "Mit Mittelpunkt und Radius", listOf(GNeed.POINT, GNeed.NUMBER), listOf("Mittelpunkt antippen oder setzen"), { "Kreis(${it[0]}, ${it[1]})" }),
    CIRCLE3("Kreis durch 3 Punkte", "Durch drei Punkte", listOf(GNeed.POINT, GNeed.POINT, GNeed.POINT), listOf("Ersten Punkt antippen oder setzen", "Zweiten Punkt", "Dritten Punkt"), { "Kreis(${it[0]}, ${it[1]}, ${it[2]})" }),

    ANGLE("Winkel", "An einem Scheitel, gegen den Uhrzeigersinn", listOf(GNeed.POINT, GNeed.POINT, GNeed.POINT), listOf("Ersten Punkt antippen", "Scheitel antippen", "Zweiten Punkt antippen"), { "Winkel(${it[0]}, ${it[1]}, ${it[2]})" }),
    DISTANCE("Abstand oder Länge", "Zwischen zwei Punkten, oder Punkt und Gerade", listOf(GNeed.POINT, GNeed.POINT_OR_OBJECT), listOf("Ersten Punkt antippen", "Zweiten Punkt oder eine Gerade antippen"), { "Text(Abstand(${it[0]}, ${it[1]}))" }),
    AREA("Fläche", "Eines Vielecks oder Kreises", listOf(GNeed.OBJECT), listOf("Vieleck oder Kreis antippen"), { "Text(Fläche(${it[0]}))" }),
    SLOPE("Steigung", "Einer Geraden", listOf(GNeed.OBJECT), listOf("Gerade antippen"), { "Text(Steigung(${it[0]}))" }),

    REFLECT_LINE("Spiegeln an Gerade", "Ein Objekt an einer Geraden spiegeln", listOf(GNeed.OBJECT, GNeed.OBJECT), listOf("Objekt antippen", "Spiegelgerade antippen"), { "Spiegle(${it[0]}, ${it[1]})" }),
    REFLECT_POINT("Spiegeln an Punkt", "Ein Objekt an einem Punkt spiegeln", listOf(GNeed.OBJECT, GNeed.POINT), listOf("Objekt antippen", "Spiegelpunkt antippen"), { "Spiegle(${it[0]}, ${it[1]})" }),
    ROTATE("Drehen um Punkt", "Ein Objekt um einen Punkt drehen", listOf(GNeed.OBJECT, GNeed.POINT, GNeed.NUMBER), listOf("Objekt antippen", "Drehpunkt antippen"), { "Drehe(${it[0]}, ${it[2]}°, ${it[1]})" }),
    TRANSLATE("Verschieben mit Vektor", "Ein Objekt um einen Vektor verschieben", listOf(GNeed.OBJECT, GNeed.OBJECT), listOf("Objekt antippen", "Vektor antippen"), { "Verschiebe(${it[0]}, ${it[1]})" }),

    SLIDER("Schiebe\u00ADregler", "Eine Zahl zum Ziehen", listOf(GNeed.PLACE), listOf("Tippen, wo er stehen soll"), { null }),
    TEXT("Text", "Ein Text im Graphen", listOf(GNeed.PLACE), listOf("Tippen, wo er stehen soll"), { null });

    /** The name on the top bar's buttons: its first word. */
    val short: String get() = label.substringBefore(' ')

    companion object {
        /** The tools offered first (like GeoGebra's basic tools). */
        val basic = listOf(MOVE, POINT, LINE, SEGMENT, POLYGON, CIRCLE, INTERSECT, SLIDER)

        /** All tools by what they do (GeoGebra's tool groups). */
        val categories = listOf(
            "Punkte" to listOf(POINT, INTERSECT, MIDPOINT, ROOTS, EXTREMUM),
            "Geraden" to listOf(LINE, SEGMENT, RAY, VECTOR, PERPENDICULAR, PARALLEL, BISECTOR, ANGLE_BISECTOR, TANGENT),
            "Vielecke und Kreise" to listOf(POLYGON, CIRCLE, CIRCLE_RADIUS, CIRCLE3),
            "Messen" to listOf(ANGLE, DISTANCE, AREA, SLOPE),
            "Abbilden" to listOf(REFLECT_LINE, REFLECT_POINT, ROTATE, TRANSLATE),
            "Sonstiges" to listOf(SLIDER, TEXT, VIEW, MOVE),
        )
    }
}

/** What the graph editor remembers between graphs: the tools used last (on the top bar), all tools shown. */
object GraphPrefs {
    private fun tools(key: String, def: String) = Settings.str(key, def).split(',')
        .mapNotNull { n -> GTool.entries.firstOrNull { it.name == n } }.filter { it != GTool.MOVE }.distinct()

    /** The top bar's tools, in their places. */
    var recent: List<GTool> = tools("graph_recent", "POINT,LINE,CIRCLE,INTERSECT").take(4)
        private set
    /** The same, the one used last first. */
    private var lastUsed: List<GTool> = tools("graph_used", "POINT,LINE,CIRCLE,INTERSECT")

    var allTools: Boolean = Settings.str("graph_all_tools", "0") == "1"
        set(v) { field = v; Settings.set("graph_all_tools", if (v) "1" else "0") }

    /** A tool was chosen: a new one takes the place of the one unused longest; the others stay where they are. */
    fun used(t: GTool) {
        if (t == GTool.MOVE) return
        lastUsed = (listOf(t) + lastUsed.filter { it != t }).take(12)
        if (t !in recent) {
            recent = if (recent.size < 4) recent + t
            else { val out = recent.maxBy { r -> lastUsed.indexOf(r).let { if (it < 0) Int.MAX_VALUE else it } }; recent.map { if (it == out) t else it } }
            Settings.set("graph_recent", recent.joinToString(",") { it.name })
        }
        Settings.set("graph_used", lastUsed.joinToString(",") { it.name })
    }
}

/** What a tool waits for next. */
enum class GNeed { POINT, OBJECT, POINT_OR_OBJECT, PLACE, POLYGON, NUMBER }

/**
 * One editing session of a graph: the draft (what the page shows), its history, the tool and
 * what it has collected so far, the selected object. Changes come back through [onChange].
 */
class GraphSession(val sheetId: String, start: FfiPlot, val fresh: Boolean, private val onChange: () -> Unit) {
    var draft: FfiPlot = start
        private set
    private val undo = ArrayDeque<FfiPlot>()
    private val redo = ArrayDeque<FfiPlot>()
    var tool: GTool = GTool.MOVE
        private set
    /** Names picked for the running tool (and ids, to show them chosen). */
    val picks = ArrayList<String>()
    val pickIds = ArrayList<String>()
    var selected: String? = null
        private set
    /** A hint (not a mistake). */
    var message: String? = null
    /** What went wrong with the last step. */
    var error: String? = null
    /** What just happened, to take back ("A gelöscht"). */
    var notice: String? = null
    /** The panel: 0 algebra, 1 tools, 2 graphics; [settings]: the object whose settings show instead. */
    var tab = 0
        set(v) { field = v; onChange() }
    var settings: String? = null
        private set
    /** The view the editing started with ("Ansicht zurücksetzen"). */
    private var home = start
    /** The page's squares under the graph (page points), 0: no squared paper. */
    var squares = 0f
    /** Edited in the whole room ("Groß"): its box is not on the page meanwhile. */
    var big = false
    /**
     * While [big]: where the room's top-left corner lies on the page (page points). Going back
     * keeps the middle, so the room is the page shifted by this: the squares drawn behind it
     * are the page's own.
     */
    var bigAt: android.graphics.PointF? = null
    /** The view was moved or zoomed by hand since it last came to rest. */
    private var viewMoved = false
    /** The graph keeps to the squared paper (its own setting, on squared paper by default). */
    val onSquares get() = draft.align && squares > 0f
    /** The graph keyboard and the field it types into. */
    val keys = GraphKeys()
    /** A number the tool waits for (radius, angle): its question. */
    var asking: String? = null
        private set
    private var infoCache: Pair<FfiPlot, List<FfiObjInfo>>? = null

    val canUndo get() = undo.isNotEmpty()
    val canRedo get() = redo.isNotEmpty()
    val changed get() = undo.isNotEmpty()

    val infos: List<FfiObjInfo>
        get() = infoCache?.takeIf { it.first === draft }?.second ?: uniffi.n0te_ffi.plotInfo(draft).also { infoCache = draft to it }

    fun infoOf(id: String?) = infos.firstOrNull { it.id == id }
    fun nameOf(id: String) = infoOf(id)?.name ?: ""

    /** A new draft (one step back to undo); [quiet]: a drag in progress, no new step. */
    fun set(p: FfiPlot, quiet: Boolean = false) {
        if (p == draft) return
        if (!quiet) { undo.addLast(draft); while (undo.size > 200) undo.removeFirst(); redo.clear(); notice = null }
        draft = p
        if (settings != null && draft.objs.none { it.id == settings }) settings = null
        onChange()
    }

    /** Something only the screen shows changed (a hint went away). */
    fun refresh() = onChange()

    fun select(id: String?) { selected = id; onChange() }

    /** The settings of [id] in the panel (null: back to the tabs). */
    fun openSettings(id: String?) { settings = id; if (id != null) selected = id; onChange() }

    /** Back to the view the editing started with. */
    fun resetView() = set(draft.copy(xMin = home.xMin, xMax = home.xMax, yMin = home.yMin, yMax = home.yMax, pi = home.pi))

    /**
     * Edits the graph in another box (page points): the whole room for "Groß", its box on the
     * page again after. Its scale and middle stay, so it shows more or less around them; every
     * step to undo moves along.
     */
    fun reframe(x: Float, y: Float, w: Float, h: Float, keepTop: Boolean = false) {
        if (w <= 0f || h <= 0f || (draft.x == x && draft.y == y && draft.w == w && draft.h == h)) return
        draft = framed(draft, x, y, w, h, keepTop)
        for (i in undo.indices) undo[i] = framed(undo[i], x, y, w, h, keepTop)
        for (i in redo.indices) redo[i] = framed(redo[i], x, y, w, h, keepTop)
        home = framed(home, x, y, w, h, keepTop)
        onChange()
    }

    /** What to tap next for the running tool. */
    val step: String
        get() {
            asking?.let { return it }
            val st = tool.steps
            if (st.isEmpty()) return tool.hint
            return st[picks.size.coerceAtMost(st.size - 1)]
        }

    /** Step i of n, for tools of several steps. */
    val stepOf: Pair<Int, Int>?
        get() {
            val n = tool.needs.size
            if (n < 2 || tool.needs.first() == GNeed.POLYGON) return null
            return (picks.size + 1).coerceAtMost(n) to n
        }

    /** A drag starts: the plot before it is the step to undo. */
    fun beginGesture() { undo.addLast(draft); redo.clear() }

    fun undo() { val p = undo.removeLastOrNull() ?: return; redo.addLast(draft); draft = p; clearPicks(); notice = null; error = null; fixRefs(); onChange() }
    fun redo() { val p = redo.removeLastOrNull() ?: return; undo.addLast(draft); draft = p; clearPicks(); notice = null; error = null; fixRefs(); onChange() }

    /** Selected or open objects that are gone (undone) let go. */
    private fun fixRefs() {
        if (draft.objs.none { it.id == selected }) selected = null
        if (draft.objs.none { it.id == settings }) settings = null
    }

    fun choose(t: GTool) {
        tool = t
        clearPicks()
        message = null
        error = null
        if (t != GTool.MOVE) selected = null
        GraphPrefs.used(t)
        onChange()
    }

    fun clearPicks() { picks.clear(); pickIds.clear(); asking = null }

    /** A line of input (GeoGebra's input bar); null: done, else what is wrong. */
    fun input(text: String): String? {
        if (text.isBlank()) return null
        return try {
            val before = draft.objs.map { it.id }.toSet()
            set(uniffi.n0te_ffi.plotAdd(draft, plain(text)))
            // The typed object (sliders made for its new letters come before it).
            selected = draft.objs.lastOrNull { it.id !in before }?.id
            onChange()
            null
        } catch (e: Exception) { errText(e) }
    }

    fun setDef(id: String, def: String): String? = try {
        set(uniffi.n0te_ffi.plotSetDef(draft, id, plain(def))); null
    } catch (e: Exception) { errText(e) }

    /** A function's domain from [from] to [to] (null: open that way); null: done, else what is wrong. */
    fun setDomain(id: String, from: Double?, to: Double?): String? = try {
        set(uniffi.n0te_ffi.plotSetDomain(draft, id, from, to)); null
    } catch (e: Exception) { errText(e) }

    /** The row's switch: [curve] a curve of the letters (a(b) = 3b), else a number with a slider (a = 3b). */
    fun asCurve(id: String, curve: Boolean) {
        runCatching { set(if (curve) uniffi.n0te_ffi.plotAsCurve(draft, id) else uniffi.n0te_ffi.plotAsSliders(draft, id)) }
            .onFailure { error = errText(it); onChange() }
    }

    /** How a typed line looks set (LaTeX), for the preview under the input; "" while it doesn't read yet. */
    fun preview(text: String): String {
        if (text.isBlank()) return ""
        // Typed as LaTeX: shown as typed.
        if ('\\' in text) return text.trim()
        return runCatching { uniffi.n0te_ffi.plotInputLatex(draft, text.trim()) }.getOrDefault("")
    }

    /** Away with an object and what depends on it; the notice offers to take it back. */
    fun remove(id: String) {
        val name = nameOf(id)
        val deps = runCatching { uniffi.n0te_ffi.plotDependents(draft, id) }.getOrDefault(emptyList())
        set(uniffi.n0te_ffi.plotRemove(draft, id))
        fixRefs()
        notice = if (deps.isEmpty()) "$name gelöscht" else "$name gelöscht, mit ${deps.joinToString(", ")}"
        onChange()
    }

    fun style(id: String, change: (uniffi.n0te_ffi.FfiStyle) -> uniffi.n0te_ffi.FfiStyle) {
        set(draft.copy(objs = draft.objs.map { if (it.id == id) it.copy(style = change(it.style)) else it }))
    }

    fun view(change: (FfiPlot) -> FfiPlot) = set(change(draft))

    /** The plot as the page shows it while editing: picked and selected objects drawn bolder. */
    fun shown(): FfiPlot {
        val marked = pickIds + listOfNotNull(selected)
        if (marked.isEmpty()) return draft
        return draft.copy(objs = draft.objs.map { o ->
            if (o.id in marked) o.copy(style = o.style.copy(width = (if (o.style.width > 0f) o.style.width else draft.width) * 2.2f, size = if (o.style.size > 0f) o.style.size * 1.9f else 4.6f)) else o
        })
    }

    // --- tapping in the graph -------------------------------------------------------------

    /**
     * A tap in the graph at page point (px, py), with the object there ([hit], an id). Collects
     * for the running tool; when it has all it needs, makes the object.
     */
    fun tap(px: Float, py: Float, hit: String?) {
        message = null
        error = null
        notice = null
        val (x, y) = uniffi.n0te_ffi.plotToPlot(draft, px, py).let { it[0] to it[1] }
        when (tool) {
            GTool.MOVE, GTool.VIEW -> { selected = hit; if (hit != null && settings != null) settings = hit; onChange(); return }
            GTool.POINT -> {
                val info = infoOf(hit)
                val onObj = hit != null && info != null && info.kind in listOf("Funktion", "Gerade", "Strecke", "Kreis")
                runCatching {
                    if (onObj) set(uniffi.n0te_ffi.plotAddPointOn(draft, hit!!, x, y))
                    else { val (sx, sy) = snapped(x, y); set(uniffi.n0te_ffi.plotAdd(draft, "(${num(sx)}, ${num(sy)})")) }
                }.onFailure { error = errText(it) }
                onChange(); return
            }
            GTool.SLIDER -> {
                val before = draft.objs.map { it.id }.toSet()
                runCatching { set(uniffi.n0te_ffi.plotAdd(draft, "Schieberegler(-5, 5, 0.1)")) }
                draft.objs.firstOrNull { it.id !in before }?.let { o -> placeAt(o.id, px, py); selected = o.id }
                onChange(); return
            }
            GTool.TEXT -> {
                val before = draft.objs.map { it.id }.toSet()
                runCatching { set(uniffi.n0te_ffi.plotAdd(draft, "Text(\"Text\")")) }
                // Its words are typed in its settings.
                draft.objs.firstOrNull { it.id !in before }?.let { o -> placeAt(o.id, px, py); selected = o.id; settings = o.id }
                onChange(); return
            }
            else -> {}
        }
        val need = nextNeed() ?: return
        val info = infoOf(hit)
        val isPoint = info != null && info.kind in listOf("Punkt", "Punkte")
        when (need) {
            GNeed.POINT, GNeed.POLYGON -> {
                // The polygon closes when its first corner is tapped again.
                if (need == GNeed.POLYGON && picks.size >= 3 && hit != null && hit == pickIds.firstOrNull()) { finish(); return }
                val (name, id) = if (isPoint) info!!.name to info.id else newPointAt(x, y, hit) ?: return
                pick(name, id)
            }
            GNeed.OBJECT -> {
                if (info == null) { error = "Da ist kein Objekt, ein Objekt antippen"; onChange(); return }
                pick(info.name, info.id)
            }
            GNeed.POINT_OR_OBJECT -> {
                if (info != null) pick(info.name, info.id) else { val (n, id) = newPointAt(x, y, null) ?: return; pick(n, id) }
                // Mittelpunkt of a segment or circle needs only that.
                if (tool == GTool.MIDPOINT && picks.size == 1 && info != null && info.kind in listOf("Strecke", "Kreis")) { finish(); return }
            }
            GNeed.PLACE, GNeed.NUMBER -> {}
        }
        if (nextNeed() == GNeed.NUMBER) {
            asking = if (tool == GTool.ROTATE) "Winkel in Grad" else "Radius"
            onChange(); return
        }
        if (nextNeed() == null) finish() else onChange()
    }

    /** The number a tool asked for (radius, angle). */
    fun answer(text: String): String? {
        val v = text.trim().replace(',', '.')
        if (v.isEmpty()) return "Eine Zahl eingeben"
        picks += v; pickIds += ""
        asking = null
        return finish()
    }

    private fun nextNeed(): GNeed? {
        val needs = tool.needs
        if (needs.firstOrNull() == GNeed.POLYGON) return GNeed.POLYGON
        return needs.getOrNull(picks.size)
    }

    private fun pick(name: String, id: String) { picks += name; pickIds += id }

    private fun finish(): String? {
        val cmd = tool.command(picks.toList())
        val err = cmd?.let { c ->
            runCatching { set(uniffi.n0te_ffi.plotAdd(draft, c)); null }.getOrElse { errText(it) }
        }
        error = err
        clearPicks()
        onChange()
        return err
    }

    /** A new point for a tool at (x, y): on the object there, else free (snapped). Its name and id. */
    private fun newPointAt(x: Double, y: Double, hit: String?): Pair<String, String>? {
        val before = draft.objs.map { it.id }.toSet()
        val info = infoOf(hit)
        val r = runCatching {
            if (hit != null && info != null && info.kind in listOf("Funktion", "Gerade", "Strecke", "Kreis")) uniffi.n0te_ffi.plotAddPointOn(draft, hit, x, y)
            else { val (sx, sy) = snapped(x, y); uniffi.n0te_ffi.plotAdd(draft, "(${num(sx)}, ${num(sy)})") }
        }.getOrElse { error = errText(it); onChange(); return null }
        set(r)
        return draft.objs.firstOrNull { it.id !in before }?.let { o -> (infos.firstOrNull { it.id == o.id }?.name ?: return null) to o.id }
    }

    private fun snapped(x: Double, y: Double): Pair<Double, Double> = uniffi.n0te_ffi.plotSnapped(draft, x, y).let { it[0] to it[1] }

    /** Puts a slider or text at page point (px, py) in the box. */
    fun placeAt(id: String, px: Float, py: Float) {
        val fx = ((px - draft.x) / draft.w).coerceIn(0f, 0.95f)
        val fy = ((py - draft.y) / draft.h).coerceIn(0f, 0.95f)
        set(draft.copy(objs = draft.objs.map { if (it.id == id) it.copy(style = it.style.copy(at = listOf(fx, fy))) else it }), quiet = true)
    }

    // --- dragging in the graph ------------------------------------------------------------

    private enum class Drag { NONE, POINT, SLIDER, VIEW, DOMAIN, AXIS }
    private var drag = Drag.NONE
    private var dragId: String? = null
    private var lastX = 0f
    private var lastY = 0f
    /** Dragging an end of a domain: which (0 from, 1 to); an axis: which (1 x, 2 y). */
    private var dragSide: Int = 0
    /** Stretching an axis: the plot when the pen went down, and where. */
    private var axisFrom: FfiPlot? = null
    private var axisDownX = 0f
    private var axisDownY = 0f

    /**
     * A finger or the pen went down at page point (px, py) on [hit] ([tol]: page points that
     * count as on something). True if it drags something: a slider, an end of a domain, a
     * point, an axis (stretched alone, like in GeoGebra), else the view.
     */
    fun down(px: Float, py: Float, hit: String?, tol: Float = 6f): Boolean {
        lastX = px; lastY = py
        drag = Drag.NONE; dragId = null; axisFrom = null
        if (tool != GTool.MOVE && tool != GTool.VIEW) return false
        val info = infoOf(hit)
        val end = if (tool == GTool.MOVE && info?.sliderValue == null) uniffi.n0te_ffi.plotDomainEndAt(draft, px, py, tol) else null
        val axis = if (tool == GTool.MOVE && info == null && end == null) uniffi.n0te_ffi.plotAxisAt(draft, px, py, tol).toInt() else 0
        when {
            tool == GTool.VIEW -> drag = Drag.VIEW
            info?.sliderValue != null -> { drag = Drag.SLIDER; dragId = hit }
            end != null -> { drag = Drag.DOMAIN; dragId = end.id; dragSide = end.side.toInt(); selected = end.id }
            info?.draggable == true -> { drag = Drag.POINT; dragId = hit }
            axis != 0 -> { drag = Drag.AXIS; dragSide = axis; axisFrom = draft; axisDownX = px; axisDownY = py }
            else -> drag = Drag.VIEW
        }
        beginGesture()
        return true
    }

    fun move(px: Float, py: Float) {
        when (drag) {
            Drag.NONE -> return
            Drag.POINT -> {
                val (x, y) = uniffi.n0te_ffi.plotToPlot(draft, px, py).let { it[0] to it[1] }
                uniffi.n0te_ffi.plotDrag(draft, dragId ?: return, x, y)?.let { set(it, quiet = true) }
            }
            Drag.SLIDER -> {
                val id = dragId ?: return
                val track = uniffi.n0te_ffi.plotSliderTrack(draft, id)
                val s = draft.objs.firstOrNull { it.id == id }?.style?.slider ?: return
                if (track.size == 3) {
                    val t = ((px - track[0]) / (track[1] - track[0])).coerceIn(0f, 1f)
                    uniffi.n0te_ffi.plotSetSlider(draft, id, s.min + (s.max - s.min) * t)?.let { set(it, quiet = true) }
                }
            }
            Drag.VIEW -> {
                val ux = (draft.xMax - draft.xMin) / draft.w; val uy = (draft.yMax - draft.yMin) / draft.h
                val dx = (px - lastX) * ux; val dy = (py - lastY) * uy
                if (dx != 0.0 || dy != 0.0) viewMoved = true
                set(draft.copy(xMin = draft.xMin - dx, xMax = draft.xMax - dx, yMin = draft.yMin + dy, yMax = draft.yMax + dy), quiet = true)
            }
            Drag.DOMAIN -> {
                val x = uniffi.n0te_ffi.plotToPlot(draft, px, py)[0]
                uniffi.n0te_ffi.plotDragDomainEnd(draft, dragId ?: return, dragSide.toUByte(), x)?.let { set(it, quiet = true) }
            }
            Drag.AXIS -> {
                // Always from the plot as it was when the pen went down: the number grabbed stays under the pen.
                val start = axisFrom ?: return
                val stretched = if (dragSide == 1) uniffi.n0te_ffi.plotAxisDragged(start, 1.toUByte(), axisDownX, px) else uniffi.n0te_ffi.plotAxisDragged(start, 2.toUByte(), axisDownY, py)
                viewMoved = true
                set(stretched.copy(x = draft.x, y = draft.y, w = draft.w, h = draft.h), quiet = true)
            }
        }
        lastX = px; lastY = py
    }

    /** Lifted: a drag ends (a drag that changed nothing is no step). */
    fun up() {
        if (drag != Drag.NONE && undo.lastOrNull() == draft) undo.removeLast()
        drag = Drag.NONE; dragId = null
        onChange()
    }

    /**
     * The fingers let go: a view moved or zoomed by hand snaps to squared paper's squares —
     * a round number of units to a square (… ½, 1, 2, 5 …), and on the page the axes on the lines.
     */
    fun settle() {
        if (!viewMoved) return
        viewMoved = false
        if (!onSquares) return
        val at = bigAt.takeIf { big }
        val p = if (at != null) draft.copy(x = at.x, y = at.y) else draft
        set(aligned(p, squares, offsets = !big || at != null).copy(x = draft.x, y = draft.y), quiet = true)
    }

    /** Back into its box on the page (page points) after "Groß", snapped to the squares there. */
    fun toPage(box: android.graphics.RectF) {
        reframe(box.left, box.top, box.width(), box.height())
        if (onSquares) set(aligned(draft, squares, offsets = true), quiet = true)
    }

    /** Two fingers in the graph: zoom by [k] about page point (cx, cy). */
    fun pinch(k: Float, cx: Float, cy: Float) {
        if (k <= 0f) return
        val (x, y) = uniffi.n0te_ffi.plotToPlot(draft, cx, cy).let { it[0] to it[1] }
        val f = 1.0 / k
        viewMoved = true
        set(draft.copy(xMin = x + (draft.xMin - x) * f, xMax = x + (draft.xMax - x) * f, yMin = y + (draft.yMin - y) * f, yMax = y + (draft.yMax - y) * f), quiet = true)
    }

    companion object {
        /**
         * [p] on squared paper of [s] points to a square: a round number of units to a square
         * (1, 2, 5 times a power of ten, the nearest), about the same middle; with [offsets] (its
         * box on the page, where the lines lie at multiples of [s]) also 0 on both axes on a line,
         * so the axes, ticks and grid lie on the paper's lines.
         */
        fun aligned(p: FfiPlot, s: Float, offsets: Boolean): FfiPlot {
            if (s <= 0f || p.w <= 0f || p.h <= 0f || p.xMax <= p.xMin || p.yMax <= p.yMin) return p
            // Units to a square, round (what is round already, up to float noise, stays as it is).
            fun round(v: Double): Double {
                val mag = Math.pow(10.0, kotlin.math.floor(kotlin.math.log10(v)))
                val r = listOf(1.0, 2.0, 5.0, 10.0).minBy { kotlin.math.abs(kotlin.math.ln(it * mag / v)) } * mag
                return if (kotlin.math.abs(r - v) < 1e-4 * r) v else r
            }
            val qx = round((p.xMax - p.xMin) / p.w * s)
            val qy = round((p.yMax - p.yMin) / p.h * s)
            val ux = qx / s; val uy = qy / s
            var xMin = (p.xMin + p.xMax) / 2 - ux * p.w / 2
            var yMax = (p.yMin + p.yMax) / 2 + uy * p.h / 2
            if (offsets) {
                // Value 0 at page x = p.x + (0 − xMin) / ux on a line: xMin = ux·p.x + n·qx (y alike, downwards).
                fun onLine(v: Double, base: Double, q: Double): Double {
                    val n = (v - base) / q
                    return if (kotlin.math.abs(n - Math.round(n)) < 1e-4) v else base + Math.round(n) * q
                }
                xMin = onLine(xMin, ux * p.x, qx)
                yMax = onLine(yMax, -uy * p.y, qy)
            }
            fun clean(v: Double) = Math.round(v * 1e9) / 1e9
            val out = p.copy(xMin = clean(xMin), xMax = clean(xMin + ux * p.w), yMin = clean(yMax - uy * p.h), yMax = clean(yMax))
            // Nothing to snap: the very same plot (no step to undo, nothing redrawn).
            val same = kotlin.math.abs(out.xMin - p.xMin) + kotlin.math.abs(out.xMax - p.xMax) + kotlin.math.abs(out.yMin - p.yMin) + kotlin.math.abs(out.yMax - p.yMax) < 1e-6 * (qx + qy)
            return if (same) p else out
        }

        /**
         * [p] in the box (x, y, w, h): the same units per point, the same middle — or, with
         * [keepTop], the same top-left (the graph keyboard came or went: nothing moves, the
         * bottom is cut or shows again).
         */
        fun framed(p: FfiPlot, x: Float, y: Float, w: Float, h: Float, keepTop: Boolean = false): FfiPlot {
            if (p.w <= 0f || p.h <= 0f) return p.copy(x = x, y = y, w = w, h = h)
            val ux = (p.xMax - p.xMin) / p.w; val uy = (p.yMax - p.yMin) / p.h
            if (keepTop) return p.copy(x = x, y = y, w = w, h = h, xMax = p.xMin + ux * w, yMin = p.yMax - uy * h)
            val cx = (p.xMin + p.xMax) / 2; val cy = (p.yMin + p.yMax) / 2
            return p.copy(x = x, y = y, w = w, h = h, xMin = cx - ux * w / 2, xMax = cx + ux * w / 2, yMin = cy - uy * h / 2, yMax = cy + uy * h / 2)
        }

        /** A typed line for the core: LaTeX (\frac{1}{2}, \sqrt{x}, \pi …) turned into the input language. */
        fun plain(text: String): String {
            val t = text.trim()
            return if ('\\' in t) uniffi.n0te_ffi.plotFromLatex(t) else t
        }

        /** What the core said is wrong (its German message, not the exception's wrapping). */
        fun errText(e: Throwable): String = (e as? uniffi.n0te_ffi.N0teException.Failed)?.msg ?: e.message ?: "Fehler"

        /** A number for a definition: dot decimals (it reads back exactly). */
        fun num(v: Double): String {
            val r = Math.round(v * 1e6) / 1e6
            return if (r == Math.rint(r)) r.toLong().toString() else r.toString()
        }
    }
}
