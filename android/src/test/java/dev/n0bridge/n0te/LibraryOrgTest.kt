package dev.n0bridge.n0te

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** The library's folders (nested by "/"), their tree, renaming and dissolving them, the search. */
class LibraryOrgTest {
    private val folders = listOf("Uni/WS26/Analysis", "Uni/WS26/Analysis", "Uni/WS26", "Uni/SS25", "Privat", "")

    @Test fun theFolderTreeComesFromThePaths() {
        val tree = LibraryOrg.tree(folders, extra = listOf("Uni/WS26/Lineare Algebra"))
        assertEquals(listOf("Privat", "Uni", "Uni/SS25", "Uni/WS26", "Uni/WS26/Analysis", "Uni/WS26/Lineare Algebra"), tree.map { it.path })
        assertEquals(listOf(0, 0, 1, 1, 2, 2), tree.map { it.depth })
        // Counts include the subfolders; an empty folder made just now counts nothing.
        assertEquals(4, tree.first { it.path == "Uni" }.count)
        assertEquals(3, tree.first { it.path == "Uni/WS26" }.count)
        assertEquals(0, tree.first { it.path == "Uni/WS26/Lineare Algebra" }.count)
        assertTrue(tree.first { it.path == "Uni" }.hasChildren)
        assertFalse(tree.first { it.path == "Privat" }.hasChildren)
        // A folded folder hides what is inside.
        assertEquals(listOf("Privat", "Uni"), LibraryOrg.tree(folders, collapsed = setOf("Uni")).map { it.path })
    }

    @Test fun renamingAndDissolvingTakeTheSubfoldersAlong() {
        assertEquals("Studium/WS26/Analysis", LibraryOrg.renamed("Uni/WS26/Analysis", "Uni", "Studium"))
        assertEquals("Studium", LibraryOrg.renamed("Uni", "Uni", " Studium "))
        assertNull("not inside", LibraryOrg.renamed("Unikram", "Uni", "Studium"))
        assertEquals("Uni", LibraryOrg.dissolved("Uni/WS26", "Uni/WS26"))
        assertEquals("Uni/Analysis", LibraryOrg.dissolved("Uni/WS26/Analysis", "Uni/WS26"))
        assertEquals("", LibraryOrg.dissolved("Privat", "Privat"))
        assertNull(LibraryOrg.dissolved("Uni/SS25", "Uni/WS26"))
        assertEquals("Uni/WS26", LibraryOrg.clean(" Uni // WS26/ "))
    }

    @Test fun theSearchLooksAtTitleFolderAndTags() {
        assertTrue(LibraryOrg.matches("Übungsblatt 3", "Uni/WS26", listOf("Mathe"), "uebung"))
        assertTrue(LibraryOrg.matches("Blatt 3", "Uni/WS26", listOf("Prüfung"), "ws26 prüf"))
        assertFalse(LibraryOrg.matches("Blatt 3", "Uni/WS26", listOf("Mathe"), "physik"))
        assertTrue(LibraryOrg.matches("x", "", emptyList(), "  "))
    }

    @Test fun openingTimesAreKeptNewestFirst() {
        var stored = ""
        stored = LibraryOrg.openedNow(stored, "a", 100)
        stored = LibraryOrg.openedNow(stored, "b", 300)
        stored = LibraryOrg.openedNow(stored, "a", 500)
        assertEquals(mapOf("a" to 500L, "b" to 300L), LibraryOrg.opened(stored))
        assertEquals(listOf("a", "b"), LibraryOrg.opened(stored).entries.sortedByDescending { it.value }.map { it.key })
        // Only the latest few are kept; rubbish is ignored.
        var many = ""
        for (i in 1..50) many = LibraryOrg.openedNow(many, "n$i", i.toLong(), keep = 40)
        assertEquals(40, LibraryOrg.opened(many).size)
        assertEquals(emptyMap<String, Long>(), LibraryOrg.opened("x;y:z;:"))
    }

    @Test fun aTagKeepsItsColour() {
        assertEquals(LibraryOrg.tagColor("Mathe"), LibraryOrg.tagColor("mathe"))
        assertTrue(LibraryOrg.tagColor("Mathe") in LibraryOrg.TAG_COLORS)
    }
}

/** The library's organising and invite links: the bits that went wrong. */
class LibraryOrgFixesTest {
    @Test fun dissolvingAFolderKeepsTheEmptyFoldersInside() {
        // "Auflösen" says nothing is deleted: "A/B/Leer" became nothing.
        assertEquals(listOf("A/Leer", "A", "C"), LibraryOrg.dissolvedFolders(listOf("A/B/Leer", "A/B", "C"), "A/B"))
        assertEquals(listOf("Leer"), LibraryOrg.dissolvedFolders(listOf("X/Leer", "X"), "X"))
        assertEquals(listOf("A/Leer"), LibraryOrg.dissolvedFolders(listOf("A/B/Leer", "A/Leer"), "A/B"))
    }

    @Test fun aTapPicksOrPutsBackAgainstWhatIsPickedNow() {
        assertEquals(setOf("a"), LibraryOrg.toggled(emptySet(), "a"))
        assertEquals(setOf("b"), LibraryOrg.toggled(setOf("a", "b"), "a"))
        assertEquals(null, LibraryOrg.toggled(null, "a"))
    }

    @Test fun inviteLinksAreFoundInWhatIsPastedOrTapped() {
        val link = "n0te://invite/AbC-12_xyz"
        assertEquals(link, LibraryOrg.inviteIn(link))
        assertEquals(link, LibraryOrg.inviteIn("Hier der Link: $link."))
        assertEquals(link, LibraryOrg.inviteIn("($link)"))
        assertEquals(link, LibraryOrg.inviteIn("$link/"))
        assertEquals(link, LibraryOrg.inviteIn("$link?utm=chat"))
        assertEquals(link, LibraryOrg.inviteIn("  N0TE://INVITE/AbC-12_xyz\n"))
        assertEquals(null, LibraryOrg.inviteIn("n0te://invite/"))
        assertEquals(null, LibraryOrg.inviteIn("n0te://invite"))
        assertEquals(null, LibraryOrg.inviteIn("https://example.com"))
        assertEquals(null, LibraryOrg.inviteIn(null))
    }
}
