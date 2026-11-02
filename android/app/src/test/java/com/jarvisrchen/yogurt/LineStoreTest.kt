package com.jarvisrchen.yogurt

import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder

class LineStoreTest {
    @get:Rule val tmp = TemporaryFolder()

    private fun line(text: String, ts: Long) = JSONObject().put("channel", "them").put("text", text).put("ts_ms", ts)

    @Test
    fun numbers_lines_from_zero_and_serves_from_an_index() {
        val s = LineStore(tmp.root)
        s.begin("m1", 1_000, 0)
        assertEquals(0, s.append("m1", line("one", 0)))
        assertEquals(1, s.append("m1", line("two", 2_000)))
        assertEquals(2, s.append("m1", line("three", 4_000)))
        assertEquals(3, s.count())
        val tail = s.from("m1", 1)
        assertEquals(2, tail.length())
        assertEquals("two", tail.getJSONObject(0).getString("text"))
        assertEquals(2_000L, tail.getJSONObject(0).getLong("ts_ms"))
        assertEquals(0, s.from("m1", 3).length())
        assertEquals(3, s.from("m1", -5).length())
    }

    @Test
    fun serves_and_takes_lines_only_for_its_own_meeting() {
        val s = LineStore(tmp.root)
        s.begin("m1", 1_000, 0)
        s.append("m1", line("one", 0))
        assertEquals(0, s.from("m2", 0).length())
        assertEquals(null, s.append("m2", line("late", 0)))
        assertEquals(1, s.count())
    }

    @Test
    fun a_new_meeting_numbers_from_the_lines_the_page_holds() {
        val s = LineStore(tmp.root)
        s.begin("m1", 1_000, 0)
        s.append("m1", line("old", 0))
        s.begin("m2", 2_000, 5)
        assertEquals(5, s.count())
        assertEquals(5, s.append("m2", line("five", 0)))
        assertEquals("five", s.from("m2", 0).getJSONObject(0).getString("text"))
        assertEquals(1, s.from("m2", 5).length())
        assertEquals(0, s.from("m2", 6).length())
        assertEquals(6, LineStore(tmp.root).count())
    }

    @Test
    fun starting_the_same_meeting_again_carries_on_its_numbering() {
        val s = LineStore(tmp.root)
        s.begin("m1", 1_000, 0)
        s.append("m1", line("one", 0))
        s.append("m1", line("two", 2_000))
        // The page holds only the first, as when the last line came in while it reloaded.
        s.begin("m1", 1_000, 1)
        assertEquals(2, s.count())
        assertEquals(2, s.append("m1", line("three", 4_000)))
        assertEquals(listOf("two", "three"), s.from("m1", 1).let { a -> (0 until a.length()).map { a.getJSONObject(it).getString("text") } })
    }

    @Test
    fun survives_a_restart_until_another_meeting_begins() {
        LineStore(tmp.root).apply {
            begin("m1", 1_000, 0)
            append("m1", line("one", 0))
            append("m1", line("two", 2_000))
        }
        val again = LineStore(tmp.root)
        assertEquals("m1", again.meetingId)
        assertEquals(1_000L, again.startedAtMs)
        assertEquals(2, again.count())
        assertEquals(2, again.append("m1", line("three", 4_000)))
        again.begin("m2", 5_000, 0)
        val fresh = LineStore(tmp.root)
        assertEquals("m2", fresh.meetingId)
        assertEquals(0, fresh.count())
    }

    @Test
    fun skips_a_last_line_cut_short_by_a_killed_process() {
        LineStore(tmp.root).apply {
            begin("m1", 1_000, 0)
            append("m1", line("one", 0))
        }
        tmp.root.resolve("lines.jsonl").appendText("{\"channel\":\"them\",\"te")
        assertEquals(1, LineStore(tmp.root).count())
    }
}
