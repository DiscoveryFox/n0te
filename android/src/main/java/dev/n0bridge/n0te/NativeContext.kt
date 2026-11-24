package dev.n0bridge.n0te

import android.content.Context
import android.util.Log

/**
 * Hands the JVM and the application context to the Rust side (iroh reads the active
 * network's DNS servers through it; without, it uses public fallback servers that some
 * networks block). The status shows whether it worked (`systemDns`).
 */
object NativeContext {
    @JvmStatic private external fun install(context: Context): Boolean

    fun init(ctx: Context): Boolean = runCatching {
        // Loaded through the JNI loader too, so `install` resolves; JNA shares the same copy.
        System.loadLibrary("n0te_ffi")
        install(ctx.applicationContext)
    }.onFailure { Log.w(N0te.TAG, "native context: ${it.message}") }.getOrDefault(false)
}
