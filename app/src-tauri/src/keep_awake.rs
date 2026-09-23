//! Keeps the screen on during long work on phones (downloads, replies). Desktop: no-op.
//!
//! Android: a tiny Kotlin plugin in the app project (KeepAwakePlugin.kt).
//! iOS: UIApplication.idleTimerDisabled, set on the main thread.

use tauri::{AppHandle, Runtime};

pub fn plugin<R: Runtime>() -> tauri::plugin::TauriPlugin<R> {
    tauri::plugin::Builder::<R>::new("keep-awake")
        .setup(|_app, _api| {
            #[cfg(target_os = "android")]
            {
                use tauri::Manager;
                let handle = _api.register_android_plugin("io.github.milandanilovic.opnlocal", "KeepAwakePlugin")?;
                _app.manage(AndroidKeepAwake(handle));
            }
            Ok(())
        })
        .build()
}

#[cfg(target_os = "android")]
struct AndroidKeepAwake<R: Runtime>(tauri::plugin::PluginHandle<R>);

/// Holds the screen on while alive. Several may overlap (a download while chatting); the screen
/// may sleep again once the last one is dropped.
pub struct Awake<R: Runtime>(AppHandle<R>);

static HOLDERS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

impl<R: Runtime> Awake<R> {
    pub fn new(app: &AppHandle<R>) -> Self {
        if HOLDERS.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0 {
            set(app, true);
        }
        Awake(app.clone())
    }
}

impl<R: Runtime> Drop for Awake<R> {
    fn drop(&mut self) {
        if HOLDERS.fetch_sub(1, std::sync::atomic::Ordering::SeqCst) == 1 {
            set(&self.0, false);
        }
    }
}

fn set<R: Runtime>(app: &AppHandle<R>, on: bool) {
    #[cfg(target_os = "android")]
    {
        use tauri::Manager;
        #[derive(serde::Serialize)]
        struct Args {
            on: bool,
        }
        if let Some(k) = app.try_state::<AndroidKeepAwake<R>>() {
            let _ = k.0.run_mobile_plugin::<()>("set", Args { on });
        }
    }
    #[cfg(target_os = "ios")]
    {
        let _ = app.run_on_main_thread(move || {
            use objc2::msg_send;
            use objc2::runtime::{AnyClass, AnyObject, Bool};
            let Some(cls) = AnyClass::get(c"UIApplication") else {
                return;
            };
            // SAFETY: UIKit calls on the main thread; sharedApplication always exists once running.
            unsafe {
                let shared: *mut AnyObject = msg_send![cls, sharedApplication];
                if !shared.is_null() {
                    let _: () = msg_send![shared, setIdleTimerDisabled: Bool::new(on)];
                }
            }
        });
    }
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    let _ = (app, on);
}
