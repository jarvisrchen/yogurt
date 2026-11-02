package com.jarvisrchen.yogurt

/** 16 kHz mono linear16. */
const val BYTES_PER_MS = 32

/** PCM stamped with its start in ms since the recording started. */
class Chunk(val tsMs: Long, val data: ByteArray) {
    val endMs get() = tsMs + data.size / BYTES_PER_MS
}

data class Gap(val fromMs: Long, val toMs: Long)

/**
 * Audio Deepgram has not finalized yet: everything still queued for a socket, plus what was sent but not yet covered by a final result.
 * Holding the sent part means a dropped socket loses nothing: the next one replays from the last finalized window.
 * Past [capMs] the oldest audio drops, and [takeGap] reports the span it covered.
 */
class AudioQueue(private val capMs: Long = 5 * 60_000L) {
    private val chunks = ArrayDeque<Chunk>()
    private var heldBytes = 0L
    private var gap: Gap? = null

    val heldMs get() = heldBytes / BYTES_PER_MS

    fun push(c: Chunk) {
        chunks.addLast(c)
        heldBytes += c.data.size
        while (heldMs > capMs) {
            val old = chunks.removeFirst()
            heldBytes -= old.data.size
            gap = Gap(gap?.fromMs ?: old.tsMs, old.endMs)
        }
    }

    /** Drops the audio before [untilMs], cutting a chunk that straddles it. */
    fun confirm(untilMs: Long) {
        while (chunks.isNotEmpty()) {
            val head = chunks.first()
            if (head.endMs <= untilMs) {
                chunks.removeFirst()
                heldBytes -= head.data.size
            } else {
                if (head.tsMs < untilMs) {
                    val cut = ((untilMs - head.tsMs) * BYTES_PER_MS).toInt()
                    chunks[0] = Chunk(untilMs, head.data.copyOfRange(cut, head.data.size))
                    heldBytes -= cut
                }
                return
            }
        }
    }

    /** Every held chunk, oldest first, for a new socket to replay. */
    fun pending(): List<Chunk> = chunks.toList()

    fun takeGap(): Gap? = gap.also { gap = null }
}
