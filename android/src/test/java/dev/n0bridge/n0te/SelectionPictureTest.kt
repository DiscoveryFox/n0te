package dev.n0bridge.n0te

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/** "An den Laptop": the selection's picture holds all of it, however big it is. */
class SelectionPictureTest {
    /** Everything drawn lands inside the picture. */
    private fun fits(w: Float, h: Float, s: SelectionPicture.Size) {
        assertTrue("${w * s.scale} px wide in ${s.width}", w * s.scale <= s.width + 0.01f)
        assertTrue("${h * s.scale} px tall in ${s.height}", h * s.scale <= s.height + 0.01f)
        assertTrue(s.width <= SelectionPicture.MAX_SIDE && s.height <= SelectionPicture.MAX_SIDE)
    }

    @Test fun aFewWordsGetThreePixelsPerPoint() {
        val s = SelectionPicture.size(300f, 80f)
        assertEquals(3f, s.scale, 0f)
        assertEquals(900, s.width)
        assertEquals(240, s.height)
        fits(300f, 80f, s)
    }

    @Test fun aLassoOverThreePagesIsNotCutOff() {
        // Three A4 pages one below the other (with the gaps): it used to end at 4096 px (1365 pt).
        val h = 3 * 842f + 2 * 40f
        val s = SelectionPicture.size(595f, h)
        assertEquals("still three pixels per point", 3f, s.scale, 0f)
        fits(595f, h, s)
    }

    @Test fun evenLongerOrWiderSelectionsAreScaledToFit() {
        for ((w, h) in listOf(595f to 8000f, 9000f to 300f, 4000f to 3000f, 20000f to 20000f)) {
            val s = SelectionPicture.size(w, h)
            assertTrue("$w x $h: ${s.scale}", s.scale < 3f && s.scale > 0f)
            fits(w, h, s)
            assertTrue("$w x $h: ${s.width} x ${s.height} px is too much memory", s.width.toLong() * s.height <= SelectionPicture.MAX_PIXELS * 101 / 100)
        }
        // The long side is what limits a long strip: it fills the picture.
        assertEquals(SelectionPicture.MAX_SIDE, SelectionPicture.size(595f, 8000f).height)
    }

    @Test fun lessMemoryStillFitsEverything() {
        val s = SelectionPicture.size(3000f, 2500f, maxPixels = SelectionPicture.MAX_PIXELS / 4)
        fits(3000f, 2500f, s)
        assertTrue(s.width.toLong() * s.height <= SelectionPicture.MAX_PIXELS / 4 * 101 / 100)
    }

    @Test fun aDotStillMakesAPicture() {
        val s = SelectionPicture.size(0f, 0f)
        assertTrue(s.width >= 16 && s.height >= 16)
    }

    @Test fun theMarginGrowsWithTheWidestBrush() {
        assertEquals(6f, SelectionPicture.margin(2f), 0f)
        assertEquals(12f, SelectionPicture.margin(24f), 0f) // a marker
        assertTrue(SelectionPicture.margin(0f) > 0f)
    }
}
