package dev.n0bridge.n0te

import uniffi.n0te_ffi.FfiBackground
import uniffi.n0te_ffi.FfiPlot
import uniffi.n0te_ffi.FfiSheetSpec

/**
 * Stretching an axis of an interactive graph right on the page (not while it is edited): the
 * x or y axis dragged alone, as in the graph editor.
 */
object PageAxis {
    /**
     * Whether a touch may grab an axis: the pen when it writes (pen or marker, not erasing);
     * a finger only where fingers write, and not a resting palm or a finger while the pen is near.
     */
    fun allowed(stylus: Boolean, erasing: Boolean, tool: NotebookView.Tool, penOnly: Boolean, navigateOnly: Boolean, palm: Boolean, penNear: Boolean): Boolean {
        if (navigateOnly || erasing || (tool != NotebookView.Tool.PEN && tool != NotebookView.Tool.MARKER)) return false
        return stylus || (!penOnly && !palm && !penNear)
    }

    /** How near the axis counts (page points at [scale] pixels per point): the pen's point is fine, a finger isn't. */
    fun tolerance(stylus: Boolean, density: Float, scale: Float): Float = (if (stylus) 7f else 14f) * density / scale

    /** The squares of the page under a graph (page points), 0 when it isn't squared paper. */
    fun squares(spec: FfiSheetSpec): Float =
        if (spec.width > 0f && spec.pdfBlob == null && spec.background == FfiBackground.GRID && spec.spacing > 2f) spec.spacing else 0f

    /** The stretched graph as it is put down: on squared paper snapped to the squares, as in the editor. */
    fun settled(p: FfiPlot, squares: Float): FfiPlot = if (p.align && squares > 0f) GraphSession.aligned(p, squares, offsets = true) else p
}
