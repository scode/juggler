use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;

use crate::config::{
    GOOGLE_OAUTH_AUTHORIZE_URL, GOOGLE_OAUTH_CLIENT_ID, GOOGLE_OAUTH_CLIENT_SECRET,
    GOOGLE_OAUTH_TOKEN_URL, GOOGLE_TASKS_SCOPE,
};
use crate::error::{JugglerError, Result};
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Method, Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use log::{error, info};
use oauth2::basic::BasicClient;
use oauth2::reqwest::async_http_client;
use oauth2::{
    AuthUrl, AuthorizationCode, ClientId, ClientSecret, CsrfToken, PkceCodeChallenge, RedirectUrl,
    Scope, TokenResponse, TokenUrl,
};
use tokio::net::TcpListener;
use tokio::sync::{Mutex, oneshot};

// Type alias to simplify complex type
type OAuthSender = Arc<Mutex<Option<oneshot::Sender<std::result::Result<String, String>>>>>;

// OAuth credentials (public desktop client) are embedded via constants in `config.rs`.
// For native/desktop apps, Google treats clients as public and permits embedding the
// client id and client secret with PKCE. See Google guidance:
// https://developers.google.com/identity/protocols/oauth2/native-app

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
    let oauth_client = BasicClient::new(
        ClientId::new(GOOGLE_OAUTH_CLIENT_ID.to_string()),
        Some(ClientSecret::new(GOOGLE_OAUTH_CLIENT_SECRET.to_string())),
        AuthUrl::new(GOOGLE_OAUTH_AUTHORIZE_URL.to_string())
            .map_err(|e| JugglerError::oauth(format!("Invalid auth URL: {e}")))?,
        Some(
            TokenUrl::new(GOOGLE_OAUTH_TOKEN_URL.to_string())
                .map_err(|e| JugglerError::oauth(format!("Invalid token URL: {e}")))?,
        ),
    )
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
                    .unwrap()
            }
        }
        _ => Response::builder()
            .status(StatusCode::METHOD_NOT_ALLOWED)
            .body(http_body_util::Full::new("Method Not Allowed".into()))
            .unwrap(),
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
                .unwrap();
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
            .unwrap();
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
            .unwrap();
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
        .unwrap()
}

fn open_browser(url: &str) -> Result<()> {
    open::that(url).map_err(|e| JugglerError::Other(format!("Failed to open browser: {e}")))?;
    Ok(())
}
