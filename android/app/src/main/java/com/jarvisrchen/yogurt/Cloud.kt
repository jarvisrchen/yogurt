package com.jarvisrchen.yogurt

import java.net.URI

object Cloud {
    /**
     * The origin of a Cloud address the user typed, or null when the app should not open it.
     * Only https, plus http to the Mac (localhost through `adb reverse`, or 10.0.2.2 from the emulator) when [debug].
     */
    fun parse(input: String, debug: Boolean): String? {
        val text = input.trim().let { if ("://" in it) it else "https://$it" }
        val uri = runCatching { URI(text) }.getOrNull() ?: return null
        val host = uri.host?.lowercase() ?: return null
        val ok = when (uri.scheme?.lowercase()) {
            "https" -> true
            "http" -> debug && (host == "localhost" || host == "10.0.2.2")
            else -> false
        }
        return if (ok && uri.userInfo == null) originOf(text) else null
    }

    /** `scheme://host[:port]`, lowercased and without a default port, or null for a URL with no host. */
    fun originOf(url: String?): String? {
        val uri = runCatching { URI(url ?: return null) }.getOrNull() ?: return null
        val scheme = uri.scheme?.lowercase() ?: return null
        val host = uri.host?.lowercase() ?: return null
        val default = (scheme == "https" && uri.port == 443) || (scheme == "http" && uri.port == 80)
        return if (uri.port == -1 || default) "$scheme://$host" else "$scheme://$host:${uri.port}"
    }

    fun sameOrigin(url: String?, origin: String) = originOf(url) == origin
}
