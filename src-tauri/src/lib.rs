//! Terminal Grid core. Module layout follows docs/technical-design.md section 2.

pub mod agent;
pub mod cloud;
pub mod error;
pub mod git;
mod glass;
#[cfg(target_os = "macos")]
mod menu;
pub mod notes;
pub mod pty;
pub mod state;
#[cfg(target_os = "macos")]
mod traffic_lights;

pub use error::AppError;

use std::time::Duration;

use tauri::{AppHandle, Manager, WindowEvent};

/// Background sweep so a branch changed from another window still shows up
/// (design section 7.4, trigger 4).
const GIT_SWEEP: Duration = Duration::from_secs(15);

/// Runs command work on a blocking thread.
///
/// Tauri executes a synchronous command on the main thread, so anything that
/// spawns a process or walks the filesystem has to be an async command that
/// hands the work off, or it stutters the window.
pub async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> T + Send + 'static,
) -> error::Result<T> {
    tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(|error| AppError::Io {
            message: error.to_string(),
        })
}

pub struct AppState {
    pub pty: pty::PtyManager,
    pub git: git::GitService,
    pub agent: agent::AgentWatcher,
    pub notes: notes::NotesService,
    pub state: state::StateService,
    pub cloud: cloud::CloudService,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default();
    // Windows and Linux keep a bare window; only macOS has an app menu bar.
    #[cfg(target_os = "macos")]
    let builder = builder.menu(menu::app_menu);
    // Windows and Linux start a second process for an augry:// link; it hands
    // the link over and quits. macOS delivers links to the running app.
    #[cfg(any(windows, target_os = "linux"))]
    let builder = builder.plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
        cloud::deliver_deep_links(app, argv);
    }));

    builder
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_deep_link::init())
        .setup(|app| {
            let handle = app.handle().clone();
            let config_dir = state::config_dir(&handle)?;
            let state_service = state::StateService::new(config_dir.clone());
            let config = state_service.config();

            // The integration scripts are refreshed from the bundle on every
            // launch so an app update also updates them (design section 4.5).
            if let Ok(resource_dir) = handle.path().resource_dir() {
                if let Err(error) = pty::shell::install_scripts(&resource_dir, &config_dir) {
                    eprintln!("terminal-grid: could not install shell integration: {error}");
                }
            }

            let notes_root = config
                .notes_root
                .clone()
                .unwrap_or_else(notes::NotesService::default_root);

            let pty = pty::PtyManager::new(config_dir);
            agent::task::clear_task_dir(&pty.tasks_dir());

            let app_state = AppState {
                pty,
                git: git::GitService::new(),
                agent: agent::AgentWatcher::new(),
                notes: notes::NotesService::new(notes_root),
                state: state_service,
                cloud: cloud::CloudService::new(),
            };

            // A missing or unwritable notes folder must not stop the app.
            if let Err(error) = app_state.notes.set_root(app_state.notes.root()) {
                eprintln!("terminal-grid: notes folder unavailable: {error}");
            }

            app.manage(app_state);

            let state = handle.state::<AppState>();
            state.agent.start(handle.clone());
            if let Err(error) = state.notes.watch(handle.clone()) {
                eprintln!("terminal-grid: notes watcher unavailable: {error}");
            }
            start_git_sweep(handle.clone());
            watch_deep_links(&handle);

            #[cfg(target_os = "macos")]
            if let Some(window) = app.get_webview_window("main") {
                traffic_lights::watch(&window.as_ref().window());
            }

            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::Destroyed = event {
                if let Some(state) = window.app_handle().try_state::<AppState>() {
                    state.agent.stop();
                    state.pty.close_all();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            // pty
            pty::create_terminal,
            pty::write_terminal,
            pty::write_terminal_bytes,
            pty::resize_terminal,
            pty::close_terminal,
            pty::restart_terminal,
            pty::list_terminals,
            pty::set_terminal_cwd,
            pty::report_cwd,
            pty::set_focused_terminal,
            // agents
            agent::installed::installed_agents,
            // git
            git::get_git_info,
            git::refresh_git_info,
            git::worktree::worktree_create,
            git::worktree::worktree_remove,
            git::review::review_status,
            git::review::review_files,
            git::review::review_file,
            git::review::review_stage,
            git::review::review_unstage,
            git::review::review_write_file,
            // notes
            notes::notes_get_root,
            notes::notes_set_root,
            notes::notes_tree,
            notes::notes_read,
            notes::notes_write,
            notes::notes_create,
            notes::notes_rename,
            notes::notes_delete,
            notes::notes_reveal,
            // config and session
            state::config_get,
            state::config_set,
            state::session_get,
            state::session_set,
            // window
            glass::set_window_glass,
            // cloud
            cloud::cloud_status,
            cloud::cloud_request,
            cloud::cloud_device_start,
            cloud::cloud_device_poll,
            cloud::cloud_sign_out,
            cloud::cloud_open_url,
            cloud::cloud_take_deep_links,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Terminal Grid");
}

/// augry://join/<code> links: the one the app was started with, and any that
/// arrive while it runs.
fn watch_deep_links(app: &AppHandle) {
    use tauri_plugin_deep_link::DeepLinkExt;

    // Dev builds and Linux aren't registered by an installer.
    #[cfg(any(windows, target_os = "linux"))]
    if let Err(error) = app.deep_link().register_all() {
        eprintln!("terminal-grid: could not register augry:// links: {error}");
    }
    if let Ok(Some(urls)) = app.deep_link().get_current() {
        cloud::deliver_deep_links(app, urls.iter().map(|u| u.to_string()).collect());
    }
    let handle = app.clone();
    app.deep_link().on_open_url(move |event| {
        cloud::deliver_deep_links(&handle, event.urls().iter().map(|u| u.to_string()).collect());
    });
}

/// Re-reads git for every open session on a slow timer. The focused session is
/// refreshed first because that is the one the user is looking at.
fn start_git_sweep(app: AppHandle) {
    std::thread::spawn(move || loop {
        std::thread::sleep(GIT_SWEEP);

        let Some(state) = app.try_state::<AppState>() else {
            return;
        };
        let focused = state.pty.focused();
        let mut ids = state.pty.ids();
        if let Some(focused) = focused {
            ids.sort_by_key(|id| *id != focused);
        }

        for id in ids {
            let Some(cwd) = state.pty.cwd_of(&id) else {
                continue;
            };
            let open_dirs = state.pty.open_dirs();
            let info = state.git.info(&cwd, GIT_SWEEP, &open_dirs);
            git::emit_git(&app, id, info);
        }
    });
}
