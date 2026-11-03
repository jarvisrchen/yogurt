package com.jarvisrchen.yogurt

import org.json.JSONArray
import org.json.JSONObject
import java.io.File

/**
 * One meeting's final lines, numbered for the whole meeting, in a file that outlives the service and the process until another meeting begins.
 * A meeting begins numbering at the count of lines the page already holds, and a start of the same meeting again carries on, so a resume keeps one numbering.
 * A page that comes back asks for the lines from the count it holds, so it never misses or repeats one.
 */
class LineStore(dir: File) {
    private val meta = File(dir, "recording.json")
    private val file = File(dir, "lines.jsonl")
    private val lines = ArrayList<String>()
    /** The index of the first line in [lines]. */
    private var base = 0

    var meetingId: String? = null
        private set
    var startedAtMs: Long = 0
        private set

    init {
        dir.mkdirs()
        runCatching { JSONObject(meta.readText()) }.getOrNull()?.let {
            meetingId = it.optString("meetingId")
            startedAtMs = it.optLong("startedAtMs")
            base = it.optInt("base")
        }
        if (file.exists()) {
            // A line cut short by a killed process is the only one that can fail to parse, and it is the last.
            file.forEachLine { l -> if (runCatching { JSONObject(l) }.isSuccess) lines.add(l) }
        }
    }

    /** Begins [meetingId] at index [have], unless this store already holds that meeting up to there, which then carries on. */
    @Synchronized
    fun begin(meetingId: String, startedAtMs: Long, have: Int) {
        if (meetingId == this.meetingId && have <= count()) return
        this.meetingId = meetingId
        this.startedAtMs = startedAtMs
        base = have.coerceAtLeast(0)
        lines.clear()
        file.writeText("")
        meta.writeText(JSONObject().put("meetingId", meetingId).put("startedAtMs", startedAtMs).put("base", base).toString())
    }

    /** Appends a line of [meetingId] and returns its index, or null when another meeting has begun since. */
    @Synchronized
    fun append(meetingId: String, line: JSONObject): Int? {
        if (meetingId != this.meetingId) return null
        val s = line.toString()
        file.appendText(s + "\n")
        lines.add(s)
        return base + lines.size - 1
    }

    /** The index the next line gets, which is how many lines the meeting has. */
    @Synchronized
    fun count() = base + lines.size

    @Synchronized
    fun from(meetingId: String, index: Int): JSONArray {
        val out = JSONArray()
        if (meetingId != this.meetingId) return out
        for (i in (index - base).coerceAtLeast(0) until lines.size) out.put(JSONObject(lines[i]))
        return out
    }
}
