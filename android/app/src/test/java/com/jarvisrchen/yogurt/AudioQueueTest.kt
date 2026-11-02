package com.jarvisrchen.yogurt

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class AudioQueueTest {
    /** 100 ms whose every byte is [tag], so a test can tell chunks apart. */
    private fun chunk(tsMs: Long, tag: Int = (tsMs / 100).toInt()) = Chunk(tsMs, ByteArray(100 * BYTES_PER_MS) { tag.toByte() })

    @Test
    fun replays_held_chunks_in_order() {
        val q = AudioQueue()
        for (t in 0L until 1_000L step 100) q.push(chunk(t))
        assertEquals((0L until 1_000L step 100).toList(), q.pending().map { it.tsMs })
        assertNull(q.takeGap())
    }

    @Test
    fun confirm_drops_finalized_audio_and_cuts_a_straddling_chunk() {
        val q = AudioQueue()
        for (t in 0L until 500L step 100) q.push(chunk(t))
        q.confirm(250)
        val held = q.pending()
        assertEquals(listOf(250L, 300L, 400L), held.map { it.tsMs })
        assertEquals(50 * BYTES_PER_MS, held[0].data.size)
        assertEquals(2.toByte(), held[0].data[0])
        assertEquals(250L, q.heldMs)
        q.confirm(10_000)
        assertEquals(0, q.pending().size)
        assertEquals(0L, q.heldMs)
    }

    @Test
    fun past_the_cap_the_oldest_drop_into_one_gap() {
        val q = AudioQueue(capMs = 1_000)
        for (t in 0L until 1_500L step 100) q.push(chunk(t))
        assertEquals(1_000L, q.heldMs)
        assertEquals(500L, q.pending().first().tsMs)
        assertEquals(Gap(0, 500), q.takeGap())
        assertNull(q.takeGap())
        q.push(chunk(1_500))
        assertEquals(Gap(500, 600), q.takeGap())
    }

    @Test
    fun the_default_cap_is_five_minutes() {
        val q = AudioQueue()
        for (t in 0L until 301_000L step 100) q.push(chunk(t))
        assertEquals(300_000L, q.heldMs)
        assertEquals(Gap(0, 1_000), q.takeGap())
    }
}
