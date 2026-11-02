package com.jarvisrchen.yogurt

import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/** The same cases as crates/yogurt-stt/src/deepgram.rs `utterance_tests`, so the phone and the Mac cut lines alike. */
class DeepgramTest {
    private fun frame(text: String, start: Double, windowFinal: Boolean, speechFinal: Boolean, duration: Double = 1.0) = JSONObject(
        """{"type":"Results","channel":{"alternatives":[{"transcript":"$text"}]},"is_final":$windowFinal,"speech_final":$speechFinal,"start":$start,"duration":$duration}""",
    )

    @Test
    fun a_multi_window_utterance_joins_all_its_text() {
        val st = Utterance()
        assertEquals(Heard("we wanna isolate", 4_000, false), Deepgram.fold(st, frame("we wanna isolate", 4.0, false, false)))
        assertEquals(Heard("we wanna isolate them longer", 4_000, false), Deepgram.fold(st, frame("we wanna isolate them longer", 4.0, true, false)))
        assertEquals(Heard("we wanna isolate them longer from the changes", 4_000, false), Deepgram.fold(st, frame("from the changes", 9.5, false, false)))
        assertEquals(
            Heard("we wanna isolate them longer from the changes we introduce", 4_000, true),
            Deepgram.fold(st, frame("from the changes we introduce", 9.5, true, true)),
        )
        assertEquals(Heard("next thought", 12_000, false), Deepgram.fold(st, frame("next thought", 12.0, false, false)))
    }

    @Test
    fun an_empty_speech_final_flushes_the_buffer() {
        val st = Utterance()
        Deepgram.fold(st, frame("hello there", 1.0, true, false))
        assertEquals(Heard("hello there", 1_000, true), Deepgram.fold(st, frame("", 2.0, true, true)))
    }

    @Test
    fun a_resent_speech_final_is_dropped() {
        val st = Utterance()
        assertEquals(Heard("hello there", 4_000, true), Deepgram.fold(st, frame("hello there", 4.0, true, true)))
        assertNull(Deepgram.fold(st, frame("hello there", 4.0, true, true)))
        assertNull(Deepgram.fold(st, frame("hello there", 0.0, true, true)))
    }

    @Test
    fun is_final_locks_a_line_only_when_speech_final_is_absent() {
        val st = Utterance()
        val noSpeechFinal = JSONObject("""{"type":"Results","channel":{"alternatives":[{"transcript":"hi"}]},"is_final":true,"start":0.5}""")
        assertEquals(Heard("hi", 500, true), Deepgram.fold(st, noSpeechFinal))
    }

    @Test
    fun other_frames_and_empty_interims_change_nothing() {
        val st = Utterance()
        assertNull(Deepgram.fold(st, JSONObject("""{"type":"Metadata","request_id":"x"}""")))
        assertNull(Deepgram.fold(st, frame("", 1.0, false, false)))
        assertEquals("", st.buf)
    }

    @Test
    fun flush_keeps_the_window_finalized_words_of_a_closed_socket() {
        val st = Utterance()
        Deepgram.fold(st, frame("half a", 3.0, true, false))
        Deepgram.fold(st, frame("sentence", 4.0, false, false))
        assertEquals(Heard("half a", 3_000, true), Deepgram.flush(st))
        assertNull(Deepgram.flush(st))
    }

    @Test
    fun a_finalized_window_ends_at_start_plus_duration() {
        assertEquals(6_500L, Deepgram.finalEndMs(frame("x", 4.0, true, false, duration = 2.5)))
        assertNull(Deepgram.finalEndMs(frame("x", 4.0, false, false)))
    }
}
