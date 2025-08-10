use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use crate::config::{GOOGLE_OAUTH_AUTHORIZE_URL, GOOGLE_OAUTH_TOKEN_URL, GOOGLE_TASKS_SCOPE};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Method, Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use log::{error, info};
use rand::Rng;
use sha2::{Digest, Sha256};
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use url::Url;

// Type alias to simplify complex type
type OAuthSender = Arc<Mutex<Option<oneshot::Sender<Result<String, String>>>>>;

// Note: Users must provide their own OAuth credentials from Google Cloud Console
// This is required for security and compliance with Google's OAuth policies

#[derive(Debug)]
pub struct OAuthResult {
    pub refresh_token: String,
}

#[derive(Debug)]
struct OAuthState {
    #[allow(dead_code)]
    code_verifier: String,
    #[allow(dead_code)]
    client_id: String,
    tx: OAuthSender,
}

pub async fn run_oauth_flow(
    client_id: String,
    port: u16,
) -> Result<OAuthResult, Box<dyn std::error::Error>> {
    info!("Starting OAuth flow for Google Tasks API...");
    info!("Client ID: {client_id}");

    // Generate PKCE parameters
    let code_verifier = generate_code_verifier();
    let code_challenge = generate_code_challenge(&code_verifier);

    // Start local HTTP server
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let listener = TcpListener::bind(addr).await?;
    let actual_port = listener.local_addr()?.port();

    info!("Started local server on port {actual_port}");

    let redirect_uri = format!("http://localhost:{actual_port}/callback");

    // Build authorization URL
    let auth_url = build_auth_url(&client_id, &redirect_uri, &code_challenge);

    // Open browser
    info!("Opening browser for authentication...");
    println!("\nOpening your browser to authenticate with Google Tasks.");
    println!("If your browser doesn't open automatically, please visit:");
    println!("{auth_url}\n");

    if let Err(e) = open_browser(&auth_url) {
        error!("Failed to open browser: {e}. Please manually visit the URL above.");
    }

    // Set up channel for receiving authorization code
    let (tx, rx) = oneshot::channel();

    let oauth_state = Arc::new(OAuthState {
        code_verifier: code_verifier.clone(),
        client_id: client_id.clone(),
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
        Ok(Err(error)) => return Err(format!("OAuth error: {error}").into()),
        Err(_) => return Err("Failed to receive authorization code".into()),
    };

    info!("Received authorization code, exchanging for tokens...");

    // Exchange authorization code for tokens
    let refresh_token =
        exchange_code_for_tokens(&auth_code, &client_id, &redirect_uri, &code_verifier).await?;

    Ok(OAuthResult { refresh_token })
}

async fn handle_request(
    req: Request<hyper::body::Incoming>,
    oauth_state: Arc<OAuthState>,
) -> Result<Response<http_body_util::Full<hyper::body::Bytes>>, hyper::Error> {
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
            if let Ok(mut tx_guard) = oauth_state.tx.lock() {
                if let Some(tx) = tx_guard.take() {
                    let _ = tx.send(Err("No query parameters".to_string()));
                }
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

        if let Ok(mut tx_guard) = oauth_state.tx.lock() {
            if let Some(tx) = tx_guard.take() {
                let _ = tx.send(Err(error_msg));
            }
        }

        return Response::builder()
            .status(StatusCode::BAD_REQUEST)
            .header("Content-Type", "text/html")
            .body(http_body_util::Full::new(
                format!("<html><body><h1>Authentication Failed</h1><p>Error: {error_description}</p></body></html>")
                .into(),
            ))
            .unwrap();
    }

    if let Some(code) = params.get("code") {
        if let Ok(mut tx_guard) = oauth_state.tx.lock() {
            if let Some(tx) = tx_guard.take() {
                let _ = tx.send(Ok(code.clone()));
            }
        }

        Response::builder()
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
            .unwrap()
    } else {
        if let Ok(mut tx_guard) = oauth_state.tx.lock() {
            if let Some(tx) = tx_guard.take() {
                let _ = tx.send(Err("Missing authorization code".to_string()));
            }
        }

        Response::builder()
            .status(StatusCode::BAD_REQUEST)
            .header("Content-Type", "text/html")
            .body(http_body_util::Full::new(
                "<html><body><h1>Authentication Failed</h1><p>Missing authorization code</p></body></html>".into(),
            ))
            .unwrap()
    }
}

fn build_auth_url(client_id: &str, redirect_uri: &str, code_challenge: &str) -> String {
    let mut url = Url::parse(GOOGLE_OAUTH_AUTHORIZE_URL).unwrap();

    url.query_pairs_mut()
        .append_pair("client_id", client_id)
        .append_pair("redirect_uri", redirect_uri)
        .append_pair("response_type", "code")
        .append_pair("scope", GOOGLE_TASKS_SCOPE)
        .append_pair("access_type", "offline")
        .append_pair("prompt", "consent")
        .append_pair("code_challenge", code_challenge)
        .append_pair("code_challenge_method", "S256");

    url.to_string()
}

async fn exchange_code_for_tokens(
    auth_code: &str,
    client_id: &str,
    redirect_uri: &str,
    code_verifier: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    let client = reqwest::Client::new();

    let params = vec![
        ("client_id", client_id),
        ("code", auth_code),
        ("grant_type", "authorization_code"),
        ("redirect_uri", redirect_uri),
        ("code_verifier", code_verifier),
    ];

    // Debug log the parameters being sent (excluding sensitive data)
    info!("Token exchange parameters:");
    info!("  client_id: {client_id}");
    info!("  grant_type: authorization_code");
    info!("  redirect_uri: {redirect_uri}");
    info!("  code_verifier: [PRESENT - {} chars]", code_verifier.len());
    info!("  code: [PRESENT - {} chars]", auth_code.len());

    let response = client
        .post(GOOGLE_OAUTH_TOKEN_URL)
        .form(&params)
        .send()
        .await?;

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response.text().await.unwrap_or_default();
        // Provide actionable guidance for common misconfiguration
        let guidance = "Google returned client_secret is missing. This usually means the client id you used is a Web (confidential) client. Use a Desktop (Installed app) client id and PKCE instead. Re-run: `juggler login` (optionally with `--client-id <DESKTOP_CLIENT_ID>` or env `JUGGLER_CLIENT_ID`) to obtain a refresh token bound to the desktop client which does not require a client secret.";
        return Err(format!(
            "Token exchange failed ({}): {}\n{}",
            status, error_text, guidance
        )
        .into());
    }

    let token_response: serde_json::Value = response.json().await?;

    if let Some(refresh_token) = token_response.get("refresh_token").and_then(|v| v.as_str()) {
        Ok(refresh_token.to_string())
    } else {
        Err("No refresh token in response. This might happen if you've already granted permission. Try revoking access at https://myaccount.google.com/permissions and try again.".into())
    }
}

fn generate_code_verifier() -> String {
    let mut rng = rand::thread_rng();
    let bytes: Vec<u8> = (0..32).map(|_| rng.r#gen()).collect();
    URL_SAFE_NO_PAD.encode(&bytes)
}

fn generate_code_challenge(code_verifier: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(code_verifier.as_bytes());
    let digest = hasher.finalize();
    URL_SAFE_NO_PAD.encode(digest)
}

fn open_browser(url: &str) -> Result<(), Box<dyn std::error::Error>> {
    open::that(url)?;
    Ok(())
}
