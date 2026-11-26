package dev.n0bridge.n0te

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import java.nio.file.Files

class CrashLogTest {
    @Test
    fun reportsAreKeptShownOnceAndPruned() {
        val dir = Files.createTempDirectory("n0te-crash").toFile()
        assertNull(CrashLog.unseen(dir))
        for (i in 0 until 7) CrashLog.save(dir, CrashLog.compose("0.35.0", "Android 15", "main", IllegalStateException("boom $i")), 1000L + i)
        assertEquals(5, dir.listFiles()!!.size)
        val report = CrashLog.unseen(dir)!!
        assertTrue(report.startsWith("n0te 0.35.0 (Android 15)"))
        assertEquals("java.lang.IllegalStateException: boom 6", CrashLog.summary(report))
        assertTrue(report.contains("CrashLogTest"))
        CrashLog.markSeen(dir)
        assertNull(CrashLog.unseen(dir))
        assertEquals(5, dir.listFiles()!!.size)
        CrashLog.save(dir, CrashLog.compose("0.35.0", "x", "io", RuntimeException("again")), 2000L)
        assertEquals("java.lang.RuntimeException: again", CrashLog.summary(CrashLog.unseen(dir)!!))
        dir.deleteRecursively()
    }
}
