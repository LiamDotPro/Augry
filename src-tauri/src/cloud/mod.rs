//! Augry cloud: accounts, teams, team processes and notes sync.
//!
//! The HTTP client and the session token live here so the renderer never
//! holds the token. The token is kept in the OS keychain (never in config or
//! session files); the renderer asks for API calls by path and gets back the
//! status and JSON body. The API is LiamDotPro/Terminal-Grid-Cloud.

use std::sync::Mutex;
use std::time::Duration;

use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_opener::OpenerExt;

use crate::error::{AppError, Result};
use crate::AppState;

/// Where the API runs. `AUGRY_CLOUD_URL` overrides it at run time or at build
/// time, for a local server (http://localhost:8080).
const DEFAULT_BASE_URL: &str = "https://augry-api.fly.dev";
const KEYCHAIN_SERVICE: &str = "app.augry.cloud";
const KEYCHAIN_USER: &str = "session";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(20);
/// Only these hosts may be opened from the sign-in screens.
const OPENABLE: [&str; 2] = ["https://github.com/", "https://augry.app/"];

pub const DEEP_LINK_EVENT: &str = "cloud://deep-link";

pub struct CloudService {
    base: String,
    http: reqwest::Client,
    /// The keychain is read once; None until then.
    token: Mutex<Option<Option<String>>>,
    /// augry:// links that arrived before the renderer was listening.
    pending_links: Mutex<Vec<String>>,
}

impl CloudService {
    pub fn new() -> Self {
        let base = std::env::var("AUGRY_CLOUD_URL")
            .ok()
            .or_else(|| option_env!("AUGRY_CLOUD_URL").map(str::to_string))
            .filter(|url| !url.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_BASE_URL.to_string());
        let http = reqwest::Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .user_agent(concat!("Augry/", env!("CARGO_PKG_VERSION")))
            .build()
            .unwrap_or_default();
        Self {
            base: base.trim_end_matches('/').to_string(),
            http,
            token: Mutex::new(None),
            pending_links: Mutex::new(Vec::new()),
        }
    }

    fn token(&self) -> Option<String> {
        let mut cached = self.token.lock().unwrap();
        if cached.is_none() {
            *cached = Some(keychain_get());
        }
        cached.clone().flatten()
    }

    fn set_token(&self, token: Option<String>) -> Result<()> {
        match &token {
            Some(value) => keychain_set(value)?,
            None => keychain_delete(),
        }
        *self.token.lock().unwrap() = Some(token);
        Ok(())
    }

    async fn send(&self, method: &str, path: &str, body: Option<Value>, auth: bool) -> Result<CloudResponse> {
        let method = reqwest::Method::from_bytes(method.to_ascii_uppercase().as_bytes()).map_err(|_| {
            AppError::InvalidName {
                message: format!("unknown method {method}"),
            }
        })?;
        let mut request = self.http.request(method, format!("{}{}", self.base, path));
        let token = if auth { self.token() } else { None };
        if let Some(token) = &token {
            request = request.bearer_auth(token);
        }
        if let Some(body) = body {
            request = request.json(&body);
        }
        let response = request.send().await.map_err(offline)?;
        let status = response.status().as_u16();
        let text = response.text().await.map_err(offline)?;
        let body = if text.trim().is_empty() {
            Value::Null
        } else {
            serde_json::from_str(&text).unwrap_or(Value::String(text))
        };
        // The session was revoked or ran out: forget it, everything local stays.
        if status == 401 && token.is_some() {
            let _ = self.set_token(None);
        }
        Ok(CloudResponse { status, body })
    }
}

impl Default for CloudService {
    fn default() -> Self {
        Self::new()
    }
}

fn offline(error: reqwest::Error) -> AppError {
    AppError::Offline {
        message: error.to_string(),
    }
}

fn keychain_entry() -> Result<keyring::Entry> {
    keyring::Entry::new(KEYCHAIN_SERVICE, KEYCHAIN_USER).map_err(keychain_error)
}

fn keychain_error(error: keyring::Error) -> AppError {
    AppError::Io {
        message: format!("keychain: {error}"),
    }
}

fn keychain_get() -> Option<String> {
    keychain_entry().ok()?.get_password().ok().filter(|t| !t.is_empty())
}

fn keychain_set(token: &str) -> Result<()> {
    keychain_entry()?.set_password(token).map_err(keychain_error)
}

fn keychain_delete() {
    if let Ok(entry) = keychain_entry() {
        let _ = entry.delete_credential();
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct CloudResponse {
    pub status: u16,
    pub body: Value,
}

/// A path on the API, never a full URL: the renderer can't send the token
/// anywhere else.
fn check_path(path: &str) -> Result<()> {
    let ok = path.starts_with("/v1/")
        && !path.contains("://")
        && !path.contains("..")
        && !path.contains('\\')
        && !path.starts_with("/v1/auth/");
    if ok {
        Ok(())
    } else {
        Err(AppError::InvalidName {
            message: format!("not an API path: {path}"),
        })
    }
}

fn cloud(app: &AppHandle) -> &CloudService {
    &app.state::<AppState>().inner().cloud
}

/// The computer's name, shown in the signed-in devices list.
fn device_name() -> String {
    let name = sysinfo::System::host_name().unwrap_or_default();
    let name = name.trim().trim_end_matches(".local").trim();
    let name: String = name.chars().take(64).collect();
    if name.is_empty() {
        "This computer".to_string()
    } else {
        name
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudStatus {
    pub signed_in: bool,
    pub base_url: String,
    pub device_name: String,
}

#[tauri::command]
pub async fn cloud_status(app: AppHandle) -> Result<CloudStatus> {
    let handle = app.clone();
    let signed_in = crate::blocking(move || cloud(&handle).token().is_some()).await?;
    let service = cloud(&app);
    Ok(CloudStatus {
        signed_in,
        base_url: service.base.clone(),
        device_name: device_name(),
    })
}

/// Any API call but sign in: `path` is like `/v1/teams`. Network errors are
/// `Offline`; HTTP errors come back as a status for the caller to word.
#[tauri::command]
pub async fn cloud_request(
    state: State<'_, AppState>,
    method: String,
    path: String,
    body: Option<Value>,
) -> Result<CloudResponse> {
    check_path(&path)?;
    state.cloud.send(&method, &path, body, true).await
}

/// Starts GitHub's device flow and opens the code page in the browser.
/// Returns the API's DeviceStart: userCode, verificationUri, handle, interval, expiresIn.
#[tauri::command]
pub async fn cloud_device_start(app: AppHandle) -> Result<CloudResponse> {
    let body = serde_json::json!({ "deviceName": device_name() });
    let response = cloud(&app)
        .send("POST", "/v1/auth/github/device", Some(body), false)
        .await?;
    if response.status == 200 {
        if let Some(uri) = response.body.get("verificationUri").and_then(Value::as_str) {
            let _ = open_allowed(&app, uri);
        }
    }
    Ok(response)
}

/// One poll of the device flow. On 200 the token goes to the keychain and the
/// body comes back without it.
#[tauri::command]
pub async fn cloud_device_poll(app: AppHandle, handle: String) -> Result<CloudResponse> {
    let service = cloud(&app);
    let mut response = service
        .send("POST", "/v1/auth/github/token", Some(serde_json::json!({ "handle": handle })), false)
        .await?;
    if response.status == 200 {
        let token = response
            .body
            .get("sessionToken")
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| AppError::Io {
                message: "sign in answered without a session".to_string(),
            })?;
        let app2 = app.clone();
        crate::blocking(move || cloud(&app2).set_token(Some(token))).await??;
        if let Some(body) = response.body.as_object_mut() {
            body.remove("sessionToken");
        }
    }
    Ok(response)
}

/// Signs this device out: revokes the session (best effort, it may be
/// offline) and forgets the token. Nothing stored locally is touched.
#[tauri::command]
pub async fn cloud_sign_out(app: AppHandle) -> Result<()> {
    let service = cloud(&app);
    if service.token().is_some() {
        let _ = service.send("POST", "/v1/auth/logout", None, true).await;
    }
    let app2 = app.clone();
    crate::blocking(move || cloud(&app2).set_token(None)).await?
}

/// Opens a GitHub or augry.app page in the browser.
#[tauri::command]
pub fn cloud_open_url(app: AppHandle, url: String) -> Result<()> {
    open_allowed(&app, &url)
}

fn open_allowed(app: &AppHandle, url: &str) -> Result<()> {
    if !OPENABLE.iter().any(|prefix| url.starts_with(prefix)) {
        return Err(AppError::InvalidName {
            message: format!("can't open {url}"),
        });
    }
    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|error| AppError::Io {
            message: error.to_string(),
        })
}

/// augry:// links that arrived before the renderer listened (a cold start
/// from a link). Taking them empties the list.
#[tauri::command]
pub fn cloud_take_deep_links(state: State<'_, AppState>) -> Vec<String> {
    std::mem::take(&mut *state.cloud.pending_links.lock().unwrap())
}

/// Queues an augry:// link and tells the renderer.
pub fn deliver_deep_links(app: &AppHandle, urls: Vec<String>) {
    let urls: Vec<String> = urls.into_iter().filter(|u| u.starts_with("augry://")).collect();
    if urls.is_empty() {
        return;
    }
    if let Some(state) = app.try_state::<AppState>() {
        state.cloud.pending_links.lock().unwrap().extend(urls.iter().cloned());
    }
    let _ = app.emit(DEEP_LINK_EVENT, urls);
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

#[cfg(test)]
mod tests {
    use super::check_path;

    #[test]
    fn only_api_paths_pass() {
        assert!(check_path("/v1/teams").is_ok());
        assert!(check_path("/v1/teams/abc/processes?since=3").is_ok());
        assert!(check_path("https://evil.example/v1/teams").is_err());
        assert!(check_path("/v1/../admin").is_err());
        assert!(check_path("//evil.example/v1/").is_err());
        assert!(check_path("/v1/auth/github/token").is_err());
        assert!(check_path("v1/teams").is_err());
    }
}
