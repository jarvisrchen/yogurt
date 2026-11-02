package com.jarvisrchen.yogurt

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class CloudTest {
    @Test
    fun accepts_https_and_keeps_only_the_origin() {
        assertEquals("https://yogurt-example.vercel.app", Cloud.parse("https://yogurt-example.vercel.app/", false))
        assertEquals("https://yogurt-example.vercel.app", Cloud.parse("  HTTPS://Yogurt-Example.vercel.app/library?x=1 ", false))
        assertEquals("https://yogurt-example.vercel.app", Cloud.parse("yogurt-example.vercel.app", false))
        assertEquals("https://cloud.example:8443", Cloud.parse("https://cloud.example:8443", false))
        assertEquals("https://cloud.example", Cloud.parse("https://cloud.example:443", false))
    }

    @Test
    fun allows_http_only_to_the_mac_in_debug_builds() {
        assertNull(Cloud.parse("http://yogurt-example.vercel.app", true))
        assertNull(Cloud.parse("http://localhost:8787", false))
        assertNull(Cloud.parse("http://10.0.2.2:8787", false))
        assertEquals("http://localhost:8787", Cloud.parse("http://localhost:8787", true))
        assertEquals("http://10.0.2.2:8787", Cloud.parse("http://10.0.2.2:8787/", true))
    }

    @Test
    fun rejects_what_is_not_a_web_origin() {
        assertNull(Cloud.parse("", false))
        assertNull(Cloud.parse("javascript:alert(1)", true))
        assertNull(Cloud.parse("file:///sdcard/x.html", true))
        assertNull(Cloud.parse("https://user:pw@cloud.example", false))
        assertNull(Cloud.parse("https://", false))
    }

    @Test
    fun same_origin_compares_scheme_host_and_port() {
        val o = "https://yogurt-example.vercel.app"
        assertTrue(Cloud.sameOrigin("https://yogurt-example.vercel.app/meeting/1#x", o))
        assertTrue(Cloud.sameOrigin("https://YOGURT-example.vercel.app:443/", o))
        assertFalse(Cloud.sameOrigin("http://yogurt-example.vercel.app/", o))
        assertFalse(Cloud.sameOrigin("https://yogurt-example.vercel.app.evil.test/", o))
        assertFalse(Cloud.sameOrigin("https://evil.test/?https://yogurt-example.vercel.app", o))
        assertFalse(Cloud.sameOrigin("https://yogurt-example.vercel.app:8443/", o))
        assertFalse(Cloud.sameOrigin(null, o))
        assertFalse(Cloud.sameOrigin("about:blank", o))
    }
}
