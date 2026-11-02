package com.jarvisrchen.yogurt

import org.json.JSONObject

/** One socket's utterance in progress, a port of `UtteranceState` in crates/yogurt-stt/src/deepgram.rs. */
class Utterance {
    var buf = ""
    var startMs: Long? = null
    /** The last line emitted, since Deepgram can resend the closing frame. */
    var lastFinal: String? = null
}

/** Text and start in ms of the socket's audio. A final is one transcript line, anything else is the words so far. */
data class Heard(val text: String, val tsMs: Long, val final: Boolean)

object Deepgram {
    private fun join(a: String, b: String) = if (a.isNotEmpty() && b.isNotEmpty()) "$a $b" else a + b

    /**
     * Folds one frame into [st] as `fold_deepgram_event` does: an interim shows the buffered text plus the live window,
     * a window-final joins the buffer, and `speech_final` closes the utterance into one line.
     */
    fun fold(st: Utterance, v: JSONObject): Heard? {
        if (v.optString("type") != "Results") return null
        val said = v.optJSONObject("channel")?.optJSONArray("alternatives")?.optJSONObject(0)?.opt("transcript") as? String
            ?: return null
        val text = said.trim()
        val windowFinal = v.optBoolean("is_final")
        val speechFinal = if (v.has("speech_final")) v.optBoolean("speech_final") else windowFinal
        val now = Math.round(v.optDouble("start", 0.0) * 1000)
        if (text.isEmpty() && !speechFinal) return null
        if (st.startMs == null && text.isNotEmpty()) st.startMs = now
        val tsMs = st.startMs ?: now
        if (speechFinal) {
            val full = join(st.buf, text)
            st.buf = ""
            st.startMs = null
            if (full.isEmpty() || full == st.lastFinal) return null
            st.lastFinal = full
            return Heard(full, tsMs, true)
        }
        if (windowFinal) {
            st.buf = join(st.buf, text)
            return Heard(st.buf, tsMs, false)
        }
        return Heard(join(st.buf, text), tsMs, false)
    }

    /** The window-finalized words of an utterance whose socket closed, as a line. Deepgram has finished those words. */
    fun flush(st: Utterance): Heard? {
        val text = st.buf
        val tsMs = st.startMs ?: 0
        st.buf = ""
        st.startMs = null
        if (text.isEmpty() || text == st.lastFinal) return null
        st.lastFinal = text
        return Heard(text, tsMs, true)
    }

    /** Where a finalized window ends, in ms of the socket's audio. Audio before it never needs sending again. */
    fun finalEndMs(v: JSONObject): Long? {
        if (v.optString("type") != "Results" || !v.optBoolean("is_final")) return null
        return Math.round((v.optDouble("start", 0.0) + v.optDouble("duration", 0.0)) * 1000)
    }
}
