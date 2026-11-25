package dev.n0bridge.n0te

import android.content.Context
import android.content.Intent
import android.util.Log
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import uniffi.n0te_ffi.FfiBrush
import uniffi.n0te_ffi.FfiDevice
import uniffi.n0te_ffi.FfiSyncEvent
import uniffi.n0te_ffi.SyncEngine
import uniffi.n0te_ffi.SyncListener
import java.util.concurrent.CopyOnWriteArrayList

/**
 * The sync engine (peer to peer over iroh, inside the app): keeps notebooks the same as on
 * the paired laptop, delivers live events (presence, strokes being drawn, open/follow
 * requests) to whoever listens, on the main thread.
 */
object SyncHub : SyncListener {
    /** The running engine (Compose state: the settings card follows it). */
    var engine by mutableStateOf<SyncEngine?>(null)
        private set
    /** Why the engine didn't start (shown in the settings, retried by itself). */
    var startError by mutableStateOf<String?>(null)
        private set
    @Volatile private var starting = false
    private var attempts = 0
    /** Paired devices (Compose state: the LAPTOP chip and the settings follow it). */
    var devices by mutableStateOf<List<FfiDevice>>(emptyList())
        private set
    /** The paired laptop (first paired device), connected or not. */
    val laptop: FfiDevice? get() = devices.firstOrNull { it.online } ?: devices.firstOrNull()
    /** A paired device is connected right now. */
    val online: Boolean get() = devices.any { it.online }
    /** The name this engine was started with (Settings.displayName then). */
    var startedName = ""
        private set
    /** Our guests connected right now (all notebooks); Compose state. */
    var guests by mutableStateOf<List<uniffi.n0te_ffi.FfiGuest>>(emptyList())
        private set
    /** A summon for a notebook that wasn't open: it opens, then jumps there. */
    @Volatile private var pendingSummon: FfiSyncEvent.Summoned? = null
    fun takeSummon(notebook: String): FfiSyncEvent.Summoned? = pendingSummon?.takeIf { it.notebook == notebook }?.also { pendingSummon = null }

    /** New guests waiting to be let in (invites with approval); Compose state. */
    var waiting by mutableStateOf<List<uniffi.n0te_ffi.FfiGuest>>(emptyList())
        private set
    /** Notebooks we joined as a guest (with our rights and whether the owner is there). */
    var joined by mutableStateOf<List<uniffi.n0te_ffi.FfiJoined>>(emptyList())
        private set

    /** Who a device id is: own device, guest, or the owner of a joined notebook. */
    fun nameOf(device: String): String =
        devices.firstOrNull { it.id == device }?.name ?: guests.firstOrNull { it.device == device }?.name ?: joined.firstOrNull { it.owner == device }?.ownerName ?: "Gast"
    fun isGuest(device: String) = guests.any { it.device == device }
    /** Our rights in a joined notebook: null when it's ours. */
    fun joinedRole(notebook: String): uniffi.n0te_ffi.FfiJoined? = joined.firstOrNull { it.notebook == notebook && !it.revoked }

    fun refreshGuests() {
        val e = engine ?: return
        N0te.io.execute {
            val st = runCatching { e.status() }.getOrNull()
            N0te.main.post { if (st != null) { guests = st.guests; joined = st.joined; waiting = st.waiting } }
        }
    }

    fun setShareView(on: Boolean) {
        Settings.set("share_view", on)
        val e = engine ?: return
        N0te.io.execute { runCatching { e.setShareView(on) } }
    }
    /** The screen in front (gets live events); the library or an open notebook. */
    private val listeners = CopyOnWriteArrayList<(FfiSyncEvent) -> Unit>()
    /** The notebook open right now (an open request for another one starts it). */
    @Volatile var openNotebook: String? = null
    /** Every notebook on screen (two side by side); [openNotebook] is the main one. */
    @Volatile var openNotebooks: Set<String> = emptySet()
    /**
     * The notebook screen in front, which can open a notebook asked for from outside (the
     * laptop's LIVE, a summon) in its main pane: true when it did. Main thread.
     */
    var openHere: ((String, String?) -> Boolean)? = null
    private lateinit var app: Context

    /** The activity in front (for screenshot requests from the laptop). */
    @Volatile private var front: java.lang.ref.WeakReference<android.app.Activity>? = null

    fun start(ctx: Context) {
        app = ctx.applicationContext
        (app as? android.app.Application)?.registerActivityLifecycleCallbacks(object : android.app.Application.ActivityLifecycleCallbacks {
            override fun onActivityResumed(a: android.app.Activity) { front = java.lang.ref.WeakReference(a) }
            override fun onActivityPaused(a: android.app.Activity) { if (front?.get() === a) front = null }
            override fun onActivityCreated(a: android.app.Activity, b: android.os.Bundle?) {}
            override fun onActivityStarted(a: android.app.Activity) {}
            override fun onActivityStopped(a: android.app.Activity) {}
            override fun onActivitySaveInstanceState(a: android.app.Activity, b: android.os.Bundle) {}
            override fun onActivityDestroyed(a: android.app.Activity) {}
        })
        NativeContext.init(app)
        tryStart()
    }

    /** Starts the engine; on failure tries again after 5, 15, 30, then every 60 s. */
    private fun tryStart() {
        if (engine != null || starting) return
        starting = true
        val attempt = attempts++
        // Something hanging shows instead of a silent "starting".
        N0te.main.postDelayed({ if (starting && engine == null) startError = "Der Start dauert ungewöhnlich lange …" }, 20_000)
        Thread({
            val r = runCatching { SyncEngine.start(N0te.root.path, Settings.displayName.also { startedName = it }, this) }
            N0te.main.post {
                starting = false
                r.onSuccess { engine = it; startError = null; attempts = 0; refreshDevices(); refreshGuests(); N0te.io.execute { runCatching { it.setShareView(Settings.shareView) } } }
                    .onFailure { e ->
                        Log.w(N0te.TAG, "sync engine: ${e.message}")
                        startError = e.message ?: e.toString()
                        N0te.main.postDelayed({ tryStart() }, longArrayOf(5_000, 15_000, 30_000, 60_000)[minOf(attempt, 3)])
                    }
            }
        }, "n0te-sync-start").start()
    }

    /** "Neu versuchen" in the settings. */
    fun retryStart() {
        attempts = 0
        tryStart()
    }

    fun refreshDevices() {
        val e = engine ?: return
        N0te.io.execute {
            val list = runCatching { e.devices() }.getOrDefault(emptyList())
            N0te.main.post { devices = list }
        }
    }

    fun listen(l: (FfiSyncEvent) -> Unit) = listeners.add(l)
    fun unlisten(l: (FfiSyncEvent) -> Unit) = listeners.remove(l)

    override fun onEvent(event: FfiSyncEvent) {
        N0te.main.post {
            when (event) {
                is FfiSyncEvent.DeviceOnline -> {
                    // Shown at once; the refresh below fills in the rest.
                    devices = devices.filter { it.id != event.device.id } + event.device
                    refreshDevices()
                }
                is FfiSyncEvent.DeviceOffline -> {
                    devices = devices.map { if (it.id == event.id) it.copy(online = false) else it }
                    refreshDevices()
                }
                is FfiSyncEvent.Paired -> refreshDevices()
                // HostOnline: as a guest, the chip "Gast bei …" shows whether the owner is reachable.
                is FfiSyncEvent.GuestJoined, is FfiSyncEvent.GuestLeft, is FfiSyncEvent.RoleChanged, is FfiSyncEvent.AccessRevoked, is FfiSyncEvent.HostOnline -> refreshGuests()
                is FfiSyncEvent.Summoned -> if (openNotebook != event.notebook && event.notebook !in openNotebooks) {
                    // Brought to the owner while elsewhere: open that notebook there.
                    pendingSummon = event
                    if (openHere?.invoke(event.notebook, null) != true) runCatching { app.startActivity(Intent(app, NotebookActivity::class.java).putExtra(NotebookActivity.EXTRA_ID, event.notebook).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)) }
                }
                is FfiSyncEvent.GuestWaiting -> {
                    refreshGuests()
                    // Someone at the door of a notebook that isn't open: say so (letting in
                    // happens in that notebook's guests overview).
                    if (event.on && openNotebook != event.notebook && event.notebook !in openNotebooks) {
                        val title = runCatching { N0te.library.loadNotebook(event.notebook).title() }.getOrNull() ?: "ein Notizbuch"
                        android.widget.Toast.makeText(app, "${event.name} möchte in „$title“ — öffne es, um hereinzulassen", android.widget.Toast.LENGTH_LONG).show()
                    }
                }
                // The laptop asks for a picture of n0te's screen (own devices only; testing).
                is FfiSyncEvent.ScreenshotRequest -> sendScreenshot(event.device)
                is FfiSyncEvent.LatexResult -> latexWaiting.remove(event.id.toLong())?.invoke(event.latex, event.engine, event.error)
                is FfiSyncEvent.OpenRequest -> if (openNotebook != event.notebook && event.notebook !in openNotebooks) {
                    // The laptop's LIVE button: open the notebook here (the app is in front).
                    if (openHere?.invoke(event.notebook, event.device) != true) runCatching {
                        app.startActivity(Intent(app, NotebookActivity::class.java).putExtra(NotebookActivity.EXTRA_ID, event.notebook).putExtra(NotebookActivity.EXTRA_LIVE, event.device).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
                    }
                }
                else -> {}
            }
            listeners.forEach { it(event) }
        }
    }

    private val latexWaiting = HashMap<Long, (String, String, String?) -> Unit>()
    private var latexNext = 1L

    /**
     * Handwriting (PNG) to LaTeX: the laptop reads it (n0bridge's engine, else its local
     * Pix2Text). [done] gets (latex, engine, error) on the main thread; false if not connected.
     */
    fun requestLatex(png: ByteArray, done: (String, String, String?) -> Unit): Boolean {
        val e = engine ?: return false
        val laptop = devices.firstOrNull { it.online } ?: return false
        val id = latexNext++
        latexWaiting[id] = done
        N0te.io.execute {
            runCatching { e.requestLatex(laptop.id, id.toULong(), png) }.onFailure { err ->
                N0te.main.post { latexWaiting.remove(id)?.invoke("", "", err.message ?: "Senden fehlgeschlagen") }
            }
        }
        N0te.main.postDelayed({ latexWaiting.remove(id)?.invoke("", "", "Keine Antwort vom Laptop") }, 90_000)
        return true
    }

    /** A picture of n0te's window as shown (PixelCopy), ≤ 1600 px wide, JPEG, to [device]. */
    private fun sendScreenshot(device: String) {
        val e = engine ?: return
        val a = front?.get() ?: return
        val v = a.window.decorView
        if (v.width <= 0 || v.height <= 0) return
        val bmp = android.graphics.Bitmap.createBitmap(v.width, v.height, android.graphics.Bitmap.Config.ARGB_8888)
        android.view.PixelCopy.request(a.window, bmp, { r ->
            if (r != android.view.PixelCopy.SUCCESS) return@request
            N0te.io.execute {
                val k = minOf(1f, 1600f / bmp.width)
                val img = if (k < 1f) android.graphics.Bitmap.createScaledBitmap(bmp, (bmp.width * k).toInt(), (bmp.height * k).toInt(), true) else bmp
                val out = java.io.ByteArrayOutputStream()
                img.compress(android.graphics.Bitmap.CompressFormat.JPEG, 85, out)
                runCatching { e.sendScreenshot(device, out.toByteArray()) }.onFailure { Log.w(N0te.TAG, "screenshot: ${it.message}") }
            }
        }, android.os.Handler(android.os.Looper.getMainLooper()))
    }

    fun notifyChanged(notebook: String) {
        val e = engine ?: return
        runCatching { e.notifyLocalChange(notebook) }
    }

    /** Asks the laptop to open [notebook] (live: it shows this view, laser and strokes). */
    fun requestOpen(device: String, notebook: String) {
        val e = engine ?: return
        N0te.io.execute { runCatching { e.requestOpen(device, notebook) }.onFailure { Log.w(N0te.TAG, "open: ${it.message}") } }
    }

    /** Asks the laptop to follow (or stop following) this view. */
    fun setFollow(device: String, on: Boolean) {
        val e = engine ?: return
        N0te.io.execute { runCatching { e.setFollow(device, on) }.onFailure { Log.w(N0te.TAG, "follow: ${it.message}") } }
    }

    /** Where this view is (sheet points); throttled by the caller. */
    fun presence(notebook: String, sheet: String?, rect: FloatArray, zoom: Float, laser: FloatArray?) {
        val e = engine ?: return
        if (!sharing(notebook)) return
        runCatching { e.sendPresence(notebook, sheet, rect.toList(), zoom, laser?.toList()) }
    }

    /** Erasing as it happens: strokes just hidden and / or area-eraser dabs (x, y, r triples). */
    fun liveErase(notebook: String, sheet: String, ids: List<String>, dabs: FloatArray) {
        val e = engine ?: return
        if (!sharing(notebook) || (ids.isEmpty() && dabs.isEmpty())) return
        runCatching { e.sendLiveErase(notebook, sheet, ids, dabs.toList()) }
    }

    /**
     * Anyone to send live data to for [notebook]: an own device, a guest in it, or (as a guest)
     * its owner. Guests and hosts got nothing live before (only own devices counted).
     */
    fun sharing(notebook: String) = online || guests.any { it.notebook == notebook } || joined.any { it.notebook == notebook && it.online }

    fun liveStroke(notebook: String, sheet: String, stroke: Long, seq: Int, brush: FfiBrush, samples: ByteArray, done: Boolean) {
        val e = engine ?: return
        if (!sharing(notebook)) return
        runCatching { e.sendLiveStroke(notebook, sheet, stroke.toULong(), seq.toUInt(), brush, samples, done) }
    }
}
