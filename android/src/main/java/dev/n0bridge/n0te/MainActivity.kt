package dev.n0bridge.n0te

import android.content.Intent
import android.os.Build
import android.os.Bundle
import android.util.Log
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uniffi.n0te_ffi.runCoreBench
import java.io.File

/**
 * M0 spike screen: run the shared Rust core's speed check on this device, and open the
 * low-latency ink test. Results also go to logcat (tag N0TE), readable via
 * `n0bridge debug <device> logs` once n0te reports there.
 */
class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContent { SpikeScreen() }
    }

    @Composable
    private fun SpikeScreen() {
        var lines by remember { mutableStateOf(listOf<String>()) }
        var running by remember { mutableStateOf(false) }
        val scope = rememberCoroutineScope()
        Column(
            Modifier.fillMaxSize().background(Black).padding(24.dp).verticalScroll(rememberScrollState()),
            verticalArrangement = Arrangement.spacedBy(16.dp)
        ) {
            Text("n0te", color = Chalk, fontSize = 40.sp, fontWeight = FontWeight.Black, fontFamily = FontFamily.Monospace)
            Text("M0 // SPIKES · ${Build.MANUFACTURER} ${Build.MODEL} · Android ${Build.VERSION.RELEASE}",
                color = Muted, fontSize = 12.sp, fontFamily = FontFamily.Monospace, letterSpacing = 1.sp)
            Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                Pill(if (running) "LÄUFT…" else "CORE-BENCH", primary = true) {
                    if (running) return@Pill
                    running = true
                    scope.launch {
                        val out = withContext(Dispatchers.Default) {
                            val dir = File(cacheDir, "bench-${System.currentTimeMillis()}")
                            runCatching { runCoreBench(dir.absolutePath).lines }
                                .getOrElse { listOf("Fehler: ${it.message}") }
                                .also { dir.deleteRecursively() }
                        }
                        out.forEach { Log.i(TAG, "bench: $it") }
                        lines = out
                        running = false
                    }
                }
                Pill("TINTE TESTEN") { startActivity(Intent(this@MainActivity, InkSpikeActivity::class.java)) }
            }
            if (lines.isNotEmpty()) {
                Column(
                    Modifier.clip(RoundedCornerShape(20.dp)).background(Surface).border(1.dp, Border, RoundedCornerShape(20.dp)).padding(16.dp),
                    verticalArrangement = Arrangement.spacedBy(6.dp)
                ) {
                    lines.forEach { Text(it, color = Chalk, fontSize = 13.sp, fontFamily = FontFamily.Monospace) }
                }
            }
        }
    }

    @Composable
    private fun Pill(text: String, primary: Boolean = false, onClick: () -> Unit) {
        Box(
            Modifier.clip(RoundedCornerShape(22.dp))
                .background(if (primary) Red else Surface)
                .border(1.dp, if (primary) Red else Border, RoundedCornerShape(22.dp))
                .clickable(onClick = onClick)
                .padding(horizontal = 20.dp, vertical = 12.dp)
        ) {
            Text(text, color = Chalk, fontSize = 13.sp, fontFamily = FontFamily.Monospace, fontWeight = FontWeight.Bold, letterSpacing = 1.sp)
        }
    }

    companion object {
        const val TAG = "N0TE"
        val Black = Color(0xFF000000)
        val Surface = Color(0xFF141416)
        val Border = Color(0xFF262629)
        val Chalk = Color(0xFFF2F2F4)
        val Muted = Color(0xFF787880)
        val Red = Color(0xFFD71921)
    }
}
