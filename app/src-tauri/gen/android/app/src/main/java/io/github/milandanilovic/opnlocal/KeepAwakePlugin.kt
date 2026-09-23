package io.github.milandanilovic.opnlocal

import android.app.Activity
import android.view.WindowManager
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.Plugin

@InvokeArg
class KeepAwakeArgs {
  var on: Boolean = false
}

/** Keeps the screen on while a model downloads or replies, so the phone doesn't sleep mid-way. */
@TauriPlugin
class KeepAwakePlugin(private val activity: Activity) : Plugin(activity) {
  @Command
  fun set(invoke: Invoke) {
    val args = invoke.parseArgs(KeepAwakeArgs::class.java)
    activity.runOnUiThread {
      if (args.on) {
        activity.window.addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
      } else {
        activity.window.clearFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
      }
    }
    invoke.resolve()
  }
}
