//! Client for the Augry cloud API. The client and its types are generated from
//! libs/api-contract/openapi.yaml into generated.rs (`npm run api:generate`).
//! This file only adds what the generator can't: building a client with a
//! session token, and decoding the few responses it leaves raw.

#[allow(clippy::all, dead_code, unused_imports)]
mod generated;

pub use generated::*;

use std::time::Duration;

use futures_util::TryStreamExt;
use serde::de::DeserializeOwned;

/// The production API. AUGRY_API_URL overrides it, for example
/// http://localhost:8080 for the local stack. Fly's own hostname until the
/// api.augry.app certificate is verified.
pub const DEFAULT_BASE_URL: &str = "https://augry-api.fly.dev";

pub fn base_url() -> String {
    std::env::var("AUGRY_API_URL").unwrap_or_else(|_| DEFAULT_BASE_URL.to_string())
}

/// A client for `base_url`. With a session token every request is signed in.
pub fn client(base_url: &str, session_token: Option<&str>) -> Client {
    let mut headers = reqwest::header::HeaderMap::new();
    if let Some(token) = session_token {
        let mut value = reqwest::header::HeaderValue::from_str(&format!("Bearer {token}"))
            .expect("session token is a valid header value");
        value.set_sensitive(true);
        headers.insert(reqwest::header::AUTHORIZATION, value);
    }
    let http = reqwest::Client::builder()
        .user_agent(concat!("terminal-grid/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(30))
        .default_headers(headers)
        .build()
        .expect("reqwest client");
    Client::new_with_client(base_url, http)
}

/// Reads a raw response body as JSON. The generator leaves a response raw when
/// one operation answers with several body types; decode it by status.
pub async fn read_json<T: DeserializeOwned>(body: ByteStream) -> Result<T, String> {
    let bytes = body
        .into_inner()
        .try_fold(Vec::new(), |mut all, chunk| async move {
            all.extend_from_slice(&chunk);
            Ok(all)
        })
        .await
        .map_err(|e| e.to_string())?;
    serde_json::from_slice(&bytes).map_err(|e| e.to_string())
}

/// The answer of postAuthGithubToken, which the generator leaves raw.
#[derive(Debug)]
pub enum GithubToken {
    /// 200: signed in. The session token is shown once.
    SignedIn(types::SessionCreated),
    /// 202: not approved yet. Wait `interval` seconds and ask again.
    Pending(types::DevicePending),
}

/// Asks once whether the GitHub device sign in for `handle` is approved.
pub async fn poll_github_token(client: &Client, handle: &str) -> Result<GithubToken, String> {
    let body = types::DeviceTokenRequest {
        handle: handle.to_string(),
    };
    let response = client
        .post_auth_github_token(&body)
        .await
        .map_err(|e| e.to_string())?;
    let status = response.status();
    let raw = response.into_inner();
    match status.as_u16() {
        200 => Ok(GithubToken::SignedIn(read_json(raw).await?)),
        202 => Ok(GithubToken::Pending(read_json(raw).await?)),
        other => Err(format!("unexpected status {other}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Talks to a running API, so it only runs when AUGRY_API_URL is set:
    //   AUGRY_API_URL=http://localhost:8080 cargo test cloud
    fn local() -> Option<Client> {
        std::env::var("AUGRY_API_URL")
            .ok()
            .map(|url| client(&url, None))
    }

    #[tokio::test]
    async fn cloud_api_without_signing_in() {
        let Some(api) = local() else {
            eprintln!("skipped: AUGRY_API_URL is not set");
            return;
        };

        assert_eq!(api.get_healthz().await.unwrap().into_inner().status, "ok");
        assert_eq!(api.get_readyz().await.unwrap().into_inner().status, "ok");

        let features = api.get_features().await.unwrap().into_inner();
        assert_eq!(features.api_version, 1);
        assert!(features.features.contains(&"templates".to_string()));

        let templates = api.list_templates().await.unwrap().into_inner().templates;
        let names: Vec<_> = templates
            .iter()
            .map(|t| format!("{} v{}", t.slug, t.version))
            .collect();
        eprintln!("templates: {}", names.join(", "));

        match api.get_me().await {
            Err(Error::ErrorResponse(err)) => {
                assert_eq!(err.status().as_u16(), 401);
                assert_eq!(err.into_inner().error.code, "unauthorized");
            }
            other => panic!("expected 401, got {other:?}"),
        }
    }
}
