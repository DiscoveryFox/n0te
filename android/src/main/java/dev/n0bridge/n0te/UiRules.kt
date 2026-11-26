package dev.n0bridge.n0te

import uniffi.n0te_ffi.FfiOutlineRow

/** Small rules of the screens, without any screen (JVM-tested). */

/** How long a status message stays: 2 s for a short one, longer hints longer (at most 7 s). */
fun flashMs(text: String): Long = (1500L + 50L * text.length).coerceIn(2000L, 7000L)

/**
 * Whether outline node [node] may be deleted: never the notebook's last page (also not as the
 * last page inside a group being deleted). [rows] is the whole outline (expanded).
 */
fun canDelete(rows: List<FfiOutlineRow>, node: String): Boolean {
    val sheets = rows.filterIsInstance<FfiOutlineRow.Sheet>()
    val gone = OutlineDrop.subtree(rows, node)
    return sheets.any { it.node !in gone }
}
