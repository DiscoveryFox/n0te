package dev.n0bridge.n0te

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

class ChangelogTest {
    private fun parts(v: String) = v.split('.').map { it.toInt() }
    private fun newer(a: String, b: String): Boolean {
        val (x, y) = parts(a) to parts(b)
        for (i in 0 until maxOf(x.size, y.size)) {
            val d = x.getOrElse(i) { 0 } - y.getOrElse(i) { 0 }
            if (d != 0) return d > 0
        }
        return false
    }

    @Test
    fun newestFirstNoDuplicatesNothingEmpty() {
        val all = Changelog.all
        all.zipWithNext().forEach { (a, b) -> assertTrue("${a.version} must come before ${b.version}", newer(a.version, b.version)) }
        all.forEach { assertTrue("${it.version} has changes", it.items.isNotEmpty() && it.title.isNotBlank()) }
    }

    /** The version being built has its entry on top (else the "new" dialog shows an old one). */
    @Test
    fun theAppsVersionIsTheNewestEntry() {
        val gradle = File("build.gradle.kts").takeIf { it.exists() } ?: File("n0te/build.gradle.kts")
        val version = Regex("versionName = \"([^\"]+)\"").find(gradle.readText())!!.groupValues[1]
        assertEquals(version, Changelog.all.first().version)
    }
}
