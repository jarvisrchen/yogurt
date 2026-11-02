package com.jarvisrchen.yogurt

import org.json.JSONArray
import org.json.JSONObject
import java.io.File

/**
 * The current recording's final lines, numbered from 0, in a file that outlives the service and the process until the next [reset].
 * A page that comes back asks for the lines from the count it holds, so it never misses or repeats one.
 */
class LineStore(dir: File) {
    private val meta = File(dir, "recording.json")
    private val file = File(dir, "lines.jsonl")
    private val lines = ArrayList<String>()

    var meetingId: String? = null
        private set
    var startedAtMs: Long = 0
        private set

    init {
        dir.mkdirs()
        runCatching { JSONObject(meta.readText()) }.getOrNull()?.let {
            meetingId = it.optString("meetingId")
            startedAtMs = it.optLong("startedAtMs")
        }
        if (file.exists()) {
            // A line cut short by a killed process is the only one that can fail to parse, and it is the last.
            file.forEachLine { l -> if (runCatching { JSONObject(l) }.isSuccess) lines.add(l) }
        }
    }

    @Synchronized
    fun reset(meetingId: String, startedAtMs: Long) {
        this.meetingId = meetingId
        this.startedAtMs = startedAtMs
        lines.clear()
        file.writeText("")
        meta.writeText(JSONObject().put("meetingId", meetingId).put("startedAtMs", startedAtMs).toString())
    }

    /** Appends a line and returns its index. */
    @Synchronized
    fun append(line: JSONObject): Int {
        val s = line.toString()
        file.appendText(s + "\n")
        lines.add(s)
        return lines.size - 1
    }

    @Synchronized
    fun count() = lines.size

    @Synchronized
    fun from(index: Int): JSONArray {
        val out = JSONArray()
        for (i in index.coerceAtLeast(0) until lines.size) out.put(JSONObject(lines[i]))
        return out
    }
}
