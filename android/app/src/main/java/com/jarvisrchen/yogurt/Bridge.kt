package com.jarvisrchen.yogurt

import android.os.Build
import android.webkit.JavascriptInterface
import org.json.JSONObject
import java.net.URI
import kotlin.concurrent.thread

/**
 * Events for the page, in order. While no Cloud page is loaded they wait here and go out when one is.
 * Interim text and a request for a fresh session are the exceptions: they are stale by then, and the recorder sends the next one when it needs it.
 */
object NativeEvents {
    private val pending = ArrayList<String>()
    private var sink: ((String) -> Unit)? = null

    @Synchronized
    fun post(detail: JSONObject) {
        val s = detail.toString()
        val to = sink
        if (to != null) to(s) else if (detail.optString("type") !in setOf("interim", "auth")) pending.add(s)
    }

    @Synchronized
    fun attach(to: (String) -> Unit) {
        sink = to
        pending.forEach(to)
        pending.clear()
    }

    @Synchronized
    fun detach() {
        sink = null
    }
}

/**
 * `window.YogurtAndroid`, the page's way to the recorder. Every value crossing it is a JSON string.
 * Calls count only while the WebView shows the Cloud the user entered; the WebView never navigates elsewhere, so this is a second fence.
 */
class YogurtAndroid(private val activity: MainActivity, private val origin: String) {
    /** The page's URL, kept current from the main thread, since the bridge runs on its own. */
    @Volatile var pageUrl: String? = null

    private fun allowed() = Cloud.sameOrigin(pageUrl, origin)

    private fun result(error: String?) =
        (if (error == null) JSONObject().put("ok", true) else JSONObject().put("ok", false).put("error", error)).toString()

    private fun auth(j: JSONObject?): Auth? {
        val jwt = j?.optString("jwt").orEmpty()
        val token = j?.optString("browserToken").orEmpty()
        return if (jwt.isEmpty() || token.isEmpty()) null else Auth(jwt, token)
    }

    @JavascriptInterface
    fun start(json: String): String {
        if (!allowed()) return result("This page is not your Cloud.")
        val a = runCatching { JSONObject(json) }.getOrNull() ?: return result("The start request is not JSON.")
        val meetingId = a.optString("meetingId")
        val startedAtMs = a.optLong("startedAtMs")
        val have = a.optInt("have", -1)
        val auth = auth(a.optJSONObject("auth"))
        // The service sends the user's session to tokenUrl, so it must be the Cloud's own.
        val tokenUrl = runCatching { URI("$origin/").resolve(a.optString("tokenUrl")).toString() }.getOrNull()
        if (meetingId.isEmpty() || startedAtMs <= 0 || have < 0 || auth == null || !Cloud.sameOrigin(tokenUrl, origin)) {
            return result("The start request needs meetingId, startedAtMs, have, a tokenUrl on this Cloud, and auth.")
        }
        val r = StartRequest(meetingId, startedAtMs, have, tokenUrl!!, auth, a.optString("model").ifEmpty { null })
        // Asking for the mic is a dialog. The recording then starts or fails through events.
        if (!activity.hasMic()) return result(null).also { activity.askMic(r) }
        return result(Recording.start(r))
    }

    @JavascriptInterface
    fun setAuth(json: String) {
        if (!allowed()) return
        auth(runCatching { JSONObject(json) }.getOrNull())?.let(Recording::setAuth)
    }

    /** Returns at once, since stopping waits up to 5 seconds for Deepgram. The last lines and then the stopped state arrive as events. */
    @JavascriptInterface
    fun stop() {
        if (allowed()) thread { Recording.stop() }
    }

    @JavascriptInterface
    fun status(): String = if (allowed()) Recording.status().toString() else "null"

    @JavascriptInterface
    fun lines(meetingId: String, from: Int): String = if (allowed()) Recording.store.from(meetingId, from).toString() else "[]"

    @JavascriptInterface
    fun appVersion(): String = BuildConfig.VERSION_NAME

    /** The on-device models and their downloads, which the Transcription page polls while one runs. */
    @JavascriptInterface
    fun models(): String = if (allowed()) Recording.models.list().toString() else "[]"

    @JavascriptInterface
    fun downloadModel(name: String): String {
        val spec = spec(name) ?: return result("There is no model called $name.")
        // The APK carries the speech runtime for 64-bit ARM only, so a model elsewhere would download and then fail to load.
        if ("arm64-v8a" !in Build.SUPPORTED_ABIS) return result("This phone can't run on-device models, which need a 64-bit ARM processor. Cloud still works.")
        Recording.models.download(spec)
        return result(null)
    }

    @JavascriptInterface
    fun cancelDownload(name: String) {
        spec(name)?.let(Recording.models::cancel)
    }

    @JavascriptInterface
    fun deleteModel(name: String): String {
        val spec = spec(name) ?: return result("There is no model called $name.")
        if (Recording.localModel() === spec) return result("A recording is using ${spec.name}. End it first.")
        Recording.models.delete(spec)
        return result(null)
    }

    /** Loads the model and decodes silence off this thread. The verdict arrives as a `model-test` event. */
    @JavascriptInterface
    fun testModel(name: String) {
        val spec = spec(name) ?: return
        thread {
            val started = System.currentTimeMillis()
            val busy = Recording.localModel()
            val error = when {
                busy != null -> "A recording is transcribing with ${busy.name}, and the phone holds one model at a time. Test after it ends."
                !Recording.models.downloaded(spec) -> "${spec.name} isn't downloaded yet."
                else -> LocalRecorder.selfTest(spec, Recording.models.dir(spec))
            }
            val verdict = JSONObject().put("type", "model-test").put("name", spec.name).put("ok", error == null)
            if (error == null) verdict.put("model", "${spec.name} · loaded and ran in ${System.currentTimeMillis() - started}ms") else verdict.put("error", error)
            NativeEvents.post(verdict)
        }
    }

    private fun spec(name: String) = if (allowed()) CATALOG.firstOrNull { it.name == name } else null
}
