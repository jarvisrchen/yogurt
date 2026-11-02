package com.jarvisrchen.yogurt

import android.annotation.SuppressLint
import android.media.AudioFormat
import android.media.AudioRecord
import android.media.MediaRecorder

/** 16 kHz mono linear16 audio, read in real time on the recorder's capture thread. */
interface PcmSource {
    /** Fills [buf] and returns the bytes read, or -1 when the audio has ended. */
    fun read(buf: ByteArray): Int
    fun close()
}

class MicSource private constructor(private val rec: AudioRecord) : PcmSource {
    override fun read(buf: ByteArray) = rec.read(buf, 0, buf.size).let { if (it < 0) -1 else it }

    override fun close() {
        runCatching { rec.stop() }
        rec.release()
    }

    companion object {
        /** Opens the mic, or throws when Android will not hand it over. The caller has checked RECORD_AUDIO. */
        @SuppressLint("MissingPermission")
        fun open(): MicSource {
            val min = AudioRecord.getMinBufferSize(16_000, AudioFormat.CHANNEL_IN_MONO, AudioFormat.ENCODING_PCM_16BIT)
            // VOICE_RECOGNITION skips the call-style processing MIC applies, which is tuned for a handset at the ear.
            val rec = AudioRecord(
                MediaRecorder.AudioSource.VOICE_RECOGNITION, 16_000, AudioFormat.CHANNEL_IN_MONO,
                AudioFormat.ENCODING_PCM_16BIT, maxOf(min, 32_000),
            )
            if (rec.state != AudioRecord.STATE_INITIALIZED) {
                rec.release()
                error("The microphone is not available.")
            }
            rec.startRecording()
            return MicSource(rec)
        }
    }
}
