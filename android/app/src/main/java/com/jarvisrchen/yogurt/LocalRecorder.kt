package com.jarvisrchen.yogurt

import com.k2fsa.sherpa.onnx.OfflineModelConfig
import com.k2fsa.sherpa.onnx.OfflineRecognizer
import com.k2fsa.sherpa.onnx.OfflineRecognizerConfig
import com.k2fsa.sherpa.onnx.OfflineTransducerModelConfig
import com.k2fsa.sherpa.onnx.OfflineWhisperModelConfig
import com.k2fsa.sherpa.onnx.SileroVadModelConfig
import com.k2fsa.sherpa.onnx.Vad
import com.k2fsa.sherpa.onnx.VadModelConfig
import org.json.JSONObject
import java.io.File
import java.util.concurrent.CountDownLatch
import kotlin.concurrent.thread

private const val SAMPLES_PER_MS = 16
private const val VAD_WINDOW = 512
// Whisper reads at most 30 seconds at once, and a shorter stretch keeps lines coming during a monologue.
private const val MAX_SPEECH_S = 20f
// Speech waiting for the model past this drops its oldest stretch, which the page shows as a gap. The page waits for the rest at End.
private const val MAX_BACKLOG_MS = 10 * 60_000L
private const val BACKLOG_STEP_MS = 1_000L
private const val PRE_ROLL_MS = 300

/** A stretch of speech the voice detector cut at a pause, stamped in ms since the recording started. */
class Speech(val tsMs: Long, val samples: FloatArray) {
    val ms get() = samples.size / SAMPLES_PER_MS.toLong()
}

/**
 * Speech waiting for the model, oldest first. Past [capMs] the oldest stretch drops and [gaps] gets its span.
 * The size is what the page shows as how far behind transcription is.
 */
class SpeechQueue(private val capMs: Long = MAX_BACKLOG_MS) {
    private val items = ArrayDeque<Speech>()
    private val lock = java.lang.Object()
    private var stopped = false
    @Volatile var heldMs = 0L
        private set

    fun push(s: Speech, gaps: (Gap) -> Unit) = synchronized(lock) {
        items.addLast(s)
        heldMs += s.ms
        while (heldMs > capMs && items.size > 1) {
            val old = items.removeFirst()
            heldMs -= old.ms
            gaps(Gap(old.tsMs, old.tsMs + old.ms))
        }
        lock.notifyAll()
    }

    /** The next stretch, waiting for one, or null once [close] has run and nothing is left. */
    fun take(): Speech? = synchronized(lock) {
        while (items.isEmpty() && !stopped) lock.wait()
        items.removeFirstOrNull()?.also { heldMs -= it.ms }
    }

    fun close() = synchronized(lock) {
        stopped = true
        lock.notifyAll()
    }
}

/**
 * The audio last fed to the voice detector, which starts a stretch a syllable late.
 * [before] gives back the moments just ahead of a stretch, so its first word is whole.
 */
class PreRoll(capacity: Int, private val lead: Int) {
    private val ring = FloatArray(capacity)
    private var fed = 0L

    fun add(w: FloatArray) {
        for (x in w) ring[(fed++ % ring.size).toInt()] = x
    }

    /** Up to [lead] samples ahead of [start], counted from the first sample added, as far as the ring still holds them. */
    fun before(start: Long): FloatArray {
        val from = maxOf(start - lead, fed - ring.size, 0L)
        return FloatArray((start - from).coerceAtLeast(0L).toInt()) { ring[((from + it) % ring.size).toInt()] }
    }
}

// Parakeet's "$" token has no word break ahead of it, so "is $18" would read "is$18".
fun spaced(text: String) = text.trim().replace(Regex("""(?<=\w)\$""")) { " \$" }

fun pcmToFloats(pcm: ByteArray, n: Int): FloatArray =
    FloatArray(n / 2) { i -> ((pcm[2 * i + 1].toInt() shl 8) or (pcm[2 * i].toInt() and 0xff)).toShort() / 32768f }

/**
 * One recording transcribed on the phone: Silero cuts the mic's audio at pauses, and Whisper or Parakeet turns each stretch into one line.
 * The detector runs on the capture thread so the mic is read in real time, and the model on its own thread, with speech waiting in [queue] meanwhile.
 * Audio lives only in memory.
 */
class LocalRecorder(
    private val store: LineStore,
    private val meetingId: String,
    private val source: PcmSource,
    val spec: ModelSpec,
    private val dir: File,
    startedAtMs: Long,
    private val emit: (JSONObject) -> Unit,
    private val onEnd: () -> Unit,
) : Rec {
    private val offsetMs = System.currentTimeMillis() - startedAtMs
    private val queue = SpeechQueue()
    // Holds the longest stretch the detector makes, plus its closing pause and the lead.
    private val preRoll = PreRoll(((MAX_SPEECH_S + 2) * 16_000).toInt(), PRE_ROLL_MS * SAMPLES_PER_MS)
    private var vad: Vad? = null
    private var speaking = false
    private var lastBacklog = 0L
    @Volatile private var ending = false
    private val finished = CountDownLatch(1)

    /** True once the model has finished the speech heard before End, which can take minutes after [stop] is called. */
    @Volatile override var stopped = false
        private set
    @Volatile override var state = "connecting"
        private set

    private val capture = thread(start = false, name = "yogurt-capture") {
        val buf = ByteArray(VAD_WINDOW * 2 * 4)
        val v = try {
            Vad(null, vadConfig(dir)).also { vad = it }
        } catch (e: Throwable) {
            source.close()
            event("error", "message", "${spec.name} could not start on this phone: ${e.message}")
            thread { stop() }
            return@thread
        }
        try {
            var pending = FloatArray(0)
            while (!ending) {
                val n = source.read(buf)
                if (n < 0) break
                pending += pcmToFloats(buf, n)
                var at = 0
                while (pending.size - at >= VAD_WINDOW) {
                    val w = pending.copyOfRange(at, at + VAD_WINDOW)
                    v.acceptWaveform(w)
                    preRoll.add(w)
                    at += VAD_WINDOW
                    drain(v)
                }
                pending = pending.copyOfRange(at, pending.size)
            }
        } catch (e: Throwable) {
            if (!ending) event("error", "message", "The microphone stopped: ${e.message}")
        } finally {
            source.close()
        }
        if (!ending) thread { stop() }
    }

    private val decoder = thread(start = false, name = "yogurt-decode") {
        val rec = try {
            OfflineRecognizer(null, recognizerConfig(spec, dir))
        } catch (e: Throwable) {
            event("error", "message", "${spec.name} could not start on this phone: ${e.message}")
            queue.close()
            thread { stop() }
            return@thread
        }
        setState("live")
        try {
            while (true) {
                val s = queue.take() ?: break
                // A stretch the model chokes on is reported and skipped, since an error escaping this thread would end the app and the meeting.
                try {
                    val stream = rec.createStream()
                    val text = try {
                        stream.acceptWaveform(s.samples, 16_000)
                        rec.decode(stream)
                        spaced(rec.getResult(stream).text)
                    } finally {
                        stream.release()
                    }
                    if (text.isNotEmpty()) line(text, s.tsMs)
                } catch (e: Throwable) {
                    event("error", "message", "${spec.name} could not transcribe the speech at ${s.tsMs / 1000}s: ${e.message}")
                }
                backlog()
            }
        } finally {
            rec.release()
        }
    }

    override fun start() {
        capture.start()
        decoder.start()
    }

    /**
     * Waits for the model to finish the speech already heard, so a caller that reads the lines next sees them all.
     * A second caller, such as the notification's Stop during that wait, waits for the same finish.
     */
    override fun stop() {
        synchronized(this) {
            if (ending) return finished.await()
            ending = true
        }
        if (Thread.currentThread() != capture) capture.join(1_000)
        // A capture thread still stuck in a read owns the detector, so its last half-heard stretch is given up.
        if (!capture.isAlive) {
            vad?.let { v ->
                v.flush()
                drain(v)
                v.release()
            }
        }
        queue.close()
        if (Thread.currentThread() != decoder) decoder.join()
        stopped = true
        setState("stopped")
        finished.countDown()
        onEnd()
    }

    private fun drain(v: Vad) {
        while (!v.empty()) {
            val seg = v.front()
            v.pop()
            val lead = preRoll.before(seg.start)
            queue.push(Speech(offsetMs + (seg.start - lead.size) / SAMPLES_PER_MS, lead + seg.samples)) { g ->
                emit(JSONObject().put("type", "gap").put("from_ms", g.fromMs).put("to_ms", g.toMs))
            }
        }
        val now = v.isSpeechDetected()
        if (now != speaking) {
            speaking = now
            emit(JSONObject().put("type", "speaking").put("speaking", now))
        }
        backlog()
    }

    /** How much heard speech the model has not reached, sent when it moves by a second. */
    private fun backlog() {
        val ms = queue.heldMs
        synchronized(this) {
            if (Math.abs(ms - lastBacklog) < BACKLOG_STEP_MS && !(ms == 0L && lastBacklog != 0L)) return
            lastBacklog = ms
        }
        emit(JSONObject().put("type", "backlog").put("ms", ms))
    }

    private fun line(text: String, tsMs: Long) {
        val line = JSONObject().put("channel", "them").put("text", text).put("ts_ms", tsMs)
        val index = store.append(meetingId, line) ?: return
        emit(JSONObject().put("type", "line").put("meetingId", meetingId).put("index", index).put("line", line))
    }

    private fun setState(s: String) {
        state = s
        event("state", "state", s)
    }

    private fun event(type: String, key: String, value: String) = emit(JSONObject().put("type", type).put(key, value))

    companion object {
        private val threads = Runtime.getRuntime().availableProcessors().coerceIn(1, 4)

        fun vadConfig(dir: File) = VadModelConfig().apply {
            sileroVadModelConfig = SileroVadModelConfig().apply {
                model = File(dir, "silero_vad.onnx").path
                minSilenceDuration = 0.5f
                minSpeechDuration = 0.25f
                maxSpeechDuration = MAX_SPEECH_S
                windowSize = VAD_WINDOW
            }
            sampleRate = 16_000
            numThreads = 1
        }

        fun recognizerConfig(spec: ModelSpec, dir: File) = OfflineRecognizerConfig().apply {
            modelConfig = OfflineModelConfig().apply {
                numThreads = threads
                tokens = spec.path(dir, "tokens.txt")
                when (spec.kind) {
                    ModelKind.WHISPER -> whisper = OfflineWhisperModelConfig().apply {
                        encoder = spec.path(dir, "encoder.int8.onnx")
                        decoder = spec.path(dir, "decoder.int8.onnx")
                        language = "en"
                        task = "transcribe"
                    }
                    ModelKind.TRANSDUCER -> {
                        transducer = OfflineTransducerModelConfig().apply {
                            encoder = spec.path(dir, "encoder.int8.onnx")
                            decoder = spec.path(dir, "decoder.int8.onnx")
                            joiner = spec.path(dir, "joiner.int8.onnx")
                        }
                        modelType = "nemo_transducer"
                    }
                }
            }
        }

        /** Loads [spec] and decodes a second of silence, so a broken model shows in Settings and not when a meeting starts. Returns the error, or null. */
        fun selfTest(spec: ModelSpec, dir: File): String? = try {
            val rec = OfflineRecognizer(null, recognizerConfig(spec, dir))
            try {
                val stream = rec.createStream()
                try {
                    stream.acceptWaveform(FloatArray(16_000), 16_000)
                    rec.decode(stream)
                } finally {
                    stream.release()
                }
            } finally {
                rec.release()
            }
            null
        } catch (e: Throwable) {
            e.message ?: e.toString()
        }
    }
}
