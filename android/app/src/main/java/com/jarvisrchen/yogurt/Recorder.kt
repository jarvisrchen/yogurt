package com.jarvisrchen.yogurt

import okhttp3.MediaType.Companion.toMediaType
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.RequestBody.Companion.toRequestBody
import okhttp3.Response
import okhttp3.WebSocket
import okhttp3.WebSocketListener
import okio.ByteString.Companion.toByteString
import org.json.JSONObject
import java.io.IOException
import java.util.concurrent.CountDownLatch
import java.util.concurrent.Executors
import java.util.concurrent.RejectedExecutionException
import java.util.concurrent.TimeUnit
import kotlin.concurrent.thread

data class Auth(val jwt: String, val browserToken: String)

private const val CHUNK_BYTES = 100 * BYTES_PER_MS
private const val MAX_BACKOFF_MS = 10_000L
// Deepgram sends the last results after CloseStream, then closes. This bounds a socket that never does.
private const val CLOSE_WAIT_MS = 5_000L

/**
 * One recording: the mic, the Deepgram socket, and the audio waiting for it.
 * Every socket gets a fresh token from [tokenUrl], and after a drop the next one replays the audio Deepgram had not finalized.
 * Audio lives only in memory.
 */
class Recorder(
    private val store: LineStore,
    private val source: PcmSource,
    private val http: OkHttpClient,
    private val tokenUrl: String,
    @Volatile var auth: Auth,
    startedAtMs: Long,
    private val emit: (JSONObject) -> Unit,
    private val onEnd: () -> Unit,
) {
    private val lock = Any()
    private val queue = AudioQueue()
    private val offsetMs = System.currentTimeMillis() - startedAtMs
    private val scheduler = Executors.newSingleThreadScheduledExecutor()
    private val closed = CountDownLatch(1)
    private var captured = 0L
    private var ws: WebSocket? = null
    private var open = false
    /** Where this socket's audio starts, in ms since the recording started. Deepgram's times count from it. */
    private var socketStart: Long? = null
    private var st = Utterance()
    private var attempt = 0

    @Volatile var stopped = false
        private set
    @Volatile var state = "connecting"
        private set

    private val capture = thread(start = false, name = "yogurt-capture") {
        val buf = ByteArray(CHUNK_BYTES)
        try {
            while (!stopped) {
                val n = source.read(buf)
                if (n < 0) break
                if (n > 0) audio(buf.copyOf(n))
            }
        } catch (e: Exception) {
            if (!stopped) event("error", "message", "The microphone stopped: ${e.message}")
        } finally {
            source.close()
        }
        if (!stopped) thread { stop() }
    }

    fun start() {
        capture.start()
        scheduler.execute(::connect)
    }

    /** Ends the recording once Deepgram has sent its last lines, so a caller that reads the lines next sees them all. */
    fun stop() {
        synchronized(lock) {
            if (stopped) return
            stopped = true
        }
        if (Thread.currentThread() != capture) capture.join(1_000)
        val live = synchronized(lock) { if (open) ws else null }
        val clean = live != null && live.send("""{"type":"CloseStream"}""") && closed.await(CLOSE_WAIT_MS, TimeUnit.MILLISECONDS)
        synchronized(lock) {
            if (!clean) {
                heard(Deepgram.flush(st))
                val held = queue.pending()
                val dropped = queue.takeGap()
                if (held.isNotEmpty() || dropped != null) {
                    gap(Gap(dropped?.fromMs ?: held.first().tsMs, held.lastOrNull()?.endMs ?: dropped!!.toMs))
                }
            }
            val w = ws
            ws = null
            open = false
            w?.cancel()
            setState("stopped")
        }
        scheduler.shutdownNow()
        onEnd()
    }

    private fun audio(data: ByteArray) {
        synchronized(lock) {
            val c = Chunk(offsetMs + captured / BYTES_PER_MS, data)
            captured += data.size
            queue.push(c)
            if (open) send(ws!!, c)
        }
    }

    private fun send(w: WebSocket, c: Chunk) {
        if (socketStart == null) socketStart = c.tsMs
        w.send(c.data.toByteString())
    }

    private fun connect() {
        if (stopped) return
        synchronized(lock) { setState(if (attempt > 0) "reconnecting" else "connecting") }
        val grant = try {
            mint()
        } catch (e: Exception) {
            event("error", "message", e.message ?: "Couldn't get a transcription token.")
            return retry()
        }
        val req = Request.Builder().url(grant.second).header("Authorization", "Bearer ${grant.first}").build()
        synchronized(lock) {
            if (!stopped) ws = http.newWebSocket(req, Listener())
        }
    }

    /** A Deepgram token and listen URL from the Cloud, which checks the caller is its signed-in owner. */
    private fun mint(): Pair<String, String> {
        val a = auth
        val body = JSONObject().put("token", a.browserToken).toString().toRequestBody("application/json".toMediaType())
        val req = Request.Builder().url(tokenUrl).header("Authorization", "Bearer ${a.jwt}").post(body).build()
        http.newCall(req).execute().use { r ->
            val j = runCatching { JSONObject(r.body.string()) }.getOrDefault(JSONObject())
            if (!r.isSuccessful) throw IOException(j.optString("message").ifEmpty { "Your Cloud answered ${r.code}." })
            val token = j.optString("token")
            val url = j.optString("url")
            if (token.isEmpty() || url.isEmpty()) throw IOException("Your Cloud returned no transcription token.")
            return token to url
        }
    }

    private fun retry() {
        if (stopped) return
        synchronized(lock) { setState("reconnecting") }
        val wait = minOf(1_000L shl attempt.coerceAtMost(4), MAX_BACKOFF_MS)
        attempt++
        try {
            scheduler.schedule(::connect, wait, TimeUnit.MILLISECONDS)
        } catch (_: RejectedExecutionException) {
            // Stopped meanwhile.
        }
    }

    private inner class Listener : WebSocketListener() {
        override fun onOpen(webSocket: WebSocket, response: Response) {
            synchronized(lock) {
                if (webSocket !== ws) return
                attempt = 0
                st = Utterance()
                socketStart = null
                queue.takeGap()?.let(::gap)
                for (c in queue.pending()) send(webSocket, c)
                open = true
                setState("live")
            }
        }

        override fun onMessage(webSocket: WebSocket, text: String) {
            synchronized(lock) {
                if (webSocket !== ws) return
                val v = runCatching { JSONObject(text) }.getOrNull() ?: return
                val start = socketStart
                Deepgram.finalEndMs(v)?.let { if (start != null) queue.confirm(start + it) }
                heard(Deepgram.fold(st, v))
            }
        }

        override fun onClosing(webSocket: WebSocket, code: Int, reason: String) {
            webSocket.close(1000, null)
        }

        override fun onClosed(webSocket: WebSocket, code: Int, reason: String) = ended(webSocket, null)

        override fun onFailure(webSocket: WebSocket, t: Throwable, response: Response?) = ended(webSocket, response)
    }

    private fun ended(w: WebSocket, refused: Response?) {
        synchronized(lock) {
            if (w !== ws) return
            heard(Deepgram.flush(st))
            event("interim", "text", "")
            open = false
            ws = null
            if (stopped) return closed.countDown()
            if (refused != null) event("error", "message", "Deepgram refused the connection (${refused.code}).")
        }
        retry()
    }

    private fun heard(h: Heard?) {
        if (h == null) return
        if (!h.final) return event("interim", "text", h.text)
        val line = JSONObject().put("channel", "them").put("text", h.text).put("ts_ms", (socketStart ?: 0) + h.tsMs)
        val index = store.append(line)
        emit(JSONObject().put("type", "line").put("index", index).put("line", line))
        event("interim", "text", "")
    }

    private fun gap(g: Gap) = emit(JSONObject().put("type", "gap").put("from_ms", g.fromMs).put("to_ms", g.toMs))

    private fun setState(s: String) {
        state = s
        event("state", "state", s)
    }

    private fun event(type: String, key: String, value: String) = emit(JSONObject().put("type", type).put(key, value))
}
