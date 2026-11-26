package dev.n0bridge.n0te

import android.content.Context
import android.os.Build
import android.widget.Toast
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.window.Dialog
import java.io.File

/**
 * Crash reports. An uncaught exception (a Rust panic arrives as one too) writes a report to
 * `files/crashes`: version, Android version, device, the stack trace. The next library start
 * offers to copy or share it. A report holds no notes, names or files of the user.
 */
object CrashLog {
    /** Reports kept (the oldest go). */
    private const val KEEP = 5
    const val ISSUES = "https://github.com/DiscoveryFox/n0te/issues/new"

    fun dir(ctx: Context) = File(ctx.filesDir, "crashes")

    /** From now on, every crash writes a report first; then the system handles it as before. */
    fun install(ctx: Context) {
        val app = ctx.applicationContext
        val previous = Thread.getDefaultUncaughtExceptionHandler()
        Thread.setDefaultUncaughtExceptionHandler { thread, e ->
            runCatching {
                val device = "Android ${Build.VERSION.RELEASE} (API ${Build.VERSION.SDK_INT}), ${Build.MANUFACTURER} ${Build.MODEL}"
                save(dir(app), compose(Updater.current(app), device, thread.name, e), System.currentTimeMillis())
            }
            previous?.uncaughtException(thread, e)
        }
    }

    fun compose(version: String, device: String, thread: String, e: Throwable): String =
        "n0te $version ($device)\nthread '$thread' crashed:\n$e\n\n${e.stackTraceToString()}"

    fun save(dir: File, text: String, atMs: Long): File {
        dir.mkdirs()
        val f = File(dir, "crash-$atMs.txt")
        f.writeText(text)
        reports(dir).dropLast(KEEP).forEach { it.first.delete() }
        return f
    }

    /** Every report, oldest first, with whether it was shown already. */
    private fun reports(dir: File): List<Pair<File, Boolean>> =
        (dir.listFiles() ?: emptyArray()).mapNotNull { f ->
            val rest = f.name.removePrefix("crash-").takeIf { it != f.name } ?: return@mapNotNull null
            val seen = rest.endsWith(".seen.txt")
            val stamp = (if (seen) rest.removeSuffix(".seen.txt") else rest.removeSuffix(".txt").takeIf { it != rest })?.toLongOrNull() ?: return@mapNotNull null
            Triple(stamp, f, seen)
        }.sortedBy { it.first }.map { it.second to it.third }

    /** The newest report not shown yet. */
    fun unseen(dir: File): String? = reports(dir).lastOrNull { !it.second }?.first?.let { runCatching { it.readText() }.getOrNull() }

    /** Marks every report as shown (they stay in the folder). */
    fun markSeen(dir: File) = reports(dir).filter { !it.second }.forEach { (f, _) -> f.renameTo(File(f.parentFile, f.name.removeSuffix(".txt") + ".seen.txt")) }

    /** What went wrong, in one line. */
    fun summary(report: String): String = report.lineSequence().dropWhile { !it.startsWith("thread '") }.drop(1).firstOrNull()?.trim()?.take(120).orEmpty()
}

/** n0te crashed last time: what went wrong, copy or share the report. */
@Composable
fun CrashDialog(report: String, onClose: () -> Unit) {
    val ctx = LocalContext.current
    val done = { CrashLog.markSeen(CrashLog.dir(ctx)); onClose() }
    Dialog(onDismissRequest = done) {
        N0teType {
            Column(Modifier.width(520.dp).panel().padding(26.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                Caption("Absturz")
                Heading("n0te ist abgestürzt", 24.sp)
                Text("Fehler: ${CrashLog.summary(report)}", color = Ui.Muted, fontSize = 13.sp, fontFamily = Type.Mono)
                Text("Hilf beim Beheben: teile den Bericht und schreib dazu, was du gerade getan hast. Er enthält keine Notizen.", color = Ui.Chalk, fontSize = 14.sp, lineHeight = 20.sp)
                Row(Modifier.fillMaxWidth().padding(top = 6.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    Btn("Schließen", BtnKind.QUIET) { done() }
                    Spacer(Modifier.weight(1f))
                    Btn("Kopieren") {
                        DesignFiles.copy(ctx, "n0te-Absturzbericht", report)
                        Toast.makeText(ctx, "Bericht kopiert", Toast.LENGTH_SHORT).show()
                        done()
                    }
                    Btn("Teilen", BtnKind.PRIMARY) { DesignFiles.sendText(ctx, report, "n0te-Absturzbericht"); done() }
                }
            }
        }
    }
}
