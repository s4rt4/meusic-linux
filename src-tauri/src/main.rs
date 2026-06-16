// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // On Linux, webkit2gtk's DMABUF renderer crashes/freezes the webview on many
    // GPU + compositor combinations ("WebKitGTK quit unexpectedly"). Disabling it
    // forces a stable rendering path. Set before the webview inits, and only as a
    // process-local env var — it affects this app only, never other apps. We don't
    // override a value the user set deliberately.
    #[cfg(target_os = "linux")]
    {
        if std::env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_none() {
            std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
        }
    }

    meusic_lib::run()
}
