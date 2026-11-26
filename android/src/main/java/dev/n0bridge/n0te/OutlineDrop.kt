package dev.n0bridge.n0te

import uniffi.n0te_ffi.FfiOutlineRow

/**
 * Where a row dragged in the outline lands (pure, so it is unit-tested). [rows] are the visible
 * outline rows, [bounds] their top/bottom in the list. The pointer's y picks the gap between
 * rows (a group's middle half: into the group, at its end); its x movement since the drag
 * started ([dx], one level per [levelPx]) picks the depth among those the gap allows. The index
 * counts the siblings before the gap without the dragged row (the core's move semantics).
 */
object OutlineDrop {
    sealed class Target {
        data class Gap(val parent: String?, val index: Int, val depth: Int, val lineY: Float) : Target()
        data class Into(val group: String) : Target()
    }

    fun nodeOf(r: FfiOutlineRow) = when (r) { is FfiOutlineRow.Group -> r.node; is FfiOutlineRow.Sheet -> r.node }
    fun depthOf(r: FfiOutlineRow) = when (r) { is FfiOutlineRow.Group -> r.depth.toInt(); is FfiOutlineRow.Sheet -> r.depth.toInt() }

    /**
     * The groups [node] lies in, outermost first, in the whole outline [rows] (a side page counts
     * as its main page): what to unfold to show it.
     */
    fun groupsAround(rows: List<FfiOutlineRow>, node: String): List<String> {
        val stack = ArrayList<Pair<String, Int>>()
        for (r in rows) {
            val d = depthOf(r)
            while (stack.isNotEmpty() && stack.last().second >= d) stack.removeAt(stack.size - 1)
            val hit = nodeOf(r) == node || (r is FfiOutlineRow.Sheet && (r.left.any { it.node == node } || r.right.any { it.node == node }))
            if (hit) return stack.map { it.first }
            if (r is FfiOutlineRow.Group) stack += r.node to d
        }
        return emptyList()
    }

    /**
     * Whether the pictures view puts a closing line before row [i] (a group ended; a group right
     * after one at its level is line enough), given the open groups' depths in [open] (updated).
     */
    fun closesBefore(rows: List<FfiOutlineRow>, i: Int, open: ArrayList<Int>): Int {
        val r = rows[i]
        val d = depthOf(r)
        var closed = -1
        while (open.isNotEmpty() && d <= open.last()) closed = open.removeAt(open.size - 1)
        val line = if (closed >= 0 && !(r is FfiOutlineRow.Group && d == closed)) closed else -1
        if (r is FfiOutlineRow.Group && !r.collapsed) open.add(d)
        return line
    }

    /** Where row [i] is among the pictures view's items (its closing lines count too). */
    fun gridIndex(rows: List<FfiOutlineRow>, i: Int): Int {
        val open = ArrayList<Int>()
        var lines = 0
        for (j in 0..minOf(i, rows.size - 1)) if (closesBefore(rows, j, open) >= 0) lines++
        return i + lines
    }

    /** [node] and the visible rows inside it. */
    fun subtree(rows: List<FfiOutlineRow>, node: String?): Set<String> {
        node ?: return emptySet()
        val i0 = rows.indexOfFirst { nodeOf(it) == node }
        if (i0 < 0) return emptySet()
        val d0 = depthOf(rows[i0])
        val out = hashSetOf(node)
        var i = i0 + 1
        while (i < rows.size && depthOf(rows[i]) > d0) { out += nodeOf(rows[i]); i++ }
        return out
    }

    fun compute(rows: List<FfiOutlineRow>, bounds: Map<String, Pair<Float, Float>>, node: String, dx: Float, y: Float, levelPx: Float): Target? {
        val i0 = rows.indexOfFirst { nodeOf(it) == node }
        if (i0 < 0) return null
        val d0 = depthOf(rows[i0])
        val skip = subtree(rows, node)
        val cand = rows.filter { nodeOf(it) !in skip }
        if (cand.isEmpty()) return Target.Gap(null, 0, 0, 0f)
        var g = cand.size
        for ((k, r) in cand.withIndex()) {
            val (top, bottom) = bounds[nodeOf(r)] ?: continue
            if (y < top) { g = k; break }
            if (y <= bottom) {
                val f = (y - top) / (bottom - top).coerceAtLeast(1f)
                if (r is FfiOutlineRow.Group && f in 0.25f..0.75f) return Target.Into(r.node)
                g = if (f < 0.5f) k else k + 1
                break
            }
        }
        val a = cand.getOrNull(g - 1)
        val b = cand.getOrNull(g)
        val minD = b?.let { depthOf(it) } ?: 0
        val maxD = maxOf(minD, a?.let { if (it is FfiOutlineRow.Group && !it.collapsed) depthOf(it) + 1 else depthOf(it) } ?: 0)
        val steps = if (dx.isNaN() || levelPx <= 0f) 0 else kotlin.math.round(dx / levelPx).toInt()
        val d = (d0 + steps).coerceIn(minD, maxD)
        var parent: String? = null
        var pIdx = -1
        if (d > 0) {
            var k = g - 1
            while (k >= 0 && depthOf(cand[k]) >= d) k--
            val pr = cand.getOrNull(k) as? FfiOutlineRow.Group ?: return null
            parent = pr.node; pIdx = k
        }
        val index = (pIdx + 1 until g).count { depthOf(cand[it]) == d }
        val lineY = b?.let { bounds[nodeOf(it)]?.first } ?: a?.let { bounds[nodeOf(it)]?.second } ?: 0f
        return Target.Gap(parent, index, d, lineY)
    }
}
