use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;

use crate::config::{
    GOOGLE_OAUTH_AUTHORIZE_URL, GOOGLE_OAUTH_CLIENT_ID, GOOGLE_OAUTH_CLIENT_SECRET,
    GOOGLE_OAUTH_TOKEN_URL, GOOGLE_TASKS_SCOPE,
};
use crate::error::{JugglerError, Result};
use crate::time::{SharedClock, system_clock};
use chrono::Utc;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Method, Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use log::{error, info};
use oauth2::basic::BasicClient;
use oauth2::reqwest::async_http_client;
use oauth2::{
    AuthUrl, AuthorizationCode, ClientId, ClientSecret, CsrfToken, PkceCodeChallenge, RedirectUrl,
    RefreshToken, Scope, TokenResponse, TokenUrl,
};
use tokio::net::TcpListener;
use tokio::sync::{Mutex, oneshot};

// Type alias to simplify complex type
type OAuthSender = Arc<Mutex<Option<oneshot::Sender<std::result::Result<String, String>>>>>;

fn create_oauth_client(client_id: &str, token_url: &str) -> Result<BasicClient> {
    Ok(BasicClient::new(
        ClientId::new(client_id.to_string()),
        Some(ClientSecret::new(GOOGLE_OAUTH_CLIENT_SECRET.to_string())),
        AuthUrl::new(GOOGLE_OAUTH_AUTHORIZE_URL.to_string())
            .map_err(|e| JugglerError::oauth(format!("Invalid auth URL: {e}")))?,
        Some(
            TokenUrl::new(token_url.to_string())
                .map_err(|e| JugglerError::oauth(format!("Invalid token URL: {e}")))?,
        ),
    ))
}

// OAuth credentials (public desktop client) are embedded via constants in `config.rs`.
// For native/desktop apps, Google treats clients as public and permits embedding the
// client id and client secret with PKCE. See Google guidance:
// https://developers.google.com/identity/protocols/oauth2/native-app

#[derive(Debug, Clone)]
pub struct GoogleOAuthCredentials {
    pub client_id: String,
    pub refresh_token: String,
}

pub struct GoogleOAuthClient {
    credentials: GoogleOAuthCredentials,
    pub client: reqwest::Client,
    cached_access_token: Option<String>,
    token_expires_at: Option<chrono::DateTime<Utc>>,
    pub(crate) oauth_token_url: String,
    clock: SharedClock,
}

#[derive(Debug)]
pub struct OAuthResult {
    pub refresh_token: String,
}

#[derive(Debug)]
struct OAuthState {
    tx: OAuthSender,
}

pub async fn run_oauth_flow(client_id: String, port: u16) -> Result<OAuthResult> {
    info!("Starting OAuth flow for Google Tasks API...");
    info!("Client ID: {client_id}");

    // Start local HTTP server
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let listener = TcpListener::bind(addr).await?;
    let actual_port = listener.local_addr()?.port();

    info!("Started local server on port {actual_port}");

    let redirect_uri = format!("http://localhost:{actual_port}/callback");

    // Set up OAuth2 client using the oauth2 crate
    let oauth_client = create_oauth_client(GOOGLE_OAUTH_CLIENT_ID, GOOGLE_OAUTH_TOKEN_URL)?
        .set_redirect_uri(
            RedirectUrl::new(redirect_uri.clone())
                .map_err(|e| JugglerError::oauth(format!("Invalid redirect URI: {e}")))?,
        );

    // Generate PKCE challenge
    let (pkce_challenge, pkce_verifier) = PkceCodeChallenge::new_random_sha256();

    // Build authorization URL
    let (auth_url, _csrf_token) = oauth_client
        .authorize_url(CsrfToken::new_random)
        .add_scope(Scope::new(GOOGLE_TASKS_SCOPE.to_string()))
        .add_extra_param("access_type", "offline")
        .add_extra_param("prompt", "consent")
        .set_pkce_challenge(pkce_challenge)
        .url();

    // Open browser
    info!("Opening browser for authentication...");
    println!("\nOpening your browser to authenticate with Google Tasks.");
    println!("If your browser doesn't open automatically, please visit:");
    println!("{auth_url}\n");

    if let Err(e) = open_browser(auth_url.as_str()) {
        error!("Failed to open browser: {e}. Please manually visit the URL above.");
    }

    // Set up channel for receiving authorization code
    let (tx, rx) = oneshot::channel();

    let oauth_state = Arc::new(OAuthState {
        tx: Arc::new(Mutex::new(Some(tx))),
    });

    // Handle incoming connections
    tokio::spawn(async move {
        loop {
            match listener.accept().await {
                Ok((stream, _)) => {
                    let io = TokioIo::new(stream);
                    let oauth_state = Arc::clone(&oauth_state);

                    tokio::spawn(async move {
                        if let Err(err) = http1::Builder::new()
                            .serve_connection(
                                io,
                                service_fn(move |req| {
                                    let oauth_state = Arc::clone(&oauth_state);
                                    handle_request(req, oauth_state)
                                }),
                            )
                            .await
                        {
                            error!("Error serving connection: {err:?}");
                        }
                    });
                }
                Err(e) => {
                    error!("Failed to accept connection: {e}");
                    break;
                }
            }
        }
    });

    // Wait for authorization code
    let auth_code = match rx.await {
        Ok(Ok(code)) => code,
        Ok(Err(error)) => return Err(JugglerError::oauth(format!("OAuth error: {error}"))),
        Err(_) => return Err(JugglerError::oauth("Failed to receive authorization code")),
    };

    info!("Received authorization code, exchanging for tokens...");

    // Exchange authorization code for tokens using oauth2 crate
    let token_result = oauth_client
        .exchange_code(AuthorizationCode::new(auth_code))
        .set_pkce_verifier(pkce_verifier)
        .request_async(async_http_client)
        .await
        .map_err(|e| JugglerError::oauth(format!("Token exchange failed: {e}")))?;

    let refresh_token = token_result
        .refresh_token()
        .ok_or_else(|| {
            JugglerError::oauth(
                "No refresh token in response. This might happen if you've already granted permission. Try revoking access at https://myaccount.google.com/permissions and try again.",
            )
        })?
        .secret()
        .to_string();

    Ok(OAuthResult { refresh_token })
}

async fn handle_request(
    req: Request<hyper::body::Incoming>,
    oauth_state: Arc<OAuthState>,
) -> std::result::Result<Response<http_body_util::Full<hyper::body::Bytes>>, hyper::Error> {
    let response = match req.method() {
        &Method::GET => {
            let uri = req.uri();
            if uri.path() == "/callback" {
                handle_callback(uri.query(), oauth_state).await
            } else {
                Response::builder()
                    .status(StatusCode::NOT_FOUND)
                    .body(http_body_util::Full::new("Not Found".into()))
                    .expect("valid response")
            }
        }
        _ => Response::builder()
            .status(StatusCode::METHOD_NOT_ALLOWED)
            .body(http_body_util::Full::new("Method Not Allowed".into()))
            .expect("valid response"),
    };

    Ok(response)
}

async fn handle_callback(
    query: Option<&str>,
    oauth_state: Arc<OAuthState>,
) -> Response<http_body_util::Full<hyper::body::Bytes>> {
    let query = match query {
        Some(q) => q,
        None => {
            let mut tx_guard = oauth_state.tx.lock().await;
            if let Some(tx) = tx_guard.take() {
                let _ = tx.send(Err("No query parameters".to_string()));
            }
            return Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .body(http_body_util::Full::new("Missing query parameters".into()))
                .expect("valid response");
        }
    };

    let params: HashMap<String, String> = url::form_urlencoded::parse(query.as_bytes())
        .into_owned()
        .collect();

    if let Some(error) = params.get("error") {
        let default_error = "Unknown error".to_string();
        let error_description = params.get("error_description").unwrap_or(&default_error);
        let error_msg = format!("{error}: {error_description}");

        let mut tx_guard = oauth_state.tx.lock().await;
        if let Some(tx) = tx_guard.take() {
            let _ = tx.send(Err(error_msg));
        }

        return Response::builder()
            .status(StatusCode::BAD_REQUEST)
            .header("Content-Type", "text/html")
            .body(http_body_util::Full::new(
                format!(
                    "<html><body><h1>Authentication Failed</h1><p>Error: {error_description}</p></body></html>"
                )
                .into(),
            ))
            .expect("valid response");
    }

    if let Some(code) = params.get("code") {
        let mut tx_guard = oauth_state.tx.lock().await;
        if let Some(tx) = tx_guard.take() {
            let _ = tx.send(Ok(code.to_string()));
        }

        return Response::builder()
            .status(StatusCode::OK)
            .header("Content-Type", "text/html")
            .body(http_body_util::Full::new(
                r#"<html><body>
                    <h1>Authentication Successful!</h1>
                    <p>You have successfully authenticated with Google Tasks.</p>
                    <p>You can now close this window and return to your terminal.</p>
                    <script>window.close();</script>
                </body></html>"#
                    .into(),
            ))
            .expect("valid response");
    }

    let mut tx_guard = oauth_state.tx.lock().await;
    if let Some(tx) = tx_guard.take() {
        let _ = tx.send(Err("Missing authorization code".to_string()));
    }

    Response::builder()
        .status(StatusCode::BAD_REQUEST)
        .header("Content-Type", "text/html")
        .body(http_body_util::Full::new(
            "<html><body><h1>Authentication Failed</h1><p>Missing authorization code</p></body></html>"
                .into(),
        ))
        .expect("valid response")
}

fn open_browser(url: &str) -> Result<()> {
    open::that(url).map_err(|e| JugglerError::Other(format!("Failed to open browser: {e}")))?;
    Ok(())
}

impl GoogleOAuthClient {
    pub fn new(credentials: GoogleOAuthCredentials, client: reqwest::Client) -> Self {
        Self {
            credentials,
            client,
            cached_access_token: None,
            token_expires_at: None,
            oauth_token_url: GOOGLE_OAUTH_TOKEN_URL.to_string(),
            clock: system_clock(),
        }
    }

    #[cfg(test)]
    pub fn new_with_custom_oauth_url(
        credentials: GoogleOAuthCredentials,
        client: reqwest::Client,
        oauth_token_url: String,
        clock: SharedClock,
    ) -> Self {
        Self {
            credentials,
            client,
            cached_access_token: None,
            token_expires_at: None,
            oauth_token_url,
            clock,
        }
    }

    pub async fn get_access_token(&mut self) -> Result<String> {
        if let (Some(token), Some(expires_at)) = (&self.cached_access_token, &self.token_expires_at)
            && self.clock.now() < *expires_at - chrono::Duration::minutes(5)
        {
            return Ok(token.clone());
        }

        self.refresh_access_token().await
    }

    #[cfg(test)]
    pub fn cached_access_token(&self) -> &Option<String> {
        &self.cached_access_token
    }

    #[cfg(test)]
    pub fn token_expires_at(&self) -> &Option<chrono::DateTime<Utc>> {
        &self.token_expires_at
    }

    #[cfg(test)]
    pub fn credentials(&self) -> &GoogleOAuthCredentials {
        &self.credentials
    }

    #[cfg(test)]
    pub fn set_cached_token(&mut self, token: String, expires_at: chrono::DateTime<Utc>) {
        self.cached_access_token = Some(token);
        self.token_expires_at = Some(expires_at);
    }

    async fn refresh_access_token(&mut self) -> Result<String> {
        info!("Using embedded client_secret for token refresh (desktop/native client)");

        let oauth_client = create_oauth_client(&self.credentials.client_id, &self.oauth_token_url)?;

        let token_result = oauth_client
            .exchange_refresh_token(&RefreshToken::new(self.credentials.refresh_token.clone()))
            .request_async(async_http_client)
            .await
            .map_err(|e| JugglerError::oauth(format!("OAuth token refresh failed: {e}")))?;

        let access_token = token_result.access_token().secret().to_string();
        let expires_in = token_result
            .expires_in()
            .map(|d| d.as_secs())
            .unwrap_or(3600);

        self.cached_access_token = Some(access_token.clone());
        self.token_expires_at =
            Some(self.clock.now() + chrono::Duration::seconds(expires_in as i64));

        Ok(access_token)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::time::fixed_clock;
    use chrono::TimeZone;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    const TEST_CLIENT_ID: &str = "test-client-id";

    fn test_clock() -> SharedClock {
        fixed_clock(chrono::Utc.with_ymd_and_hms(2025, 1, 1, 0, 0, 0).unwrap())
    }

    #[tokio::test]
    async fn test_oauth_client_token_refresh() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "access_token": "new_access_token",
                "expires_in": 3600,
                "token_type": "Bearer"
            })))
            .mount(&mock_server)
            .await;

        let credentials = GoogleOAuthCredentials {
            client_id: TEST_CLIENT_ID.to_string(),
            refresh_token: "test_refresh_token".to_string(),
        };

        let oauth_token_url = format!("{}/token", mock_server.uri());
        let mut oauth_client = GoogleOAuthClient::new_with_custom_oauth_url(
            credentials,
            reqwest::Client::new(),
            oauth_token_url,
            test_clock(),
        );

        assert!(oauth_client.cached_access_token().is_none());
        assert!(oauth_client.token_expires_at().is_none());

        let token = oauth_client.get_access_token().await.unwrap();
        assert_eq!(token, "new_access_token");
        assert!(oauth_client.cached_access_token().is_some());
        assert!(oauth_client.token_expires_at().is_some());
    }

    #[tokio::test]
    async fn test_oauth_client_token_caching() {
        let credentials = GoogleOAuthCredentials {
            client_id: TEST_CLIENT_ID.to_string(),
            refresh_token: "test_refresh_token".to_string(),
        };

        let clock = test_clock();
        let mut oauth_client = GoogleOAuthClient::new_with_custom_oauth_url(
            credentials,
            reqwest::Client::new(),
            GOOGLE_OAUTH_TOKEN_URL.to_string(),
            clock.clone(),
        );

        oauth_client.set_cached_token(
            "cached_token".to_string(),
            clock.now() + chrono::Duration::hours(1),
        );

        let token = oauth_client.get_access_token().await.unwrap();
        assert_eq!(token, "cached_token");
    }

    #[tokio::test]
    async fn test_oauth_token_refresh_failure() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/token"))
            .respond_with(ResponseTemplate::new(400).set_body_json(serde_json::json!({
                "error": "invalid_grant",
                "error_description": "The provided authorization grant is invalid"
            })))
            .mount(&mock_server)
            .await;

        let credentials = GoogleOAuthCredentials {
            client_id: TEST_CLIENT_ID.to_string(),
            refresh_token: "invalid_refresh_token".to_string(),
        };

        let oauth_token_url = format!("{}/token", mock_server.uri());
        let mut oauth_client = GoogleOAuthClient::new_with_custom_oauth_url(
            credentials,
            reqwest::Client::new(),
            oauth_token_url,
            test_clock(),
        );

        let result = oauth_client.get_access_token().await;
        assert!(result.is_err());
        let error_msg = result.unwrap_err().to_string();
        assert!(error_msg.contains("OAuth token refresh failed"));
    }

    #[tokio::test]
    async fn test_oauth_credentials_structure() {
        let credentials = GoogleOAuthCredentials {
            client_id: TEST_CLIENT_ID.to_string(),
            refresh_token: "test_refresh_token".to_string(),
        };

        assert_eq!(credentials.client_id, TEST_CLIENT_ID);
        assert_eq!(credentials.refresh_token, "test_refresh_token");

        let cloned_credentials = credentials.clone();
        assert_eq!(cloned_credentials.client_id, credentials.client_id);
        assert_eq!(cloned_credentials.refresh_token, credentials.refresh_token);
    }

    #[tokio::test]
    async fn test_oauth_client_initialization() {
        let credentials = GoogleOAuthCredentials {
            client_id: TEST_CLIENT_ID.to_string(),
            refresh_token: "test_refresh_token".to_string(),
        };

        let oauth_client = GoogleOAuthClient::new(credentials.clone(), reqwest::Client::new());

        assert_eq!(oauth_client.credentials().client_id, credentials.client_id);
        assert_eq!(
            oauth_client.credentials().refresh_token,
            credentials.refresh_token
        );
        assert!(oauth_client.cached_access_token().is_none());
        assert!(oauth_client.token_expires_at().is_none());
        assert_eq!(oauth_client.oauth_token_url, GOOGLE_OAUTH_TOKEN_URL);
    }
}
