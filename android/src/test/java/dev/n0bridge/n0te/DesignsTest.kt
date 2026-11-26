package dev.n0bridge.n0te

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import java.nio.file.Files

/** The designs from the Rust core as the tablet sees them, and the decorations' scatter. */
class DesignsTest {
    @Test
    fun builtInDesignsArriveWithTheirLook() {
        val all = uniffi.n0te_ffi.designBuiltins().map(Themes::fromCore)
        assertEquals("nothing", all[0].id)
        assertTrue(all.size >= 14)
        val nothing = all[0]
        assertEquals(0xFF000000, nothing.bg)
        assertEquals(0xFFD71921, nothing.accent)
        assertEquals(LabelVoice.MONO_CAPS, nothing.style.label)
        assertEquals(HeadingFace.DOT, nothing.style.heading)
        assertEquals(Fill.OUTLINE, nothing.style.fill)
        assertEquals(20f, nothing.style.button)
        val xmas = all.first { it.id == "christmas" }
        assertTrue(xmas.decor.any { it.kind == "snow" && it.animated })
        assertTrue(xmas.decor.first { it.kind == "lights" }.colors.all { (it ushr 24) == 0xFFL })
        assertEquals(Fill.RULE, all.first { it.id == "oled" }.style.fill)
        assertEquals(6, uniffi.n0te_ffi.designClassics().toInt())
        // Every classic keeps the id the settings stored before designs came from the core.
        assertEquals(listOf("nothing", "light", "oled", "ocean", "forest", "sepia"), all.take(6).map { it.id })
    }

    @Test
    fun aPastedAnswerIsCheckedThenStored() {
        val dir = Files.createTempDirectory("n0te-designs").toFile()
        val bad = uniffi.n0te_ffi.designRead("{\"name\": \"Mint\"}", false)
        assertNull(bad.design)
        assertTrue(bad.errors.any { it.startsWith("colors") })
        val answer = "Klar!\n```json\n{\"name\":\"Mint\",\"colors\":{\"background\":\"#0b1512\",\"surface\":\"#11201b\",\"text\":\"#e8f5ef\",\"accent\":\"#2dd4a0\"},}\n```"
        val ok = uniffi.n0te_ffi.designRead(answer, false)
        assertNotNull(ok.design)
        val saved = uniffi.n0te_ffi.designImport(dir.absolutePath, answer)
        assertEquals("mint", saved.id)
        val listed = uniffi.n0te_ffi.designImported(dir.absolutePath).map(Themes::fromCore)
        assertEquals(listOf("mint"), listed.map { it.id })
        assertFalse(listed[0].builtin)
        assertEquals(0xFF111111, listed[0].onAccent)
        assertTrue(uniffi.n0te_ffi.designRemove(dir.absolutePath, "mint"))
        assertTrue(uniffi.n0te_ffi.designImported(dir.absolutePath).isEmpty())
        dir.deleteRecursively()
    }

    @Test
    fun theScatterMatchesTheLaptop() {
        // The same numbers as crates/desktop/src/decor.rs (32-bit wrapping arithmetic).
        assertEquals(0.6782859f, Decorations.h(0, 1), 1e-6f)
        assertEquals(0.247704f, Decorations.h(7, 3), 1e-6f)
        assertEquals(0.9187596f, Decorations.h(123456, 11), 1e-6f)
        assertEquals(0.3138365f, Decorations.h(-5, 2), 1e-6f)
        assertEquals(100, Decorations.count(100f, 1200f, 800f))
        assertEquals(12, Decorations.count(100f, 10f, 10f))
    }
}
