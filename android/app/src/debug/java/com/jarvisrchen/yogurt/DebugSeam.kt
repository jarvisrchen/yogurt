package com.jarvisrchen.yogurt

import android.content.Context
import android.content.Intent
import java.io.File
import java.io.RandomAccessFile

/**
 * Test hooks that exist only in debug builds. Launch extras, kept until changed:
 * `--es wav <file in the app's files dir>` records from that 16 kHz mono wav instead of the mic ("" for the mic again),
 * and `--es releasesUrl <url>` checks a fake GitHub release ("" for the real one).
 */
object DebugSeam {
    private fun prefs(c: Context) = c.getSharedPreferences("debug", Context.MODE_PRIVATE)

    fun onLaunch(c: Context, intent: Intent?) {
        val e = prefs(c).edit()
        for (key in listOf("wav", "releasesUrl")) intent?.getStringExtra(key)?.let { e.putString(key, it) }
        e.apply()
    }

    fun pcmSource(c: Context): PcmSource? =
        prefs(c).getString("wav", "")?.takeIf { it.isNotEmpty() }?.let { WavSource(File(c.filesDir, it)) }

    fun releasesUrl(c: Context): String? = prefs(c).getString("releasesUrl", "")?.takeIf { it.isNotEmpty() }
}

/** Plays a wav's PCM at the mic's pace, so a test runs like a real recording. */
private class WavSource(file: File) : PcmSource {
    private val f = RandomAccessFile(file, "r")
    private var read = 0L
    private val began = System.nanoTime()

    init {
        // Walk the RIFF chunks to the start of "data".
        f.seek(12)
        while (true) {
            val id = ByteArray(4).also { f.readFully(it) }
            val size = Integer.reverseBytes(f.readInt()).toLong() and 0xffffffffL
            if (String(id) == "data") break
            f.seek(f.filePointer + size)
        }
    }

    override fun read(buf: ByteArray): Int {
        val n = f.read(buf)
        if (n <= 0) return -1
        read += n
        val due = began + read * 1_000_000L / BYTES_PER_MS
        val wait = due - System.nanoTime()
        if (wait > 0) Thread.sleep(wait / 1_000_000, (wait % 1_000_000).toInt())
        return n
    }

    override fun close() = f.close()
}
