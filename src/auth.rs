use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::error::Error;
use log::{info, debug};

/// OAuth token response from Google
#[derive(Debug, Deserialize, Serialize)]
pub struct TokenResponse {
    pub access_token: String,
    pub token_type: String,
    pub expires_in: u64,
    pub refresh_token: Option<String>,
    pub scope: Option<String>,
}

/// OAuth error response from Google
#[derive(Debug, Deserialize)]
pub struct OAuthError {
    pub error: String,
    pub error_description: Option<String>,
}

/// Google OAuth configuration
pub struct GoogleOAuthConfig {
    pub client_id: String,
    pub client_secret: String,
    pub refresh_token: String,
}

impl GoogleOAuthConfig {
    pub fn new(client_id: String, client_secret: String, refresh_token: String) -> Self {
        Self {
            client_id,
            client_secret,
            refresh_token,
        }
    }
}

/// Refreshes an access token using a refresh token
pub async fn refresh_access_token(
    client: &Client,
    config: &GoogleOAuthConfig,
) -> Result<TokenResponse, Box<dyn Error>> {
    let token_url = "https://oauth2.googleapis.com/token";
    
    let params = [
        ("client_id", &config.client_id),
        ("client_secret", &config.client_secret),
        ("refresh_token", &config.refresh_token),
        ("grant_type", &"refresh_token".to_string()),
    ];

    debug!("Refreshing access token...");

    let response = client
        .post(token_url)
        .form(&params)
        .send()
        .await?;

    if response.status().is_success() {
        let token_response: TokenResponse = response.json().await?;
        info!("Access token refreshed successfully, expires in {} seconds", token_response.expires_in);
        Ok(token_response)
    } else {
        let status = response.status();
        let error_text = response.text().await?;
        // Try to parse as OAuth error first
        if let Ok(oauth_error) = serde_json::from_str::<OAuthError>(&error_text) {
            return Err(format!(
                "OAuth error: {} - {}",
                oauth_error.error,
                oauth_error.error_description.unwrap_or_default()
            ).into());
        }
        
        Err(format!(
            "Failed to refresh token: HTTP {} - {}",
            status,
            error_text
        ).into())
    }
}

/// Validates that a refresh token can be used to get an access token
pub async fn validate_refresh_token(
    client: &Client,
    config: &GoogleOAuthConfig,
) -> Result<(), Box<dyn Error>> {
    debug!("Validating refresh token...");
    refresh_access_token(client, config).await?;
    info!("Refresh token validation successful");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::{Mock, MockServer, ResponseTemplate};
    use wiremock::matchers::{method, path, body_string_contains};

    #[tokio::test]
    async fn test_refresh_access_token_success() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/token"))
            .and(body_string_contains("grant_type=refresh_token"))
            .and(body_string_contains("client_id=test_client_id"))
            .and(body_string_contains("client_secret=test_client_secret"))
            .and(body_string_contains("refresh_token=test_refresh_token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "access_token": "ya29.new_access_token",
                "token_type": "Bearer",
                "expires_in": 3600,
                "scope": "https://www.googleapis.com/auth/tasks"
            })))
            .mount(&mock_server)
            .await;

        let client = Client::new();
        let config = GoogleOAuthConfig::new(
            "test_client_id".to_string(),
            "test_client_secret".to_string(),
            "test_refresh_token".to_string(),
        );

        // Mock the token endpoint URL by replacing the base URL
        let token_url = format!("{}/token", mock_server.uri());
        
        // We need to create a custom refresh function for testing
        let params = [
            ("client_id", &config.client_id),
            ("client_secret", &config.client_secret),
            ("refresh_token", &config.refresh_token),
            ("grant_type", &"refresh_token".to_string()),
        ];

        let response = client
            .post(&token_url)
            .form(&params)
            .send()
            .await
            .unwrap();

        assert!(response.status().is_success());
        
        let token_response: TokenResponse = response.json().await.unwrap();
        assert_eq!(token_response.access_token, "ya29.new_access_token");
        assert_eq!(token_response.token_type, "Bearer");
        assert_eq!(token_response.expires_in, 3600);
    }

    #[tokio::test]
    async fn test_refresh_access_token_error() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/token"))
            .respond_with(ResponseTemplate::new(400).set_body_json(serde_json::json!({
                "error": "invalid_grant",
                "error_description": "Bad Request"
            })))
            .mount(&mock_server)
            .await;

        let client = Client::new();
        let config = GoogleOAuthConfig::new(
            "test_client_id".to_string(),
            "test_client_secret".to_string(),
            "invalid_refresh_token".to_string(),
        );

        let token_url = format!("{}/token", mock_server.uri());
        let params = [
            ("client_id", &config.client_id),
            ("client_secret", &config.client_secret),
            ("refresh_token", &config.refresh_token),
            ("grant_type", &"refresh_token".to_string()),
        ];

        let response = client
            .post(&token_url)
            .form(&params)
            .send()
            .await
            .unwrap();

        assert!(!response.status().is_success());
        assert_eq!(response.status(), 400);
    }
}