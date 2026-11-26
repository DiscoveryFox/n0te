package dev.n0bridge.n0te

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.n0te_ffi.FfiBackground
import uniffi.n0te_ffi.FfiOutlineRow
import uniffi.n0te_ffi.FfiSheetSpec

class UiRulesTest {
    private val spec = FfiSheetSpec(595f, 842f, FfiBackground.BLANK, 20f, null, 0u)
    private fun sheet(n: String, d: Int) = FfiOutlineRow.Sheet(n, d.toUInt(), "s-$n", "", spec, emptyList(), emptyList())
    private fun group(n: String, d: Int) = FfiOutlineRow.Group(n, d.toUInt(), n, false, 0u)

    @Test fun theLastPageStaysFromEveryMenu() {
        // Only the page menu refused it; the outline's hold menu, the pictures and the rename
        // dialog deleted the last page (or the last group with every page in it).
        val one = listOf(sheet("a", 0))
        assertFalse(canDelete(one, "a"))
        val inGroup = listOf(group("g", 0), sheet("a", 1), sheet("b", 1))
        assertFalse("the group holds every page", canDelete(inGroup, "g"))
        assertTrue(canDelete(inGroup, "a"))
        assertTrue(canDelete(inGroup + sheet("c", 0), "g"))
        assertTrue("an empty group goes", canDelete(listOf(group("g", 0), sheet("a", 0)), "g"))
        assertTrue("a side page is no row: it may go", canDelete(one, "side-of-a"))
    }

    @Test fun longMessagesStayLongerButNotForever() {
        assertEquals(2000L, flashMs("Kopiert"))
        val hint = "Zwei nebeneinander: der Strich in der Mitte ändert die Breite, ★ ist für Laptop und Gäste"
        assertTrue(flashMs(hint) in 5000L..7000L)
        assertEquals(7000L, flashMs("x".repeat(1000)))
    }
}
