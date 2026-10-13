//! The Glass theme's window material. On macOS the window is transparent
//! (tauri.macos.conf.json) and this puts a native NSVisualEffectView behind the
//! webview, so whatever sits behind the app shows through, blurred, the way
//! Finder's sidebar or a HUD panel does. Elsewhere it does nothing.

use tauri::WebviewWindow;

use crate::error::Result;

/// Turns the native glass behind the webview on or off. Safe to repeat.
#[tauri::command]
pub fn set_window_glass(window: WebviewWindow, enabled: bool) -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        let target = window.clone();
        window
            .run_on_main_thread(move || {
                // apply_vibrancy stacks a new view on every call, so start clean.
                let _ = window_vibrancy::clear_vibrancy(&target);
                if enabled {
                    if let Err(error) = window_vibrancy::apply_vibrancy(
                        &target,
                        window_vibrancy::NSVisualEffectMaterial::HudWindow,
                        // Stay frosted while another app has focus; the panes
                        // still dim themselves when they lose focus.
                        Some(window_vibrancy::NSVisualEffectState::Active),
                        None,
                    ) {
                        eprintln!("augry: could not apply window glass: {error}");
                    }
                }
            })
            .map_err(|error| crate::AppError::Io {
                message: error.to_string(),
            })?;
    }
    #[cfg(not(target_os = "macos"))]
    let _ = (window, enabled);
    Ok(())
}
