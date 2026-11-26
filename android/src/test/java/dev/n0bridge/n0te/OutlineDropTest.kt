package dev.n0bridge.n0te

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test
import uniffi.n0te_ffi.FfiBackground
import uniffi.n0te_ffi.FfiOutlineRow
import uniffi.n0te_ffi.FfiSheetSpec
import dev.n0bridge.n0te.OutlineDrop.Target

class OutlineDropTest {
    private val spec = FfiSheetSpec(595f, 842f, FfiBackground.GRID, 17f, null, 0u)
    private fun sheet(n: String, d: Int) = FfiOutlineRow.Sheet(n, d.toUInt(), "s-$n", "", spec, emptyList(), emptyList())
    private fun group(n: String, d: Int, collapsed: Boolean = false) = FfiOutlineRow.Group(n, d.toUInt(), n, collapsed, 0u)

    // S1, G(open){ A, B }, S2 — rows 40 px high.
    private val rows = listOf(sheet("S1", 0), group("G", 0), sheet("A", 1), sheet("B", 1), sheet("S2", 0))
    private val bounds = rows.mapIndexed { i, r -> OutlineDrop.nodeOf(r) to (i * 40f to i * 40f + 40f) }.toMap()
    private fun drop(node: String, y: Float, dx: Float = 0f) = OutlineDrop.compute(rows, bounds, node, dx, y, 25f)

    @Test fun moveTopPageToTheEnd() = assertEquals(Target.Gap(null, 2, 0, 200f), drop("S1", 195f))

    @Test fun ontoAGroupsMiddleGoesInside() = assertEquals(Target.Into("G"), drop("S2", 60f))

    @Test fun betweenGroupChildrenStaysInTheGroup() = assertEquals(Target.Gap("G", 1, 1, 120f), drop("S1", 118f))

    @Test fun dragLeftTakesAPageOutOfItsGroup() {
        // B is G's last child; dropped right below itself but one level left: after G at root.
        assertEquals(Target.Gap(null, 2, 0, 160f), drop("B", 158f, dx = -30f))
        // Without the sideways move it stays inside.
        assertEquals(Target.Gap("G", 1, 1, 160f), drop("B", 158f))
    }

    @Test fun dragRightPutsAPageIntoTheOpenGroupAbove() =
        // S2 dropped where it is, moved one level right: end of G (index 2 among A, B).
        assertEquals(Target.Gap("G", 2, 1, 160f), drop("S2", 198f, dx = 30f)) // line under B: S2 itself is skipped

    @Test fun aGroupNeverLandsInsideItself() {
        // Pointer over its own child A: those rows are skipped, so it lands between S1 and S2.
        val t = drop("G", 100f) as Target.Gap
        assertEquals(null, t.parent)
        assertNull(OutlineDrop.subtree(rows, "G").firstOrNull { it == "S1" || it == "S2" })
    }

    @Test fun aSearchHitInAFoldedGroupKnowsWhatToUnfold() {
        // Tapping such a hit did nothing: the page isn't laid out while its groups are folded.
        val side = uniffi.n0te_ffi.FfiSideSheet("R", "s-R", spec)
        val all = listOf(group("U", 0), group("G", 1), sheet("P", 2).copy(right = listOf(side)), sheet("Q", 1), sheet("Z", 0))
        assertEquals(listOf("U", "G"), OutlineDrop.groupsAround(all, "P"))
        assertEquals("a side page: its page's groups", listOf("U", "G"), OutlineDrop.groupsAround(all, "R"))
        assertEquals(listOf("U"), OutlineDrop.groupsAround(all, "Q"))
        assertEquals(emptyList<String>(), OutlineDrop.groupsAround(all, "Z"))
        assertEquals(emptyList<String>(), OutlineDrop.groupsAround(all, "gone"))
    }

    @Test fun thePicturesViewScrollsToThePageNotAboveIt() {
        // The closing line after a group is an item too: scrolling by row index landed too early.
        val all = listOf(group("G", 0), sheet("A", 1), sheet("B", 1), sheet("S", 0), group("H", 0), group("K", 0), sheet("C", 1))
        assertEquals(0, OutlineDrop.gridIndex(all, 0))
        assertEquals(2, OutlineDrop.gridIndex(all, 2))
        assertEquals("a line before S", 4, OutlineDrop.gridIndex(all, 3))
        assertEquals("H right after the group: no line of its own", 5, OutlineDrop.gridIndex(all, 4))
        assertEquals(6, OutlineDrop.gridIndex(all, 5))
        assertEquals(7, OutlineDrop.gridIndex(all, 6))
    }
}
