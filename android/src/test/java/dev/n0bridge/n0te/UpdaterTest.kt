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
        // Pre-releases only when wanted (with the label from their tag); nothing newer: nothing.
        assertEquals("0.27.1-beta", Updater.pick(releases, "0.26.1", pre = true)!!.version)
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

    @Test fun aPreReleaseIsOlderThanItsReleaseAndLabelsCount() {
        // A beta tester on 0.35.0-beta.1 was never offered 0.35.0 (same numbers: "not newer").
        assertTrue(Updater.newer("0.35.0", "0.35.0-beta.1"))
        assertFalse(Updater.newer("0.35.0-beta.1", "0.35.0"))
        assertTrue(Updater.newer("0.35.0-beta.2", "0.35.0-beta.1"))
        assertTrue("numbers in a label count as numbers", Updater.newer("0.35.0-beta.10", "0.35.0-beta.9"))
        assertTrue(Updater.newer("0.35.0-rc.1", "0.35.0-beta.3"))
        assertTrue(Updater.newer("0.35.0-beta.1", "0.34.9"))
        assertFalse(Updater.newer("0.35.0-beta.1", "0.35.0-beta.1"))
        assertFalse(Updater.newer("0.34.1", "0.34.1"))
    }

    @Test fun aBetaTesterGetsTheReleaseAndTheNextBeta() {
        val json = """
        [
          {"tag_name": "v0.35.0-beta.2", "name": "", "draft": false, "prerelease": true,
           "assets": [{"name": "n0te-0.35.0-beta.2-android.apk", "browser_download_url": "https://x/b2", "url": "", "size": 1}]},
          {"tag_name": "v0.35.0-beta.1", "name": "", "draft": false, "prerelease": true,
           "assets": [{"name": "n0te-0.35.0-beta.1-android.apk", "browser_download_url": "https://x/b1", "url": "", "size": 1}]}
        ]
        """.trimIndent()
        val b = Updater.pick(json, "0.35.0-beta.1", pre = true)!!
        assertEquals("0.35.0-beta.2", b.version)
        assertEquals("https://x/b2", b.url)
        assertNull(Updater.pick(json, "0.35.0-beta.2", pre = true))
        // The release itself (newer than every beta of it); its asset name carries no label.
        val withRelease = json.replaceFirst("[", """[{"tag_name": "v0.35.0", "name": "", "draft": false, "prerelease": false,
           "assets": [{"name": "n0te-0.35.0-android.apk", "browser_download_url": "https://x/r", "url": "", "size": 1}]},""")
        assertEquals("0.35.0", Updater.pick(withRelease, "0.35.0-beta.2", pre = true)!!.version)
        assertEquals("0.35.0", Updater.pick(withRelease, "0.35.0-beta.1", pre = false)!!.version)
    }

    @Test fun theJsonReaderReadsGitHubsAnswers() {
        val v = MiniJson.parse("""{"a": [1, -2.5, true, null, "x\"y\\zü"], "b": {}}""") as Map<*, *>
        assertEquals(listOf(1L, -2.5, true, null, "x\"y\\zü"), v["a"])
        assertEquals(emptyMap<String, Any?>(), v["b"])
    }

    @Test fun releaseNotesBecomeWholeItemsUnderGermanHeadings() {
        val md = "### Added\n\n- Export: pick single pages, even a single side page (\"03 R1\").\n- Graphs: stretch an axis\n  right on the page.\n\n### Fixed\n\n- `a = 3b` when **editing** a row."
        val s = Updater.noteSections(md)
        assertEquals(listOf("Neu", "Behoben"), s.map { it.first })
        assertEquals(listOf("Export: pick single pages, even a single side page (\"03 R1\").", "Graphs: stretch an axis right on the page."), s[0].second)
        assertEquals(listOf("a = 3b when editing a row."), s[1].second)
        assertEquals(listOf("" to listOf("Just text")), Updater.noteSections("Just text\n"))
        assertTrue(Updater.noteSections("").isEmpty())
    }
}
