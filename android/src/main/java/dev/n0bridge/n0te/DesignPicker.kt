package dev.n0bridge.n0te

import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.content.Intent
import android.net.Uri
import android.widget.Toast
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.window.Dialog
import uniffi.n0te_ffi.FfiDesignRead

/**
 * SETTINGS › Design: every design as a card (the classics, the special ones, your own), the
 * decoration switches, and making your own design: copy the instructions for an AI, paste its
 * answer back (clipboard or file) — n0te checks it, shows a preview or lists what the AI should
 * fix. [apply] switches to a design (the activity re-creates).
 */
@OptIn(ExperimentalLayoutApi::class)
@Composable
fun DesignPicker(apply: (Theme) -> Unit) {
    val ctx = LocalContext.current
    var version by remember { mutableIntStateOf(0) }
    var read by remember { mutableStateOf<Pair<String, FfiDesignRead>?>(null) }
    var showAi by remember { mutableStateOf(false) }
    var deleting by remember { mutableStateOf<Theme?>(null) }
    var decorShow by remember { mutableStateOf(Settings.decorShow) }
    var decorMotion by remember { mutableStateOf(Settings.decorMotion) }
    val check = { text: String ->
        if (text.isBlank()) Toast.makeText(ctx, "Nichts zum Importieren gefunden", Toast.LENGTH_SHORT).show()
        else read = text to uniffi.n0te_ffi.designRead(text, false)
    }
    val pick = rememberLauncherForActivityResult(ActivityResultContracts.OpenDocument()) { uri: Uri? ->
        if (uri != null) DesignFiles.readText(ctx, uri).onSuccess(check).onFailure {
            Toast.makeText(ctx, "Datei ließ sich nicht lesen: ${it.message}", Toast.LENGTH_LONG).show()
        }
    }
    @Suppress("UNUSED_VARIABLE") val v = version

    Column(verticalArrangement = Arrangement.spacedBy(20.dp)) {
        DesignGroup("Klassisch", Themes.builtins.take(Themes.classics), apply)
        DesignGroup("Besonders", Themes.builtins.drop(Themes.classics), apply)
        if (Themes.imported.isNotEmpty()) DesignGroup("Eigene", Themes.imported, apply, onShare = { DesignFiles.share(ctx, it) }, onDelete = { deleting = it })

        Column(Modifier.widthIn(max = 820.dp).fillMaxWidth().group().padding(vertical = 8.dp)) {
            SwitchRow("Dekorationen", "Schnee, Lichter, Sterne … mancher Designs hinter der Bibliothek", decorShow) {
                decorShow = !decorShow; Settings.set("decor_show", decorShow)
            }
            if (decorShow) SwitchRow("Dekorationen bewegen sich", "Aus: sie stehen still (spart etwas Akku)", decorMotion) {
                decorMotion = !decorMotion; Settings.set("decor_motion", decorMotion)
            }
        }

        Column(Modifier.widthIn(max = 820.dp).fillMaxWidth().group().padding(18.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
            Text("Eigenes Design", color = Ui.Chalk, fontSize = 17.sp, fontFamily = Voice.titleFont, fontWeight = FontWeight.Medium)
            Text("Beschreib einer KI (ChatGPT, Claude, Gemini …), wie n0te aussehen soll. Sie schreibt eine kleine Design-Datei, die du hier importierst. Funktioniert auch auf dem Laptop.",
                color = Ui.Muted, fontSize = 13.sp, lineHeight = 18.sp)
            FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Btn("Mit KI gestalten", BtnKind.PRIMARY) { showAi = true }
                Btn("Aus Zwischenablage") { check(DesignFiles.clipboard(ctx)) }
                Btn("Datei öffnen") { pick.launch(arrayOf("application/json", "text/plain", "application/octet-stream")) }
            }
        }
    }

    if (showAi) AiDialog(onClose = { showAi = false }, onPaste = { showAi = false; check(DesignFiles.clipboard(ctx)) })
    read?.let { (text, r) ->
        ReadDialog(r, onClose = { read = null }, onSave = { andApply ->
            N0te.background({ uniffi.n0te_ffi.designImport(Themes.folder, text) }) { res ->
                read = null
                res.onSuccess { d ->
                    Themes.reloadImported()
                    version++
                    if (andApply) apply(Themes.byId(d.id)) else Toast.makeText(ctx, "„${d.name}“ gespeichert", Toast.LENGTH_SHORT).show()
                }.onFailure { Toast.makeText(ctx, "Speichern ging nicht: ${it.message}", Toast.LENGTH_LONG).show() }
            }
        })
    }
    deleting?.let { t ->
        ConfirmDialog("„${t.name}“ löschen?", "Das Design verschwindet von diesem Gerät. Die Datei, aus der es kam, bleibt.", "Löschen", onClose = { deleting = null }) {
            deleting = null
            val wasOn = Ui.theme.id == t.id
            if (!uniffi.n0te_ffi.designRemove(Themes.folder, t.id)) Toast.makeText(ctx, "Löschen ging nicht", Toast.LENGTH_SHORT).show()
            Themes.reloadImported()
            version++
            if (wasOn) apply(Themes.builtins.first())
        }
    }
}

@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun DesignGroup(title: String, themes: List<Theme>, apply: (Theme) -> Unit, onShare: ((Theme) -> Unit)? = null, onDelete: ((Theme) -> Unit)? = null) {
    if (themes.isEmpty()) return
    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        Caption(title, Modifier.padding(start = 4.dp))
        FlowRow(horizontalArrangement = Arrangement.spacedBy(12.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            themes.forEach { t -> StyleCard(t, onClick = { if (Ui.theme.id != t.id) apply(t) }, onShare = onShare?.let { { it(t) } }, onDelete = onDelete?.let { { it(t) } }) }
        }
    }
}

/** A design as a card: a miniature (its chrome, decorations, heading face, button shapes, paper), name, one line. */
@Composable
fun StyleCard(t: Theme, modifier: Modifier = Modifier, wide: Boolean = false, onClick: (() -> Unit)? = null, onShare: (() -> Unit)? = null, onDelete: (() -> Unit)? = null) {
    val on = Ui.theme.id == t.id
    val st = t.style
    Column(modifier.width(if (wide) 360.dp else 210.dp).option(on).then(if (onClick != null) Modifier.clickable(onClick = onClick) else Modifier).padding(10.dp)) {
        DesignMiniature(t, Modifier.fillMaxWidth().height(if (wide) 150.dp else 86.dp))
        Row(Modifier.padding(top = 8.dp), verticalAlignment = Alignment.CenterVertically) {
            Text(t.name, color = Ui.Chalk, fontSize = 15.sp, fontFamily = Voice.titleFont, fontWeight = FontWeight.Medium, modifier = Modifier.weight(1f), maxLines = 1)
            if (on) Box(Modifier.size(8.dp).clip(CircleShape).background(Ui.Red))
        }
        if (st.desc.isNotEmpty()) Text(st.desc, color = Ui.Muted, fontSize = 12.sp, lineHeight = 15.sp, maxLines = if (wide) 3 else 2)
        if (t.author.isNotEmpty()) Text("von ${t.author}", color = Ui.Muted, fontSize = 11.sp, maxLines = 1)
        if (onShare != null || onDelete != null) Row(Modifier.padding(top = 6.dp), horizontalArrangement = Arrangement.spacedBy(14.dp)) {
            onShare?.let { CardAction("Teilen", Ui.Chalk, it) }
            onDelete?.let { CardAction("Löschen", Ui.Red, it) }
        }
    }
}

@Composable
private fun CardAction(label: String, color: Color, onClick: () -> Unit) {
    Text(Voice.cased(label), color = color, fontSize = 12.sp, fontFamily = Voice.labelFont, fontWeight = FontWeight.Medium, letterSpacing = Voice.labelSpacing,
        modifier = Modifier.clip(RoundedCornerShape(6.dp)).clickable(onClick = onClick).padding(vertical = 4.dp, horizontal = 2.dp))
}

/** A design in small, in its own colours, whatever design n0te itself is in. */
@Composable
fun DesignMiniature(t: Theme, modifier: Modifier) {
    val st = t.style
    val mini = RoundedCornerShape(minOf(st.card, 10f).dp)
    Box(modifier.clip(mini).background(Color(t.bg)).border(1.dp, Color(t.border), mini)) {
        DecorLayer(t, Modifier.matchParentSize(), still = true)
        Row(Modifier.matchParentSize().padding(12.dp), verticalAlignment = Alignment.CenterVertically) {
            Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                val face = when (st.heading) { HeadingFace.DOT -> Type.Dot; HeadingFace.MONO -> Type.Mono; HeadingFace.SANS -> Type.Sans; HeadingFace.SERIF -> Type.Serif }
                Text(if (st.heading == HeadingFace.DOT) "aa" else "Aa", color = Color(t.fg), fontFamily = face,
                    fontWeight = if (st.heading == HeadingFace.DOT) FontWeight.Black else FontWeight.Bold, fontSize = if (st.heading == HeadingFace.DOT) 28.sp else 24.sp)
                Row(horizontalArrangement = Arrangement.spacedBy(5.dp)) {
                    val r = RoundedCornerShape(minOf(st.button, 7f).dp)
                    Box(Modifier.size(width = 34.dp, height = 12.dp).clip(r).background(Color(t.accent)))
                    val (bg, line) = when (st.fill) {
                        Fill.OUTLINE -> Color.Transparent to Color(t.border)
                        Fill.TONAL -> (if (t.dark) Color(t.accent).copy(alpha = 0.22f) else Color(t.elevated)) to Color.Transparent
                        Fill.RULE -> Color.Transparent to Color(t.fg).copy(alpha = 0.6f)
                    }
                    Box(Modifier.size(width = 28.dp, height = 12.dp).clip(r).background(bg).then(if (line.alpha > 0f) Modifier.border(1.dp, line, r) else Modifier))
                }
            }
            Box(Modifier.size(width = 34.dp, height = 46.dp).clip(RoundedCornerShape(minOf(st.card, 3f).dp)).background(Color(t.paper)))
        }
    }
}

@Composable
private fun SwitchRow(title: String, sub: String, value: Boolean, onClick: () -> Unit) {
    Row(Modifier.fillMaxWidth().clickable(onClick = onClick).padding(horizontal = 16.dp, vertical = 11.dp), verticalAlignment = Alignment.CenterVertically) {
        Column(Modifier.weight(1f)) {
            Text(title, color = Ui.Chalk, fontSize = 15.sp)
            Text(sub, color = Ui.Muted, fontSize = 12.sp)
        }
        Switch(value)
    }
}

/** Three steps: copy the instructions, ask an AI, paste its answer. */
@Composable
private fun AiDialog(onClose: () -> Unit, onPaste: () -> Unit) {
    val ctx = LocalContext.current
    Dialog(onDismissRequest = onClose) {
        N0teType {
            Column(Modifier.width(560.dp).panel().padding(26.dp), verticalArrangement = Arrangement.spacedBy(14.dp)) {
                Caption("Eigenes Design")
                Heading("Mit KI gestalten", 24.sp)
                Column(Modifier.weight(1f, fill = false).verticalScroll(rememberScrollState()), verticalArrangement = Arrangement.spacedBy(14.dp)) {
                    Step(1, "Kopiere die Anleitung für die KI. Sie erklärt das Format und enthält ein Beispiel.") {
                        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                            Btn("Anleitung kopieren") {
                                DesignFiles.copy(ctx, "n0te-Design-Anleitung", uniffi.n0te_ffi.designPrompt(false))
                                Toast.makeText(ctx, "Anleitung kopiert", Toast.LENGTH_SHORT).show()
                            }
                            Btn("An KI-App senden", BtnKind.QUIET) { DesignFiles.sendText(ctx, uniffi.n0te_ffi.designPrompt(false), "n0te-Design") }
                        }
                    }
                    Step(2, "Füge sie in einen KI-Chat ein und schreib in die Zeile „Mein Wunsch“, wie n0te aussehen soll: „gemütlich wie eine Bibliothek bei Nacht“, „meine Schulfarben Blau und Gold“ …") {}
                    Step(3, "Kopiere die Antwort der KI und füge sie hier ein. Passt etwas noch nicht, zeigt n0te, was die KI ändern soll.") {}
                }
                Row(Modifier.fillMaxWidth().padding(top = 4.dp), verticalAlignment = Alignment.CenterVertically) {
                    Btn("Schließen", BtnKind.QUIET) { onClose() }
                    Spacer(Modifier.weight(1f))
                    Btn("Antwort einfügen", BtnKind.PRIMARY) { onPaste() }
                }
            }
        }
    }
}

@Composable
private fun Step(n: Int, text: String, extra: @Composable () -> Unit) {
    Row(verticalAlignment = Alignment.Top) {
        Box(Modifier.size(26.dp).clip(CircleShape).background(Ui.Red), contentAlignment = Alignment.Center) {
            Text("$n", color = Ui.OnAccent, fontSize = 13.sp, fontWeight = FontWeight.Bold)
        }
        Column(Modifier.padding(start = 12.dp).weight(1f), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Text(text, color = Ui.Chalk, fontSize = 14.sp, lineHeight = 20.sp)
            extra()
        }
    }
}

/** A read design: its preview with "apply"/"save", or what is wrong with it (to hand back to the AI). */
@Composable
private fun ReadDialog(r: FfiDesignRead, onClose: () -> Unit, onSave: (apply: Boolean) -> Unit) {
    val ctx = LocalContext.current
    val d = r.design
    Dialog(onDismissRequest = onClose) {
        N0teType {
            Column(Modifier.width(560.dp).panel().padding(26.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                if (d == null) {
                    Caption("Import")
                    Heading("Das Design passt noch nicht", 24.sp)
                    Column(Modifier.weight(1f, fill = false).verticalScroll(rememberScrollState()), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                        Text("Gib der KI diese Liste, sie bessert das Design aus. Dann die neue Antwort wieder einfügen.", color = Ui.Muted, fontSize = 14.sp)
                        r.errors.forEach { ChangeItem(it) }
                    }
                    Row(Modifier.fillMaxWidth().padding(top = 6.dp), verticalAlignment = Alignment.CenterVertically) {
                        Btn("Schließen", BtnKind.QUIET) { onClose() }
                        Spacer(Modifier.weight(1f))
                        Btn("Fehler kopieren", BtnKind.PRIMARY) {
                            DesignFiles.copy(ctx, "n0te-Design-Fehler", "n0te kann das Design so nicht importieren. Bitte korrigiere es und antworte wieder nur mit dem JSON:\n" + r.errors.joinToString("\n") { "- $it" })
                            Toast.makeText(ctx, "Fehler kopiert", Toast.LENGTH_SHORT).show()
                        }
                    }
                } else {
                    val t = Themes.fromCore(d)
                    Caption("Neues Design")
                    Column(Modifier.weight(1f, fill = false).verticalScroll(rememberScrollState()), verticalArrangement = Arrangement.spacedBy(10.dp)) {
                        StyleCard(t, Modifier.fillMaxWidth(), wide = true)
                        if (r.warnings.isNotEmpty()) {
                            Caption("Angepasst", Modifier.padding(top = 4.dp))
                            r.warnings.forEach { ChangeItem(it) }
                        }
                    }
                    Row(Modifier.fillMaxWidth().padding(top = 6.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        Btn("Abbrechen", BtnKind.QUIET) { onClose() }
                        Spacer(Modifier.weight(1f))
                        Btn("Nur speichern") { onSave(false) }
                        Btn("Übernehmen", BtnKind.PRIMARY) { onSave(true) }
                    }
                }
            }
        }
    }
}

@Composable
private fun ConfirmDialog(title: String, text: String, action: String, onClose: () -> Unit, onConfirm: () -> Unit) {
    Dialog(onDismissRequest = onClose) {
        N0teType {
            Column(Modifier.width(440.dp).panel().padding(26.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                Heading(title, 22.sp)
                Text(text, color = Ui.Muted, fontSize = 14.sp, lineHeight = 20.sp)
                Row(Modifier.fillMaxWidth().padding(top = 6.dp), verticalAlignment = Alignment.CenterVertically) {
                    Btn("Abbrechen", BtnKind.QUIET) { onClose() }
                    Spacer(Modifier.weight(1f))
                    Btn(action, BtnKind.DANGER) { onConfirm() }
                }
            }
        }
    }
}

/** Getting design text in and out: clipboard, files, the share sheet. */
object DesignFiles {
    /** A design file is small; anything bigger is cut here and refused by the core. */
    private const val MAX = 70_000

    fun clipboard(ctx: Context): String = runCatching {
        val cm = ctx.getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager
        cm.primaryClip?.getItemAt(0)?.coerceToText(ctx)?.toString()
    }.getOrNull().orEmpty()

    fun copy(ctx: Context, label: String, text: String) {
        val cm = ctx.getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager
        cm.setPrimaryClip(ClipData.newPlainText(label, text))
    }

    fun readText(ctx: Context, uri: Uri): Result<String> = runCatching {
        ctx.contentResolver.openInputStream(uri)!!.use { input ->
            val out = java.io.ByteArrayOutputStream()
            val buf = ByteArray(8192)
            while (out.size() < MAX) {
                val n = input.read(buf)
                if (n < 0) break
                out.write(buf, 0, n)
            }
            out.toString(Charsets.UTF_8.name())
        }
    }

    /** Hands text to another app (an AI chat, a messenger). */
    fun sendText(ctx: Context, text: String, subject: String) {
        val send = Intent(Intent.ACTION_SEND).setType("text/plain").putExtra(Intent.EXTRA_TEXT, text).putExtra(Intent.EXTRA_SUBJECT, subject)
        runCatching { ctx.startActivity(Intent.createChooser(send, null)) }
            .onFailure { Toast.makeText(ctx, "Keine App zum Teilen gefunden", Toast.LENGTH_SHORT).show() }
    }

    /** Shares a design as its file text (imports on another tablet or the laptop). */
    fun share(ctx: Context, t: Theme) = sendText(ctx, t.json, "n0te-Design: ${t.name}")
}
