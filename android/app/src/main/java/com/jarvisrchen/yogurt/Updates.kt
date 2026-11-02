package com.jarvisrchen.yogurt

import android.app.Activity
import android.content.Intent
import androidx.core.content.FileProvider
import okhttp3.OkHttpClient
import okhttp3.Request
import org.json.JSONObject
import java.io.File
import java.io.IOException

const val RELEASES_URL = "https://api.github.com/repos/jarvisrchen/yogurt/releases/latest"

class Update(val version: String, val url: String)

object Updates {
    private val ASSET = Regex("""^yogurt-android-(\d+(?:\.\d+)*)\.apk$""")
    private val http = OkHttpClient()

    /** Whether dotted version [a] is newer than [b]. A missing part counts as 0. */
    fun newer(a: String, b: String): Boolean {
        val x = a.split(".").map { it.toIntOrNull() ?: return false }
        val y = b.split(".").map { it.toIntOrNull() ?: return false }
        for (i in 0 until maxOf(x.size, y.size)) {
            val d = x.getOrElse(i) { 0 } - y.getOrElse(i) { 0 }
            if (d != 0) return d > 0
        }
        return false
    }

    /** The APK in a GitHub release, when it is newer than [current]. */
    fun pick(release: JSONObject, current: String): Update? {
        val assets = release.optJSONArray("assets") ?: return null
        for (i in 0 until assets.length()) {
            val a = assets.optJSONObject(i) ?: continue
            val version = ASSET.matchEntire(a.optString("name"))?.groupValues?.get(1) ?: continue
            val url = a.optString("browser_download_url")
            if (url.isNotEmpty() && newer(version, current)) return Update(version, url)
        }
        return null
    }

    /** Asks GitHub for the latest release. Blocking, and null on any failure: no update is the safe answer. */
    fun check(url: String): Update? = runCatching {
        val req = Request.Builder().url(url).header("Accept", "application/vnd.github+json").build()
        http.newCall(req).execute().use { r ->
            if (!r.isSuccessful) return null
            pick(JSONObject(r.body.string()), BuildConfig.VERSION_NAME)
        }
    }.getOrNull()

    /** Downloads the APK into the cache, which the FileProvider shares with the installer. Blocking. */
    fun download(u: Update, cacheDir: File): File {
        val dir = File(cacheDir, "updates").apply { deleteRecursively(); mkdirs() }
        val file = File(dir, "yogurt-android-${u.version}.apk")
        http.newCall(Request.Builder().url(u.url).build()).execute().use { r ->
            if (!r.isSuccessful) throw IOException("The download answered ${r.code}.")
            file.outputStream().use { r.body.byteStream().copyTo(it) }
        }
        return file
    }

    fun install(activity: Activity, apk: File) {
        val uri = FileProvider.getUriForFile(activity, "${activity.packageName}.files", apk)
        activity.startActivity(
            Intent(Intent.ACTION_VIEW)
                .setDataAndType(uri, "application/vnd.android.package-archive")
                .addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION),
        )
    }
}
