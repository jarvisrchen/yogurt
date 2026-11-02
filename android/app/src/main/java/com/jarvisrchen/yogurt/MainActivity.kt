package com.jarvisrchen.yogurt

import android.Manifest
import android.annotation.SuppressLint
import android.app.Activity
import android.content.Intent
import android.content.pm.PackageManager
import android.graphics.Bitmap
import android.os.Build
import android.os.Bundle
import android.util.Log
import android.view.KeyEvent
import android.view.View
import android.view.WindowInsets
import android.view.inputmethod.EditorInfo
import android.webkit.ConsoleMessage
import android.webkit.PermissionRequest
import android.webkit.WebChromeClient
import android.webkit.WebResourceError
import android.webkit.WebResourceRequest
import android.webkit.WebView
import android.webkit.WebViewClient
import android.widget.Button
import android.widget.EditText
import android.widget.TextView
import android.window.OnBackInvokedCallback
import android.window.OnBackInvokedDispatcher
import org.json.JSONObject
import kotlin.concurrent.thread

private const val CHANGE_CLOUD = "com.jarvisrchen.yogurt.CHANGE_CLOUD"

class MainActivity : Activity() {
    private val prefs by lazy { getSharedPreferences("yogurt", MODE_PRIVATE) }
    private var web: WebView? = null
    private var pendingStart: StartRequest? = null
    private var pendingWebMic: PermissionRequest? = null
    private var back: OnBackInvokedCallback? = null

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        Recording.init(this)
        route(intent)
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        route(intent)
    }

    private fun route(intent: Intent?) {
        DebugSeam.onLaunch(this, intent)
        val origin = prefs.getString("cloud", null)
        when {
            origin == null || intent?.action == CHANGE_CLOUD -> showSetup(origin, null)
            web == null -> showWeb(origin)
        }
    }

    private fun showSetup(prefill: String?, error: String?) {
        closeWeb()
        setContentView(R.layout.setup)
        fitInsets(findViewById(R.id.root))
        val address = findViewById<EditText>(R.id.address)
        val errorView = findViewById<TextView>(R.id.error)
        prefill?.let { address.setText(it) }
        fun show(message: String?) {
            errorView.text = message
            errorView.visibility = if (message == null) View.GONE else View.VISIBLE
        }
        show(error)
        val go = {
            val origin = Cloud.parse(address.text.toString(), BuildConfig.DEBUG)
            if (origin == null) {
                show(getString(R.string.bad_address))
            } else {
                prefs.edit().putString("cloud", origin).apply()
                showWeb(origin)
            }
        }
        findViewById<Button>(R.id.go).setOnClickListener { go() }
        address.setOnEditorActionListener { _, id, _ -> (id == EditorInfo.IME_ACTION_GO).also { if (it) go() } }
    }

    @SuppressLint("SetJavaScriptEnabled", "JavascriptInterface")
    private fun showWeb(origin: String) {
        closeWeb()
        setContentView(R.layout.main)
        fitInsets(findViewById(R.id.root))
        val view = findViewById<WebView>(R.id.web)
        val b = YogurtAndroid(this, origin)
        web = view
        WebView.setWebContentsDebuggingEnabled(BuildConfig.DEBUG)
        view.settings.javaScriptEnabled = true
        view.settings.domStorageEnabled = true
        view.addJavascriptInterface(b, "YogurtAndroid")
        view.webViewClient = object : WebViewClient() {
            override fun shouldOverrideUrlLoading(v: WebView, request: WebResourceRequest): Boolean {
                if (Cloud.sameOrigin(request.url.toString(), origin)) return false
                // Anything off the Cloud opens in the browser, so the bridge only ever faces the Cloud.
                runCatching { startActivity(Intent(Intent.ACTION_VIEW, request.url)) }
                return true
            }

            override fun onPageStarted(v: WebView, url: String, favicon: Bitmap?) {
                NativeEvents.detach()
                b.pageUrl = url
            }

            override fun onPageFinished(v: WebView, url: String) {
                b.pageUrl = url
                if (Cloud.sameOrigin(url, origin)) NativeEvents.attach(::dispatch)
            }

            override fun doUpdateVisitedHistory(v: WebView, url: String, isReload: Boolean) {
                b.pageUrl = url
                updateBack()
            }

            override fun onReceivedError(v: WebView, request: WebResourceRequest, error: WebResourceError) {
                // Posted, since this callback runs inside the WebView that showSetup destroys.
                if (request.isForMainFrame) v.post { showSetup(origin, getString(R.string.unreachable, origin)) }
            }
        }
        view.webChromeClient = object : WebChromeClient() {
            override fun onPermissionRequest(request: PermissionRequest) {
                val mic = PermissionRequest.RESOURCE_AUDIO_CAPTURE
                when {
                    !Cloud.sameOrigin(request.origin.toString(), origin) || mic !in request.resources -> request.deny()
                    hasMic() -> request.grant(arrayOf(mic))
                    else -> {
                        pendingWebMic = request
                        requestPermissions(arrayOf(Manifest.permission.RECORD_AUDIO), 1)
                    }
                }
            }

            override fun onConsoleMessage(m: ConsoleMessage): Boolean {
                if (BuildConfig.DEBUG) Log.d("YogurtWeb", "${m.message()} (${m.sourceId()}:${m.lineNumber()})")
                return BuildConfig.DEBUG
            }
        }
        // onPageStarted reaches the main thread after the page's first scripts may already call the bridge.
        b.pageUrl = origin
        view.loadUrl(origin)
        checkForUpdate()
    }

    private fun closeWeb() {
        NativeEvents.detach()
        web?.destroy()
        web = null
        updateBack()
    }

    /** Posts one event to the page as `yogurt-native`, on the main thread as WebView requires. */
    private fun dispatch(detail: String) {
        runOnUiThread {
            web?.evaluateJavascript("window.dispatchEvent(new CustomEvent(\"yogurt-native\",{detail:$detail}))", null)
        }
    }

    // Back walks the WebView's history first. With nothing left it falls through to the system, which leaves the app.
    private fun updateBack() {
        val canGoBack = web?.canGoBack() == true
        if (Build.VERSION.SDK_INT < 33) return
        if (canGoBack && back == null) {
            back = OnBackInvokedCallback { web?.goBack() }.also {
                onBackInvokedDispatcher.registerOnBackInvokedCallback(OnBackInvokedDispatcher.PRIORITY_DEFAULT, it)
            }
        } else if (!canGoBack) {
            back?.let { onBackInvokedDispatcher.unregisterOnBackInvokedCallback(it) }
            back = null
        }
    }

    // Before Android 13, back arrives as a key.
    override fun onKeyDown(keyCode: Int, event: KeyEvent): Boolean {
        val w = web
        if (keyCode == KeyEvent.KEYCODE_BACK && w != null && w.canGoBack()) {
            w.goBack()
            return true
        }
        return super.onKeyDown(keyCode, event)
    }

    fun hasMic() = checkSelfPermission(Manifest.permission.RECORD_AUDIO) == PackageManager.PERMISSION_GRANTED

    /** Asks for the mic, and for notifications so the Recording notification shows, then starts [r]. */
    fun askMic(r: StartRequest) = runOnUiThread {
        pendingStart = r
        val perms = mutableListOf(Manifest.permission.RECORD_AUDIO)
        if (Build.VERSION.SDK_INT >= 33) perms += Manifest.permission.POST_NOTIFICATIONS
        requestPermissions(perms.toTypedArray(), 0)
    }

    override fun onRequestPermissionsResult(requestCode: Int, permissions: Array<out String>, grantResults: IntArray) {
        if (requestCode == 1) {
            pendingWebMic?.let { if (hasMic()) it.grant(it.resources) else it.deny() }
            pendingWebMic = null
            return
        }
        val r = pendingStart ?: return
        pendingStart = null
        val error = if (hasMic()) Recording.start(r) else getString(R.string.mic_denied)
        if (error != null) {
            NativeEvents.post(JSONObject().put("type", "error").put("message", error))
            NativeEvents.post(JSONObject().put("type", "state").put("state", "stopped"))
        }
    }

    private fun checkForUpdate() {
        val banner = findViewById<View>(R.id.banner)
        val text = findViewById<TextView>(R.id.banner_text)
        val action = findViewById<TextView>(R.id.banner_action)
        val url = DebugSeam.releasesUrl(this) ?: RELEASES_URL
        thread {
            val u = Updates.check(url) ?: return@thread
            runOnUiThread {
                if (isDestroyed) return@runOnUiThread
                text.text = getString(R.string.update_ready, u.version)
                action.text = getString(R.string.update)
                banner.visibility = View.VISIBLE
                banner.setOnClickListener {
                    banner.isEnabled = false
                    action.text = getString(R.string.downloading)
                    thread {
                        val apk = runCatching { Updates.download(u, cacheDir) }
                        runOnUiThread {
                            banner.isEnabled = true
                            apk.onSuccess {
                                action.text = getString(R.string.update)
                                Updates.install(this, it)
                            }.onFailure {
                                text.text = getString(R.string.download_failed)
                                action.text = getString(R.string.retry)
                            }
                        }
                    }
                }
            }
        }
    }

    /** Pads [v] clear of the system bars and the keyboard, since the app draws edge to edge. */
    private fun fitInsets(v: View) {
        v.setOnApplyWindowInsetsListener { view, insets ->
            if (Build.VERSION.SDK_INT >= 30) {
                val bars = insets.getInsets(WindowInsets.Type.systemBars() or WindowInsets.Type.ime())
                view.setPadding(bars.left, bars.top, bars.right, bars.bottom)
            } else {
                @Suppress("DEPRECATION")
                view.setPadding(insets.systemWindowInsetLeft, insets.systemWindowInsetTop, insets.systemWindowInsetRight, insets.systemWindowInsetBottom)
            }
            insets
        }
        v.requestApplyInsets()
    }

    override fun onDestroy() {
        closeWeb()
        super.onDestroy()
    }
}
