package com.jarvisrchen.yogurt

import android.app.DownloadManager
import android.content.Context
import android.net.Uri
import org.json.JSONArray
import org.json.JSONObject
import java.io.File
import java.security.MessageDigest
import kotlin.concurrent.thread

class ModelFile(val name: String, val url: String, val size: Long, val sha256: String)

enum class ModelKind { WHISPER, TRANSDUCER }

/** A speech model sherpa-onnx runs on the phone. Every file is pinned by size and SHA-256, at a fixed Hugging Face revision. */
class ModelSpec(val name: String, val kind: ModelKind, files: List<ModelFile>) {
    /** The model's own files, then the voice detector that cuts speech at pauses for it. */
    val files = files + VAD

    val bytes get() = files.sumOf { it.size }

    fun path(dir: File, suffix: String) = File(dir, files.first { it.name.endsWith(suffix) }.name).path
}

private fun hf(repo: String, rev: String, vararg files: Triple<String, Long, String>) =
    files.map { (name, size, sha) -> ModelFile(name, "https://huggingface.co/$repo/resolve/$rev/$name", size, sha) }

private val VAD = ModelFile(
    "silero_vad.onnx", "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/silero_vad.onnx",
    643_854, "9e2449e1087496d8d4caba907f23e0bd3f78d91fa552479bb9c23ac09cbb1fd6",
)

private const val WHISPER_TOKENS = "306cd27f03c1a714eca7108e03d66b7dc042abe8c258b44c199a7ed9838dd930"

/** Ascending by size, as on the Mac. The Whisper names are the Mac's. */
val CATALOG = listOf(
    ModelSpec(
        "tiny.en", ModelKind.WHISPER,
        hf(
            "csukuangfj/sherpa-onnx-whisper-tiny.en", "d026532c022fa99fd789d6b32446a1df7b6bfc43",
            Triple("tiny.en-encoder.int8.onnx", 12_937_772, "0ce578b827c94a961aacb8fa14b02f096504b337e5c94be37c36238cbe3e8bc6"),
            Triple("tiny.en-decoder.int8.onnx", 89_853_865, "06c0e6ff6348d427e51839219d1c886c18cfdf411e629e33f5e1679bff9c1527"),
            Triple("tiny.en-tokens.txt", 835_554, WHISPER_TOKENS),
        ),
    ),
    ModelSpec(
        "base.en", ModelKind.WHISPER,
        hf(
            "csukuangfj/sherpa-onnx-whisper-base.en", "59eea950fc76df2453efb57e6c0fd334548e8ffe",
            Triple("base.en-encoder.int8.onnx", 29_120_534, "ef6b936f4c9b1d90a3b68634b60c4ed8576b26172b33c2535ec0e933c9edb823"),
            Triple("base.en-decoder.int8.onnx", 130_669_978, "f7162ad6db2dbef16cfaeaa7f945b9d7dd9c1b8d472f6aca82f2273d185e4d41"),
            Triple("base.en-tokens.txt", 835_554, WHISPER_TOKENS),
        ),
    ),
    ModelSpec(
        "small.en", ModelKind.WHISPER,
        hf(
            "csukuangfj/sherpa-onnx-whisper-small.en", "d9533f69affd85061aee349af7fea5cb2996dbbe",
            Triple("small.en-encoder.int8.onnx", 112_442_483, "8bdac288f369aa94ee2194059238c465ed82ea9d47ee8fa4a8c0a891873e462f"),
            Triple("small.en-decoder.int8.onnx", 262_223_042, "710ccf890e10f3faa15f51ec346081a2723c9f3adb6e4da81c6573a5a6f877fb"),
            Triple("small.en-tokens.txt", 835_554, WHISPER_TOKENS),
        ),
    ),
    ModelSpec(
        "parakeet-v3", ModelKind.TRANSDUCER,
        hf(
            "csukuangfj/sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-int8", "2bda32ec70b097a55adaa07d9a7173915b43cc78",
            Triple("encoder.int8.onnx", 652_184_281, "acfc2b4456377e15d04f0243af540b7fe7c992f8d898d751cf134c3a55fd2247"),
            Triple("decoder.int8.onnx", 11_845_275, "179e50c43d1a9de79c8a24149a2f9bac6eb5981823f2a2ed88d655b24248db4e"),
            Triple("joiner.int8.onnx", 6_355_277, "3164c13fc2821009440d20fcb5fdc78bff28b4db2f8d0f0b329101719c0948b3"),
            Triple("tokens.txt", 93_939, "d58544679ea4bc6ac563d1f545eb7d474bd6cfa467f0a6e2c1dc1c7d37e3c35d"),
        ),
    ),
)

fun sha256(f: File): String {
    val md = MessageDigest.getInstance("SHA-256")
    f.inputStream().use { input ->
        val buf = ByteArray(1 shl 16)
        while (true) {
            val n = input.read(buf)
            if (n < 0) break
            md.update(buf, 0, n)
        }
    }
    return md.digest().joinToString("") { "%02x".format(it) }
}

/** What one model's row shows: its download, if any, and why it stopped. */
data class ModelState(val downloaded: Boolean, val downloading: Boolean, val bytes: Long, val waiting: String?, val error: String?)

/**
 * The models on this phone, in the app's own external files directory, which Android deletes with the app.
 * Android's download manager fetches each file over Wi-Fi only, keeps going while the app is closed, and shows its progress in the shade.
 * A finished download counts only once every file matches its pinned size and hash. Then a `.verified` file marks the model.
 */
class Models(private val context: Context) {
    private val root = File(context.getExternalFilesDir(null) ?: context.filesDir, "models")
    private val dm = context.getSystemService(DownloadManager::class.java)
    private val prefs = context.getSharedPreferences("models", Context.MODE_PRIVATE)
    private val verifying = HashSet<String>()
    private val failed = HashMap<String, String>()

    fun dir(spec: ModelSpec) = File(root, spec.name)

    fun downloaded(spec: ModelSpec) = File(dir(spec), ".verified").exists()

    @Synchronized
    fun download(spec: ModelSpec) {
        if (downloaded(spec) || ids(spec).isNotEmpty()) return
        failed.remove(spec.name)
        val dir = dir(spec).apply { mkdirs() }
        val ids = spec.files.filter { f -> File(dir, f.name).let { !it.exists() || it.length() != f.size } }.map { f ->
            File(dir, "${f.name}.part").delete()
            dm.enqueue(
                DownloadManager.Request(Uri.parse(f.url))
                    .setTitle("${spec.name} · ${f.name}")
                    .setDescription(context.getString(R.string.app_name))
                    .setAllowedOverMetered(false)
                    .setAllowedOverRoaming(false)
                    .setNotificationVisibility(DownloadManager.Request.VISIBILITY_VISIBLE)
                    .setDestinationUri(Uri.fromFile(File(dir, "${f.name}.part"))),
            )
        }
        prefs.edit().putString(spec.name, ids.joinToString(",")).apply()
        if (ids.isEmpty()) verify(spec)
    }

    @Synchronized
    fun cancel(spec: ModelSpec) {
        val ids = ids(spec)
        if (ids.isNotEmpty()) dm.remove(*ids.toLongArray())
        prefs.edit().remove(spec.name).apply()
    }

    @Synchronized
    fun delete(spec: ModelSpec) {
        cancel(spec)
        failed.remove(spec.name)
        dir(spec).deleteRecursively()
    }

    /** Each model's state. A download that has finished starts its check here, so a model is verified once the page next looks. */
    @Synchronized
    fun list(): JSONArray {
        val out = JSONArray()
        for (spec in CATALOG) {
            val s = state(spec)
            out.put(
                JSONObject().put("name", spec.name).put("size_mb", Math.round(spec.bytes / 1e6))
                    .put("downloaded", s.downloaded).put("downloading", s.downloading)
                    .put("bytes", s.bytes).put("total", spec.bytes)
                    .put("waiting", s.waiting ?: JSONObject.NULL).put("error", s.error ?: JSONObject.NULL),
            )
        }
        return out
    }

    private fun state(spec: ModelSpec): ModelState {
        if (downloaded(spec)) return ModelState(true, false, spec.bytes, null, null)
        if (spec.name in verifying) return ModelState(false, true, spec.bytes, null, null)
        val ids = ids(spec)
        if (ids.isEmpty()) return ModelState(false, false, 0, null, failed[spec.name])
        val have = spec.files.filter { f -> File(dir(spec), f.name).length() == f.size }.sumOf { it.size }
        var bytes = have
        var running = 0
        var waiting: String? = null
        var error: String? = null
        dm.query(DownloadManager.Query().setFilterById(*ids.toLongArray())).use { c ->
            while (c.moveToNext()) {
                val status = c.getInt(c.getColumnIndexOrThrow(DownloadManager.COLUMN_STATUS))
                val reason = c.getInt(c.getColumnIndexOrThrow(DownloadManager.COLUMN_REASON))
                bytes += c.getLong(c.getColumnIndexOrThrow(DownloadManager.COLUMN_BYTES_DOWNLOADED_SO_FAR)).coerceAtLeast(0)
                when (status) {
                    DownloadManager.STATUS_SUCCESSFUL -> Unit
                    DownloadManager.STATUS_FAILED -> error = "The download failed (${reason})."
                    DownloadManager.STATUS_PAUSED -> {
                        running++
                        if (reason == DownloadManager.PAUSED_QUEUED_FOR_WIFI || reason == DownloadManager.PAUSED_WAITING_FOR_NETWORK) {
                            waiting = "Waiting for Wi-Fi"
                        }
                    }
                    else -> running++
                }
            }
        }
        if (error != null) {
            // The files that did finish are kept, since removing a download deletes its file, and Get fetches only the rest.
            for (f in spec.files) File(dir(spec), "${f.name}.part").takeIf { it.length() == f.size }?.renameTo(File(dir(spec), f.name))
            cancel(spec)
            failed[spec.name] = error!!
            return ModelState(false, false, 0, null, error)
        }
        if (running == 0) verify(spec)
        return ModelState(false, true, bytes.coerceAtMost(spec.bytes), waiting, null)
    }

    /** Checks every file off the caller's thread, then marks the model, or deletes what failed so Get starts over. */
    private fun verify(spec: ModelSpec) {
        if (!verifying.add(spec.name)) return
        prefs.edit().remove(spec.name).apply()
        thread(name = "yogurt-verify") {
            val dir = dir(spec)
            val bad = spec.files.firstOrNull { f ->
                val done = File(dir, f.name)
                val part = File(dir, "${f.name}.part")
                if (part.exists()) {
                    done.delete()
                    part.renameTo(done)
                }
                !(done.length() == f.size && sha256(done) == f.sha256).also { ok -> if (!ok) done.delete() }
            }
            synchronized(this) {
                verifying.remove(spec.name)
                if (bad == null) File(dir, ".verified").createNewFile() else failed[spec.name] = "${bad.name} did not match its pinned hash. Try Get again."
            }
        }
    }

    private fun ids(spec: ModelSpec) = prefs.getString(spec.name, "")!!.split(",").filter { it.isNotEmpty() }.map { it.toLong() }
}
