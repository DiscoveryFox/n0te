package dev.n0bridge.n0te

import android.app.Activity
import android.app.PendingIntent
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.pm.PackageInstaller
import android.content.pm.PackageManager
import android.os.Build
import android.util.Log
import android.widget.Toast
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import java.io.File
import java.io.InputStream
import java.net.HttpURLConnection
import java.net.URL
import java.security.MessageDigest
import java.util.concurrent.Executors

/**
 * n0te updates from GitHub releases: looks for a newer n0te APK (when the app opens, every few
 * hours, or by hand), downloads it in the background, and installs it on a tap (Android asks
 * once more). The repo is n0te's own ([DEFAULT_REPO]) unless the settings name another; an
 * empty field there switches updates off.
 */
object Updater {
    /** Where n0te's releases are: every install updates from here without any setup. */
    const val DEFAULT_REPO = "DiscoveryFox/n0te"
    /** A release with an n0te APK. */
    data class Release(
        val version: String, val tag: String, val title: String, val notes: String,
        val asset: String, val url: String, val apiUrl: String, val size: Long, val sha256: String?,
    )

    /**
     * Release notes (GitHub Markdown) as (heading, items) for the update dialog: "### Added"
     * starts a section (in German), every "- " line is an item, and its wrapped lines join it.
     */
    fun noteSections(md: String): List<Pair<String, List<String>>> {
        val out = ArrayList<Pair<String, MutableList<String>>>()
        fun items(): MutableList<String> {
            if (out.isEmpty()) out.add("" to mutableListOf())
            return out.last().second
        }
        var blank = true
        for (raw in md.lines()) {
            val l = raw.trim()
            when {
                l.isEmpty() -> { blank = true; continue }
                l.startsWith("#") -> out.add(heading(l.trimStart('#').trim()) to mutableListOf())
                l.startsWith("- ") || l.startsWith("* ") -> items().add(l.substring(2).trim())
                else -> {
                    val list = items()
                    if (blank || list.isEmpty()) list.add(l) else list[list.size - 1] = list.last() + " " + l
                }
            }
            blank = false
        }
        return out.filter { it.second.isNotEmpty() }.map { (h, list) -> h to list.map { it.replace("**", "").replace("`", "") } }
    }

    private fun heading(h: String) = when (h.lowercase()) {
        "added" -> "Neu"
        "changed" -> "Geändert"
        "fixed" -> "Behoben"
        "removed" -> "Entfernt"
        else -> h
    }

    sealed class State {
        object Idle : State()
        object Checking : State()
        data class UpToDate(val version: String) : State()
        data class Downloading(val release: Release, val done: Long, val total: Long) : State()
        data class Ready(val release: Release, val file: File) : State()
        data class Installing(val release: Release) : State()
        data class Failed(val message: String) : State()
    }

    var state by mutableStateOf<State>(State.Idle)
        private set

    /** The ready version the start screen asked about (Später: not again until the app starts anew). */
    var asked: String? = null

    private val REPO = Regex("^[\\w.-]+/[\\w.-]+$")
    val repo: String get() = Settings.str("update_repo", DEFAULT_REPO).trim().removePrefix("https://github.com/").trimEnd('/')
    val token: String get() = Settings.str("update_token", "").trim()
    /** "open": when the app opens; "periodic": also every 6 hours while it runs; "manual". */
    val mode: String get() = Settings.str("update_mode", "open")
    val prerelease: Boolean get() = Settings.str("update_pre", "0") == "1"
    fun configured() = REPO.matches(repo)

    private const val HOUR = 3_600_000L
    private val net = Executors.newSingleThreadExecutor { r -> Thread(r, "n0te-update").apply { isDaemon = true } }
    private var busy = false

    // --- the pure part (JVM-tested) ---------------------------------------------------------

    /** The version in a tag or file name: "n0te-v0.27.0", "v0.27", "n0te-v0.27.1-release.apk". */
    fun versionIn(s: String): List<Int>? =
        Regex("(\\d+)\\.(\\d+)(?:\\.(\\d+))?").find(s)?.groupValues?.drop(1)?.map { it.ifEmpty { "0" }.toInt() }

    /** Whether version [a] is newer than [b] ("0.27.0" > "0.26.1"). */
    fun newer(a: String, b: String): Boolean {
        val x = versionIn(a) ?: return false
        val y = versionIn(b) ?: return true
        for (i in 0 until maxOf(x.size, y.size)) {
            val d = x.getOrElse(i) { 0 } - y.getOrElse(i) { 0 }
            if (d != 0) return d > 0
        }
        return false
    }

    /**
     * The newest release newer than [current] that has an n0te APK, from the GitHub API's list
     * of releases (JSON). Drafts never, pre-releases only with [pre]. A release of the repo may
     * carry other apps too: an APK named n0te… wins; emulator builds never.
     */
    fun pick(json: String, current: String, pre: Boolean): Release? {
        val list = MiniJson.parse(json) as? List<*> ?: return null
        var best: Release? = null
        for (r in list) {
            val m = r as? Map<*, *> ?: continue
            if (m["draft"] == true || (m["prerelease"] == true && !pre)) continue
            val tag = m["tag_name"] as? String ?: ""
            val apks = (m["assets"] as? List<*>).orEmpty().mapNotNull { it as? Map<*, *> }.filter {
                val n = (it["name"] as? String).orEmpty().lowercase()
                n.endsWith(".apk") && "emu" !in n && "x86" !in n
            }
            val apk = apks.firstOrNull { (it["name"] as String).lowercase().contains("n0te") } ?: apks.singleOrNull() ?: continue
            val name = apk["name"] as String
            val v = versionIn(name) ?: versionIn(tag) ?: continue
            val version = v.joinToString(".")
            if (!newer(version, current) || (best != null && !newer(version, best.version))) continue
            best = Release(
                version = version, tag = tag, title = (m["name"] as? String)?.takeIf { it.isNotBlank() } ?: tag,
                notes = (m["body"] as? String).orEmpty().trim(), asset = name,
                url = apk["browser_download_url"] as? String ?: continue, apiUrl = apk["url"] as? String ?: "",
                size = (apk["size"] as? Number)?.toLong() ?: -1L,
                sha256 = (apk["digest"] as? String)?.takeIf { it.startsWith("sha256:") }?.removePrefix("sha256:")?.lowercase(),
            )
        }
        return best
    }

    // --- looking, downloading -----------------------------------------------------------------

    /** The installed version ("0.26.1"). */
    fun current(ctx: Context): String = runCatching { ctx.packageManager.getPackageInfo(ctx.packageName, 0).versionName }.getOrNull() ?: "0"

    /** At the start of the app: a download from before is ready again; every half hour a look if set so. */
    fun start(ctx: Context) {
        val app = ctx.applicationContext
        restore(app)
        N0te.main.postDelayed(object : Runnable {
            override fun run() {
                if (mode == "periodic") maybeCheck(app)
                N0te.main.postDelayed(this, HOUR / 2)
            }
        }, HOUR / 2)
    }

    /** When the app opens (and from [start]): looks, not more often than the mode allows. */
    fun maybeCheck(ctx: Context) {
        if (!configured() || mode == "manual" || state is State.Ready || busy) return
        val last = Settings.str("update_last", "0").toLongOrNull() ?: 0L
        // Without a token GitHub answers 60 times an hour per address: once an hour is plenty.
        val every = if (mode == "periodic") 6 * HOUR else HOUR
        if (System.currentTimeMillis() - last < every) return
        check(ctx, manual = false)
    }

    /** Looks for a newer release now; a new one is downloaded at once. */
    fun check(ctx: Context, manual: Boolean) {
        val app = ctx.applicationContext
        if (!configured()) {
            if (manual) state = State.Failed("Erst das GitHub-Repo eintragen (besitzer/name)")
            return
        }
        if (busy) return
        if (state is State.Ready && !manual) return
        busy = true
        state = State.Checking
        Settings.set("update_last", System.currentTimeMillis().toString())
        val repo = repo; val pre = prerelease; val now = current(app)
        net.execute {
            val r = runCatching {
                val c = open("https://api.github.com/repos/$repo/releases?per_page=30", "application/vnd.github+json")
                val json = c.inputStream.use { it.readBytes().decodeToString() }
                c.disconnect()
                pick(json, now, pre)
            }
            N0te.main.post {
                r.fold({ rel ->
                    if (rel == null) { state = State.UpToDate(now); busy = false } else download(app, rel)
                }, { e ->
                    Log.w(N0te.TAG, "update check failed", e)
                    state = State.Failed(e.message ?: "Keine Verbindung zu GitHub")
                    busy = false
                })
            }
        }
    }

    private fun dir(ctx: Context) = File(ctx.cacheDir, "updates").apply { mkdirs() }

    private fun download(app: Context, r: Release) {
        val file = File(dir(app), "n0te-${r.version}.apk")
        keepFound(r)
        // Downloaded before: ready at once.
        if (complete(file, r)) { state = State.Ready(r, file); busy = false; return }
        dir(app).listFiles()?.forEach { if (it != file) it.delete() }
        state = State.Downloading(r, 0, r.size)
        net.execute {
            val res = runCatching {
                val part = File(file.path + ".part")
                val c = if (token.isNotEmpty() && r.apiUrl.isNotEmpty()) open(r.apiUrl, "application/octet-stream") else open(r.url, "application/octet-stream")
                val total = c.contentLengthLong.takeIf { it > 0 } ?: r.size
                c.inputStream.use { input -> part.outputStream().use { out -> copy(input, out, r, total) } }
                c.disconnect()
                if (r.size > 0 && part.length() != r.size) error("Download unvollständig (${part.length()} von ${r.size} Bytes)")
                if (r.sha256 != null && sha256(part) != r.sha256) { part.delete(); error("Die Prüfsumme stimmt nicht: Download verworfen") }
                if (!part.renameTo(file)) error("Konnte das Update nicht ablegen")
            }
            N0te.main.post {
                res.fold({ state = State.Ready(r, file) }, { e -> Log.w(N0te.TAG, "update download failed", e); state = State.Failed(e.message ?: "Download fehlgeschlagen") })
                busy = false
            }
        }
    }

    private fun copy(input: InputStream, out: java.io.OutputStream, r: Release, total: Long) {
        val buf = ByteArray(64 * 1024)
        var done = 0L
        var shown = 0L
        while (true) {
            val n = input.read(buf)
            if (n < 0) break
            out.write(buf, 0, n)
            done += n
            if (done - shown > 256 * 1024) {
                shown = done
                val d = done
                N0te.main.post { state = State.Downloading(r, d, total) }
            }
        }
    }

    private fun complete(file: File, r: Release) = file.isFile && (r.size <= 0 || file.length() == r.size) && (r.sha256 == null || sha256(file) == r.sha256)

    private fun sha256(f: File): String {
        val md = MessageDigest.getInstance("SHA-256")
        f.inputStream().use { input -> val buf = ByteArray(64 * 1024); while (true) { val n = input.read(buf); if (n < 0) break; md.update(buf, 0, n) } }
        return md.digest().joinToString("") { "%02x".format(it) }
    }

    /**
     * A GitHub request: our own redirects (the token goes only to GitHub, never to the storage
     * a download is sent on to), a short German reason when it fails.
     */
    private fun open(url: String, accept: String): HttpURLConnection {
        var u = URL(url)
        repeat(6) {
            val c = u.openConnection() as HttpURLConnection
            c.instanceFollowRedirects = false
            c.connectTimeout = 15_000
            c.readTimeout = 30_000
            c.setRequestProperty("Accept", accept)
            c.setRequestProperty("User-Agent", "n0te-updater")
            if (token.isNotEmpty() && (u.host == "api.github.com" || u.host == "github.com")) c.setRequestProperty("Authorization", "Bearer $token")
            val code = c.responseCode
            when {
                code in 300..399 -> {
                    val to = c.getHeaderField("Location") ?: error("GitHub leitet ins Leere weiter")
                    c.disconnect()
                    u = URL(u, to)
                }
                code == 404 -> error(if (token.isEmpty()) "Repo nicht gefunden (ein privates Repo braucht einen Token)" else "Repo oder Datei nicht gefunden")
                code == 401 -> error("Der Token gilt nicht (mehr)")
                code == 403 || code == 429 -> error("GitHub lässt gerade nicht mehr Abfragen zu, später nochmal")
                code !in 200..299 -> error("GitHub antwortet mit $code")
                else -> return c
            }
        }
        error("Zu viele Weiterleitungen")
    }

    /** The release found last, so a download is still known after the app restarts. */
    private fun keepFound(r: Release) {
        val sep = '\u001f'
        Settings.set("update_found", listOf(r.version, r.tag, r.title, r.notes, r.asset, r.url, r.apiUrl, r.size.toString(), r.sha256 ?: "").joinToString(sep.toString()))
    }

    private fun restore(app: Context) {
        val f = Settings.str("update_found", "").split('\u001f')
        val now = current(app)
        if (f.size == 9 && newer(f[0], now)) {
            val r = Release(f[0], f[1], f[2], f[3], f[4], f[5], f[6], f[7].toLongOrNull() ?: -1L, f[8].ifEmpty { null })
            val file = File(dir(app), "n0te-${r.version}.apk")
            if (complete(file, r)) { state = State.Ready(r, file); return }
        }
        // Installed already (or gone): away with what is left.
        dir(app).listFiles()?.forEach { it.delete() }
    }

    // --- installing ---------------------------------------------------------------------------

    /** Installs the downloaded update (Android shows its own confirmation). */
    fun install(activity: Activity) {
        val s = state as? State.Ready ?: return
        val pm = activity.packageManager
        if (!pm.canRequestPackageInstalls()) {
            Toast.makeText(activity, "n0te braucht einmal die Erlaubnis, Apps zu installieren: einschalten, dann zurück und nochmal tippen", Toast.LENGTH_LONG).show()
            activity.startActivity(Intent(android.provider.Settings.ACTION_MANAGE_UNKNOWN_APP_SOURCES, android.net.Uri.parse("package:${activity.packageName}")))
            return
        }
        checkApk(activity, s.file)?.let { state = State.Failed(it); return }
        state = State.Installing(s.release)
        val app = activity.applicationContext
        net.execute {
            val r = runCatching {
                val installer = app.packageManager.packageInstaller
                val params = PackageInstaller.SessionParams(PackageInstaller.SessionParams.MODE_FULL_INSTALL).apply {
                    setAppPackageName(app.packageName)
                    if (Build.VERSION.SDK_INT >= 31) setRequireUserAction(PackageInstaller.SessionParams.USER_ACTION_NOT_REQUIRED)
                }
                val id = installer.createSession(params)
                installer.openSession(id).use { session ->
                    session.openWrite("n0te.apk", 0, s.file.length()).use { out -> s.file.inputStream().use { it.copyTo(out) }; session.fsync(out) }
                    val back = Intent(app, UpdateReceiver::class.java).setAction(UpdateReceiver.ACTION)
                    val flags = PendingIntent.FLAG_UPDATE_CURRENT or (if (Build.VERSION.SDK_INT >= 31) PendingIntent.FLAG_MUTABLE else 0)
                    session.commit(PendingIntent.getBroadcast(app, id, back, flags).intentSender)
                }
            }
            r.exceptionOrNull()?.let { e -> Log.w(N0te.TAG, "update install failed", e); N0te.main.post { state = State.Failed(e.message ?: "Installation fehlgeschlagen") } }
        }
    }

    /** What is wrong with a downloaded APK (null: fine): another app, not newer, another key. */
    private fun checkApk(ctx: Context, file: File): String? {
        val pm = ctx.packageManager
        val apk = pm.getPackageArchiveInfo(file.path, PackageManager.GET_SIGNING_CERTIFICATES) ?: return "Die heruntergeladene Datei ist keine App"
        if (apk.packageName != ctx.packageName) return "Das ist nicht n0te (${apk.packageName})"
        val me = pm.getPackageInfo(ctx.packageName, PackageManager.GET_SIGNING_CERTIFICATES)
        if (apk.longVersionCode <= me.longVersionCode) return "Version ${apk.versionName} ist nicht neuer als die installierte"
        val a = apk.signingInfo?.apkContentsSigners?.toSet()
        val b = me.signingInfo?.apkContentsSigners?.toSet()
        if (a != null && b != null && a != b) return "Das Update ist mit einem anderen Schlüssel signiert: Android kann n0te damit nicht aktualisieren"
        return null
    }

    /** From [UpdateReceiver]: Android's answer. Cancelled: the update stays ready. */
    internal fun answer(ctx: Context, intent: Intent) {
        val status = intent.getIntExtra(PackageInstaller.EXTRA_STATUS, -999)
        when (status) {
            PackageInstaller.STATUS_PENDING_USER_ACTION -> {
                val confirm = if (Build.VERSION.SDK_INT >= 33) intent.getParcelableExtra(Intent.EXTRA_INTENT, Intent::class.java)
                else @Suppress("DEPRECATION") intent.getParcelableExtra(Intent.EXTRA_INTENT)
                confirm?.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)?.let { runCatching { ctx.startActivity(it) } }
            }
            PackageInstaller.STATUS_SUCCESS -> Log.i(N0te.TAG, "update installed")
            else -> N0te.main.post {
                val ready = (state as? State.Installing)?.release?.let { r -> File(dir(ctx), "n0te-${r.version}.apk").takeIf { it.isFile }?.let { State.Ready(r, it) } }
                state = if (status == PackageInstaller.STATUS_FAILURE_ABORTED && ready != null) ready
                else State.Failed(intent.getStringExtra(PackageInstaller.EXTRA_STATUS_MESSAGE) ?: "Installation fehlgeschlagen ($status)")
            }
        }
    }
}

/** Android's answers to an update install (confirm, done, failed) come here. */
class UpdateReceiver : BroadcastReceiver() {
    companion object { const val ACTION = "dev.n0bridge.n0te.UPDATE_STATUS" }
    override fun onReceive(ctx: Context, intent: Intent) = Updater.answer(ctx, intent)
}

/** A small JSON reader (the GitHub API's answers; Android's org.json isn't there in JVM tests). */
internal object MiniJson {
    fun parse(s: String): Any? = P(s).run { val v = value(); space(); v }

    private class P(val s: String) {
        var i = 0
        fun space() { while (i < s.length && s[i].isWhitespace()) i++ }
        fun value(): Any? {
            space()
            if (i >= s.length) error("JSON zu Ende")
            return when (val c = s[i]) {
                '{' -> obj()
                '[' -> arr()
                '"' -> str()
                't' -> word("true", true)
                'f' -> word("false", false)
                'n' -> word("null", null)
                else -> if (c == '-' || c.isDigit()) num() else error("JSON: unerwartetes $c")
            }
        }
        fun word(w: String, v: Any?): Any? { if (!s.startsWith(w, i)) error("JSON: $w erwartet"); i += w.length; return v }
        fun num(): Number {
            val st = i
            while (i < s.length && (s[i].isDigit() || s[i] in "+-.eE")) i++
            val t = s.substring(st, i)
            return t.toLongOrNull() ?: t.toDouble()
        }
        fun str(): String {
            i++
            val b = StringBuilder()
            while (true) {
                if (i >= s.length) error("JSON: Text ohne Ende")
                when (val c = s[i++]) {
                    '"' -> return b.toString()
                    '\\' -> when (val e = s[i++]) {
                        'n' -> b.append('\n'); 't' -> b.append('\t'); 'r' -> b.append('\r'); 'b' -> b.append('\b'); 'f' -> b.append('\u000c')
                        'u' -> { b.append(s.substring(i, i + 4).toInt(16).toChar()); i += 4 }
                        else -> b.append(e)
                    }
                    else -> b.append(c)
                }
            }
        }
        fun arr(): List<Any?> {
            i++
            val out = ArrayList<Any?>()
            space()
            if (s[i] == ']') { i++; return out }
            while (true) {
                out.add(value()); space()
                when (s[i++]) { ',' -> continue; ']' -> return out; else -> error("JSON: , oder ] erwartet") }
            }
        }
        fun obj(): Map<String, Any?> {
            i++
            val out = LinkedHashMap<String, Any?>()
            space()
            if (s[i] == '}') { i++; return out }
            while (true) {
                space()
                val k = str(); space()
                if (s[i++] != ':') error("JSON: : erwartet")
                out[k] = value(); space()
                when (s[i++]) { ',' -> continue; '}' -> return out; else -> error("JSON: , oder } erwartet") }
            }
        }
    }
}
