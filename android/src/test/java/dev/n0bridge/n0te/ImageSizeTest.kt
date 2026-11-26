package dev.n0bridge.n0te

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/** How big pictures are decoded: never past the limit (the image cache thrashed with three photos). */
class ImageSizeTest {
    @Test fun aStoredPhotoIsShownAtMostAtTheLimit() {
        // Stored pictures are up to 3200 px; shown at most 2400. It used to decode all 3200
        // (31 MB a photo): three on a page pushed each other out of the 64 MB cache every frame.
        assertEquals(2400 to 1800, ImageSize.fit(3200, 2400, 2400))
        assertEquals(1800 to 2400, ImageSize.fit(2400, 3200, 2400))
        assertEquals("small ones stay", 800 to 600, ImageSize.fit(800, 600, 2400))
        assertEquals("never 0", 2400 to 1, ImageSize.fit(100_000, 10, 2400))
    }

    @Test fun theSampleIsAPowerOfTwoThatKeepsEnoughPixels() {
        for (long in listOf(1, 2399, 2400, 3200, 4799, 4800, 6399, 12000, 50_000)) {
            val s = ImageSize.sample(long, 2400)
            assertTrue("$long: $s", s >= 1 && s and (s - 1) == 0)
            // Decoded at 1/s it still has the limit's pixels (sharp after scaling down) ...
            assertTrue("$long: $s too coarse", long < 2400 || long / s >= 2400)
            // ... and is less than twice the limit (memory).
            assertTrue("$long: $s too fine", long / s < 2 * 2400)
        }
    }

    @Test fun aHugePhotoIsDecodedSmallBeforeItIsShrunk() {
        // 108 MP: decoded at 1/2 (6000 px) for a 3200 px picture, not at full size.
        assertEquals(2, ImageSize.sample(12000, 3200))
        assertEquals(3200 to 2400, ImageSize.fit(12000 / 2, 9000 / 2, 3200))
    }
}
