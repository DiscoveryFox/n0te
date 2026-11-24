package dev.n0bridge.n0te

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** Two notebooks side by side: where the panes go, where the divider rests, what is kept. */
class SplitGeometryTest {
    @Test fun onePaneTakesTheWholeScreen() {
        assertEquals(listOf(Area(0, 0, 2560, 1600)), SplitGeometry.rects(2560, 1600, 1, 0.3f, 12))
    }

    @Test fun twoPanesShareTheWidthOnALandscapeScreenWithAGapBetween() {
        val (a, b) = SplitGeometry.rects(2560, 1600, 2, 0.5f, 12)
        assertEquals(Area(0, 0, 1274, 1600), a)
        assertEquals(Area(1286, 0, 2560, 1600), b)
        assertEquals(12, b.left - a.right)
    }

    @Test fun uprightTheyStandAboveEachOther() {
        val (a, b) = SplitGeometry.rects(1600, 2560, 2, 1f / 3f, 10)
        assertEquals(0, a.top); assertEquals(1600, a.width())
        assertEquals(853 - 5, a.bottom)
        assertEquals(853 + 5, b.top); assertEquals(2560, b.bottom)
    }

    @Test fun aPaneShownAloneTakesEverythingTheOtherNothing() {
        val r = SplitGeometry.rects(2560, 1600, 2, 0.5f, 12, maxed = 1)
        assertTrue(r[0].isEmpty)
        assertEquals(Area(0, 0, 2560, 1600), r[1])
    }

    @Test fun theDividerNeverLeavesAPaneTooSmall() {
        val (a, _) = SplitGeometry.rects(2000, 1000, 2, 0.01f, 0)
        assertEquals(200, a.width())
    }

    @Test fun lettingGoSnapsToThirdsAndTheHalfOrClosesAtTheEdges() {
        assertEquals(SplitGeometry.Release.Snap(0.5f), SplitGeometry.release(0.46f))
        assertEquals(SplitGeometry.Release.Snap(1f / 3f), SplitGeometry.release(0.30f))
        assertEquals(SplitGeometry.Release.Snap(2f / 3f), SplitGeometry.release(0.80f))
        assertEquals(SplitGeometry.Release.Close(0), SplitGeometry.release(0.08f))
        assertEquals(SplitGeometry.Release.Close(1), SplitGeometry.release(0.93f))
        assertEquals(0, SplitGeometry.closing(0.1f))
        assertEquals(-1, SplitGeometry.closing(0.5f))
    }

    @Test fun thePanesAreKeptWhenTheScreenIsRebuilt() {
        val s = SplitGeometry.saveState(listOf("nb-a", "nb-a"), main = 1, active = 0, fraction = 2f / 3f)
        val r = SplitGeometry.readState(s)!!
        assertEquals(listOf("nb-a", "nb-a"), r.ids)
        assertEquals(1, r.main); assertEquals(0, r.active)
        assertEquals(2f / 3f, r.fraction, 1e-6f)
        // Broken or missing: nothing (the notebook opens alone as usual).
        assertNull(SplitGeometry.readState(null))
        assertNull(SplitGeometry.readState("garbage"))
        assertEquals(0, SplitGeometry.readState("nb-a\n7|9|5")!!.main)
    }

    @Test fun areasKnowWhatTheyContain() {
        val a = Area(10, 10, 20, 20)
        assertTrue(a.contains(10, 10)); assertTrue(!a.contains(20, 20)); assertTrue(!Area.NONE.contains(0, 0))
    }
}
