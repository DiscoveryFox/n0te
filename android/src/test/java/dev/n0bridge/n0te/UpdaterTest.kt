package dev.n0bridge.n0te

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** The updater's choice among GitHub releases (a fixed answer, no network). */
class UpdaterTest {
    private val releases = """
    [
      {"tag_name": "n0te-v0.28.0", "name": "Entwurf", "draft": true, "prerelease": false,
       "assets": [{"name": "n0te-v0.28.0-release.apk", "browser_download_url": "https://x/d", "url": "https://api/d", "size": 1}]},
      {"tag_name": "n0te-v0.27.1-beta", "name": "Beta", "draft": false, "prerelease": true,
       "assets": [{"name": "n0te-v0.27.1-release.apk", "browser_download_url": "https://x/b", "url": "https://api/b", "size": 2}]},
      {"tag_name": "v0.15.0", "name": "n0bridge 0.15", "draft": false, "prerelease": false,
       "assets": [{"name": "n0bridge-app-release.apk", "browser_download_url": "https://x/n", "url": "https://api/n", "size": 3},
                  {"name": "n0te-v0.27.0-release.apk", "browser_download_url": "https://x/n0te", "url": "https://api/n0te", "size": 17860034,
                   "digest": "sha256:ABCDEF"},
                  {"name": "n0te-v0.27.0-emu.apk", "browser_download_url": "https://x/emu", "url": "https://api/emu", "size": 5}],
       "body": "## Neu\n- Graphen \"größer\"\n- Tastatur ä"},
      {"tag_name": "n0te-v0.26.0", "name": "", "draft": false, "prerelease": false,
       "assets": [{"name": "n0te.apk", "browser_download_url": "https://x/old", "url": "https://api/old", "size": 4}]}
    ]
    """.trimIndent()

    @Test fun theNewestN0teApkWinsDraftsAndOtherAppsDont() {
        val r = Updater.pick(releases, "0.26.1", pre = false)!!
        assertEquals("0.27.0", r.version)
        assertEquals("n0te-v0.27.0-release.apk", r.asset)
        assertEquals("https://x/n0te", r.url)
        assertEquals(17860034L, r.size)
        assertEquals("abcdef", r.sha256)
        assertEquals("n0bridge 0.15", r.title)
        assertTrue(r.notes.contains("Graphen \"größer\"") && r.notes.contains("Tastatur ä"))
        // Pre-releases only when wanted; nothing newer: nothing.
        assertEquals("0.27.1", Updater.pick(releases, "0.26.1", pre = true)!!.version)
        assertNull(Updater.pick(releases, "0.27.0", pre = false))
        assertNull(Updater.pick("[]", "0.1.0", pre = true))
    }

    @Test fun versionsCompareByNumbers() {
        assertTrue(Updater.newer("0.27.0", "0.26.1"))
        assertTrue(Updater.newer("0.10.0", "0.9.9"))
        assertTrue(Updater.newer("1.0", "0.99.99"))
        assertFalse(Updater.newer("0.26.1", "0.26.1"))
        assertFalse(Updater.newer("0.26", "0.26.0"))
        assertEquals(listOf(0, 27, 1), Updater.versionIn("n0te-v0.27.1-release.apk"))
        assertNull(Updater.versionIn("n0te.apk"))
    }

    @Test fun theJsonReaderReadsGitHubsAnswers() {
        val v = MiniJson.parse("""{"a": [1, -2.5, true, null, "x\"y\\zü"], "b": {}}""") as Map<*, *>
        assertEquals(listOf(1L, -2.5, true, null, "x\"y\\zü"), v["a"])
        assertEquals(emptyMap<String, Any?>(), v["b"])
    }
}
