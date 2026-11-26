package dev.n0bridge.n0te

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * Which PDF tiles a page view asks the worker for. The worker drops queued tiles of a view when
 * that view's generation moves on and merges requests for the same tile from two views; dropped
 * tiles get no answer. A tile must never stay "asked for" for good (the page stayed white).
 */
class TileWaitTest {
    private val thumb = "blob/0/0.25/0/0"
    private val sharp = "blob/0/2.0/3/4"

    @Test fun aTileIsAskedForOnceWhileItIsOnItsWay() {
        val w = TileWait()
        assertTrue(w.ask(sharp, 3u, now = 0))
        assertFalse(w.ask(sharp, 3u, now = 100))
        w.arrived(sharp)
        assertTrue("arrived and later dropped from the cache: asked again", w.ask(sharp, 3u, now = 200))
    }

    @Test fun aZoomStepForgetsEveryTileOfThatViewAlsoTheThumbnails() {
        // The worker drops all queued tiles of the view (thumbnails too, they wait behind the
        // sharp tiles): they were never asked again and the page stayed white while scrolling.
        val w = TileWait()
        assertTrue(w.ask(thumb, 3u, now = 0))
        assertTrue(w.ask(sharp, 3u, now = 0))
        w.newGeneration(3u)
        assertTrue(w.ask(thumb, 3u, now = 10))
        assertTrue(w.ask(sharp, 3u, now = 10))
    }

    @Test fun anotherViewsZoomLeavesThisViewsTilesAlone() {
        val w = TileWait()
        assertTrue(w.ask(sharp, 3u, now = 0))
        w.newGeneration(4u)
        assertFalse(w.ask(sharp, 3u, now = 10))
    }

    @Test fun aTileTwoViewsWaitForIsDoneForBoth() {
        // The worker merges both requests into one answer that names only the last view.
        val w = TileWait()
        assertTrue(w.ask(sharp, 3u, now = 0))
        assertTrue(w.ask(sharp, 4u, now = 0))
        w.arrived(sharp)
        assertTrue(w.ask(sharp, 3u, now = 10))
        assertTrue(w.ask(sharp, 4u, now = 10))
    }

    @Test fun aRequestWithoutAnswerIsAskedAgainAfterAWhile() {
        // Merged into the other view's request and dropped with that view's generation.
        val w = TileWait()
        assertTrue(w.ask(sharp, 3u, now = 0))
        assertFalse(w.ask(sharp, 3u, now = TileWait.PENDING_MS - 1))
        assertTrue(w.ask(sharp, 3u, now = TileWait.PENDING_MS + 1))
    }

    @Test fun aRefusedTileWaitsBeforeItIsAskedAgain() {
        val w = TileWait()
        assertTrue(w.ask(sharp, 3u, now = 0))
        w.failed(sharp, now = 50)
        assertFalse(w.ask(sharp, 3u, now = 60))
        assertFalse(w.ask(sharp, 4u, now = 60 + TileWait.RETRY_MS - 20))
        assertTrue(w.ask(sharp, 3u, now = 60 + TileWait.RETRY_MS))
        assertFalse("asked again: on its way", w.ask(sharp, 3u, now = 70 + TileWait.RETRY_MS))
        // Rendered at last: no more waiting for it.
        w.failed(sharp, now = 100 + TileWait.RETRY_MS)
        w.arrived(sharp)
        assertTrue(w.ask(sharp, 3u, now = 110 + TileWait.RETRY_MS))
    }

    @Test fun forgottenRequestsDoNotPileUp() {
        val w = TileWait()
        for (i in 0 until 10_000) w.ask("blob/0/2.0/$i/0", 3u, now = i.toLong())
        w.newGeneration(3u)
        assertTrue(w.size == 0)
    }
}
