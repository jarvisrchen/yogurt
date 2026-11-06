package com.jarvisrchen.yogurt

import android.annotation.SuppressLint
import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.graphics.drawable.Icon
import android.os.Build
import okhttp3.OkHttpClient
import org.json.JSONObject
import java.io.File
import java.util.concurrent.TimeUnit
import kotlin.concurrent.thread

/** [model] names a [CATALOG] model to transcribe on the phone, or is null for Deepgram. */
class StartRequest(val meetingId: String, val startedAtMs: Long, val have: Int, val tokenUrl: String, val auth: Auth, val model: String? = null)

/**
 * The process's one recording, which outlives the activity. The service only keeps the process in the foreground while it runs.
 * It holds only the application context, which lives as long as the process anyway.
 */
@SuppressLint("StaticFieldLeak")
object Recording {
    private lateinit var app: Context
    lateinit var store: LineStore
        private set
    lateinit var models: Models
        private set
    // Pings find a socket whose network vanished without a close.
    private val http = OkHttpClient.Builder().pingInterval(15, TimeUnit.SECONDS).build()
    @Volatile private var current: Rec? = null

    @Synchronized
    fun init(context: Context) {
        if (::app.isInitialized) return
        app = context.applicationContext
        store = LineStore(File(app.filesDir, "recording"))
        models = Models(app)
    }

    /** Starts recording, or returns why it could not. Call while the app is on screen, as a microphone foreground service requires. */
    @Synchronized
    fun start(r: StartRequest): String? {
        current?.takeIf { !it.stopped }?.let {
            return if (store.meetingId == r.meetingId) null else "Already recording another meeting. Stop it first."
        }
        val spec = r.model?.let { name -> CATALOG.firstOrNull { it.name == name } ?: return "There is no model called $name." }
        if (spec != null && !models.downloaded(spec)) return "${spec.name} isn't on this phone. Download it in Transcription settings, or record this meeting with Cloud."
        val source = try {
            DebugSeam.pcmSource(app) ?: MicSource.open()
        } catch (e: Exception) {
            return e.message ?: "The microphone is not available."
        }
        store.begin(r.meetingId, r.startedAtMs, r.have)
        lateinit var rec: Rec
        val onEnd = {
            synchronized(this) { if (current === rec) app.stopService(Intent(app, RecorderService::class.java)) }
        }
        rec = if (spec != null) {
            LocalRecorder(store, r.meetingId, source, spec, models.dir(spec), r.startedAtMs, NativeEvents::post, onEnd)
        } else {
            Recorder(store, r.meetingId, source, http, r.tokenUrl, r.auth, r.startedAtMs, NativeEvents::post, onEnd)
        }
        try {
            app.startForegroundService(
                Intent(app, RecorderService::class.java).putExtra(EXTRA_STARTED, r.startedAtMs).putExtra(EXTRA_LOCAL, spec != null),
            )
        } catch (e: Exception) {
            source.close()
            return "Android did not let the recorder start: ${e.message}"
        }
        current = rec
        rec.start()
        return null
    }

    fun setAuth(a: Auth) {
        current?.renew(a)
    }

    /** The model a recording is transcribing with, until it has finished every line. Its files must stay, and a phone holds one model at a time. */
    fun localModel(): ModelSpec? = current.let { if (it is LocalRecorder && !it.stopped) it.spec else null }

    /** Ends with a stopped state event even when nothing records, since the page waits for one. */
    fun stop() {
        val r = current
        if (r == null || r.stopped) NativeEvents.post(JSONObject().put("type", "state").put("state", "stopped")) else r.stop()
    }

    fun status(): JSONObject {
        val r = current
        val recording = r != null && !r.stopped
        return JSONObject()
            .put("recording", recording)
            .put("meetingId", store.meetingId ?: JSONObject.NULL)
            .put("startedAtMs", if (store.meetingId != null) store.startedAtMs else JSONObject.NULL)
            .put("state", if (recording) r!!.state else "stopped")
            .put("lines", store.count())
    }
}

private const val EXTRA_STARTED = "startedAtMs"
private const val EXTRA_LOCAL = "local"
private const val ACTION_STOP = "com.jarvisrchen.yogurt.STOP"
private const val CHANNEL = "recording"

class RecorderService : Service() {
    override fun onBind(intent: Intent?) = null

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        if (intent?.action == ACTION_STOP) {
            // Stop waits for Deepgram's last lines, so it runs off the main thread. The recorder then stops this service.
            thread {
                Recording.stop()
                stopSelf()
            }
            return START_NOT_STICKY
        }
        val nm = getSystemService(NotificationManager::class.java)
        // Default importance, silenced, so the notification sits with alerting ones and on the lock screen, as a live recording should.
        nm.createNotificationChannel(
            NotificationChannel(CHANNEL, getString(R.string.recording), NotificationManager.IMPORTANCE_DEFAULT).apply { setSound(null, null) },
        )
        val open = PendingIntent.getActivity(
            this, 0, Intent(this, MainActivity::class.java), PendingIntent.FLAG_IMMUTABLE,
        )
        val stop = PendingIntent.getService(
            this, 1, Intent(this, RecorderService::class.java).setAction(ACTION_STOP), PendingIntent.FLAG_IMMUTABLE,
        )
        val n = Notification.Builder(this, CHANNEL)
            .setSmallIcon(R.drawable.ic_notification)
            .setContentTitle(getString(R.string.recording))
            .apply { if (intent?.getBooleanExtra(EXTRA_LOCAL, false) == true) setContentText(getString(R.string.transcribing_here)) }
            .setWhen(intent?.getLongExtra(EXTRA_STARTED, 0L)?.takeIf { it > 0 } ?: System.currentTimeMillis())
            .setShowWhen(true)
            .setUsesChronometer(true)
            .setOngoing(true)
            .setOnlyAlertOnce(true)
            .setVisibility(Notification.VISIBILITY_PUBLIC)
            .setCategory(Notification.CATEGORY_SERVICE)
            .setColor(getColor(R.color.blue))
            .setContentIntent(open)
            .addAction(Notification.Action.Builder(null as Icon?, getString(R.string.stop), stop).build())
            .apply { if (Build.VERSION.SDK_INT >= 31) setForegroundServiceBehavior(Notification.FOREGROUND_SERVICE_IMMEDIATE) }
            .build()
        if (Build.VERSION.SDK_INT >= 30) {
            startForeground(1, n, ServiceInfo.FOREGROUND_SERVICE_TYPE_MICROPHONE)
        } else {
            startForeground(1, n)
        }
        return START_NOT_STICKY
    }
}
