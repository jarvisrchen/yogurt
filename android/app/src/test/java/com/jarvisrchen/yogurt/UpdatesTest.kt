package com.jarvisrchen.yogurt

import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class UpdatesTest {
    @Test
    fun compares_versions_by_number() {
        assertTrue(Updates.newer("0.2.0", "0.1.0"))
        assertTrue(Updates.newer("0.10.0", "0.9.9"))
        assertTrue(Updates.newer("1.0", "0.99.99"))
        assertTrue(Updates.newer("0.1.1", "0.1"))
        assertFalse(Updates.newer("0.1.0", "0.1.0"))
        assertFalse(Updates.newer("0.1", "0.1.0"))
        assertFalse(Updates.newer("0.0.9", "0.1.0"))
        assertFalse(Updates.newer("0.2.0-rc1", "0.1.0"))
    }

    private fun release(vararg names: String) = JSONObject().put(
        "assets",
        JSONArray(names.map { JSONObject().put("name", it).put("browser_download_url", "https://example.test/$it") }),
    )

    @Test
    fun picks_the_apk_only_when_newer() {
        val r = release("yogurt-aarch64-apple-darwin.tar.gz", "yogurt-android-0.2.0.apk", "SHA256SUMS")
        val u = Updates.pick(r, "0.1.0")!!
        assertEquals("0.2.0", u.version)
        assertEquals("https://example.test/yogurt-android-0.2.0.apk", u.url)
        assertNull(Updates.pick(r, "0.2.0"))
        assertNull(Updates.pick(release("yogurt-android-latest.apk", "yogurt-android-0.3.0.apk.sha256"), "0.1.0"))
        assertNull(Updates.pick(JSONObject(), "0.1.0"))
    }
}
