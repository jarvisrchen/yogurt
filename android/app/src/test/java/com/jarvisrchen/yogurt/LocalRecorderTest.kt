package com.jarvisrchen.yogurt

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class LocalRecorderTest {
    private fun speech(tsMs: Long, ms: Int) = Speech(tsMs, FloatArray(ms * 16))

    @Test
    fun queueHandsOutSpeechInOrderAndCountsWhatWaits() {
        val q = SpeechQueue()
        q.push(speech(0, 1_000)) {}
        q.push(speech(2_000, 500)) {}
        assertEquals(1_500, q.heldMs)
        assertEquals(0, q.take()!!.tsMs)
        assertEquals(500, q.heldMs)
        q.close()
        assertEquals(2_000, q.take()!!.tsMs)
        assertNull(q.take())
    }

    @Test
    fun queuePastItsCapDropsTheOldestAsAGap() {
        val q = SpeechQueue(capMs = 2_000)
        val gaps = mutableListOf<Gap>()
        q.push(speech(0, 1_000), gaps::add)
        q.push(speech(1_000, 1_000), gaps::add)
        q.push(speech(5_000, 1_000), gaps::add)
        assertEquals(listOf(Gap(0, 1_000)), gaps)
        assertEquals(2_000, q.heldMs)
        assertEquals(1_000, q.take()!!.tsMs)
    }

    @Test
    fun queueKeepsOneStretchLongerThanItsCap() {
        val q = SpeechQueue(capMs = 1_000)
        val gaps = mutableListOf<Gap>()
        q.push(speech(0, 3_000), gaps::add)
        assertTrue(gaps.isEmpty())
        assertEquals(3_000, q.heldMs)
    }

    @Test
    fun preRollGivesBackTheLeadAheadOfAStretchWhileTheRingHoldsIt() {
        val p = PreRoll(capacity = 8, lead = 3)
        p.add(floatArrayOf(0f, 1f, 2f, 3f, 4f, 5f))
        assertEquals(listOf(2f, 3f, 4f), p.before(5).toList())
        assertEquals(listOf(0f), p.before(1).toList())
        p.add(floatArrayOf(6f, 7f, 8f, 9f, 10f, 11f))
        // Samples 0 to 3 are overwritten, so only 4 is left ahead of 5.
        assertEquals(listOf(4f), p.before(5).toList())
    }

    @Test
    fun aDollarSignGetsTheSpaceParakeetLeavesOut() {
        assertEquals("proposal is \$18 a month and \$180 a year.", spaced(" proposal is\$18 a month and\$180 a year. "))
        assertEquals("It costs \$5.", spaced("It costs \$5."))
    }

    @Test
    fun pcmBecomesFloatsInMinusOneToOne() {
        val pcm = byteArrayOf(0, 0, 0xff.toByte(), 0x7f, 0, 0x80.toByte(), 0xff.toByte(), 0xff.toByte())
        val f = pcmToFloats(pcm, pcm.size)
        assertEquals(listOf(0f, 32767 / 32768f, -1f, -1 / 32768f), f.toList())
    }

    @Test
    fun catalogIsAscendingBySizeWithTheVoiceDetectorInEveryModel() {
        assertEquals(listOf("tiny.en", "base.en", "small.en", "parakeet-v3"), CATALOG.map { it.name })
        assertEquals(CATALOG.sortedBy { it.bytes }, CATALOG)
        for (m in CATALOG) {
            assertTrue(m.files.any { it.name == "silero_vad.onnx" })
            assertTrue(m.files.all { it.sha256.length == 64 && it.url.startsWith("https://") })
        }
    }
}
