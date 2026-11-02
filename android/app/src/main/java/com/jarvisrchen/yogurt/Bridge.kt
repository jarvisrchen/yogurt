package com.jarvisrchen.yogurt

import android.webkit.JavascriptInterface
import org.json.JSONObject
import java.net.URI

/**
 * Events for the page, in order. While no Cloud page is loaded they wait here and go out when one is.
 * Interim text is the exception: it is stale by then, and the next one replaces it.
 */
object NativeEvents {
    private val pending = ArrayList<String>()
    private var sink: ((String) -> Unit)? = null

    @Synchronized
    fun post(detail: JSONObject) {
        val s = detail.toString()
        val to = sink
        if (to != null) to(s) else if (detail.optString("type") != "interim") pending.add(s)
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
        val auth = auth(a.optJSONObject("auth"))
        // The service sends the user's session to tokenUrl, so it must be the Cloud's own.
        val tokenUrl = runCatching { URI("$origin/").resolve(a.optString("tokenUrl")).toString() }.getOrNull()
        if (meetingId.isEmpty() || startedAtMs <= 0 || auth == null || !Cloud.sameOrigin(tokenUrl, origin)) {
            return result("The start request needs meetingId, startedAtMs, a tokenUrl on this Cloud, and auth.")
        }
        val r = StartRequest(meetingId, startedAtMs, tokenUrl!!, auth)
        // Asking for the mic is a dialog. The recording then starts or fails through events.
        if (!activity.hasMic()) return result(null).also { activity.askMic(r) }
        return result(Recording.start(r))
    }

    @JavascriptInterface
    fun setAuth(json: String) {
        if (!allowed()) return
        auth(runCatching { JSONObject(json) }.getOrNull())?.let(Recording::setAuth)
    }

    @JavascriptInterface
    fun stop() {
        if (allowed()) Recording.stop()
    }

    @JavascriptInterface
    fun status(): String = if (allowed()) Recording.status().toString() else "null"

    @JavascriptInterface
    fun lines(from: Int): String = if (allowed()) Recording.store.from(from).toString() else "[]"

    @JavascriptInterface
    fun appVersion(): String = BuildConfig.VERSION_NAME
}
