package com.jarvisrchen.yogurt

import android.content.Context
import android.content.Intent

/** The release build's side of the debug test hooks: none. */
object DebugSeam {
    fun onLaunch(c: Context, intent: Intent?) = Unit

    fun pcmSource(c: Context): PcmSource? = null

    fun releasesUrl(c: Context): String? = null
}
