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
        s.reset("m1", 1_000)
        assertEquals(0, s.append(line("one", 0)))
        assertEquals(1, s.append(line("two", 2_000)))
        assertEquals(2, s.append(line("three", 4_000)))
        assertEquals(3, s.count())
        val tail = s.from(1)
        assertEquals(2, tail.length())
        assertEquals("two", tail.getJSONObject(0).getString("text"))
        assertEquals(2_000L, tail.getJSONObject(0).getLong("ts_ms"))
        assertEquals(0, s.from(3).length())
        assertEquals(3, s.from(-5).length())
    }

    @Test
    fun survives_a_restart_until_the_next_reset() {
        LineStore(tmp.root).apply {
            reset("m1", 1_000)
            append(line("one", 0))
            append(line("two", 2_000))
        }
        val again = LineStore(tmp.root)
        assertEquals("m1", again.meetingId)
        assertEquals(1_000L, again.startedAtMs)
        assertEquals(2, again.count())
        assertEquals(2, again.append(line("three", 4_000)))
        again.reset("m2", 5_000)
        val fresh = LineStore(tmp.root)
        assertEquals("m2", fresh.meetingId)
        assertEquals(0, fresh.count())
    }

    @Test
    fun skips_a_last_line_cut_short_by_a_killed_process() {
        LineStore(tmp.root).apply {
            reset("m1", 1_000)
            append(line("one", 0))
        }
        tmp.root.resolve("lines.jsonl").appendText("{\"channel\":\"them\",\"te")
        assertEquals(1, LineStore(tmp.root).count())
    }
}
