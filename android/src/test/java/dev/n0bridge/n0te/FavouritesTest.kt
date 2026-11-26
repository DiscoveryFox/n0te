package dev.n0bridge.n0te

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class FavouritesTest {
    @Test fun aFavouriteSurvivesBeingStored() {
        val f = Tools.Favourite(marker = false, pen = PenType.FOUNTAIN, color = 0xFFD71921.toInt(), width = 1.6f)
        assertEquals(f, Tools.Favourite.decode(f.encode()))
        val m = Tools.Favourite(marker = true, pen = PenType.BRUSH, color = 0xFF30D158.toInt(), width = 3f)
        assertEquals(m, Tools.Favourite.decode(m.encode()))
    }

    @Test fun brokenEntriesAreSkippedAndWidthsStayInRange() {
        assertNull(Tools.Favourite.decode("P|NOPE|1|1"))
        assertNull(Tools.Favourite.decode("garbage"))
        assertNull(Tools.Favourite.decode("P|FOUNTAIN|x|1"))
        assertEquals(Tools.MAX_W, Tools.Favourite.decode("P|FOUNTAIN|4278190080|99")!!.width)
    }
}
