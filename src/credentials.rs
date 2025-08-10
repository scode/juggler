use std::fs;
use std::path::PathBuf;

use log::info;
use serde::Deserialize;

use crate::config::get_juggler_dir;

#[derive(Deserialize)]
struct InstalledBlock {
    client_id: Option<String>,
    client_secret: Option<String>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum CredentialsShape {
    Google {
        installed: InstalledBlock,
    },
    Flat {
        client_id: Option<String>,
        client_secret: Option<String>,
    },
}

fn default_credentials_path() -> Option<PathBuf> {
    get_juggler_dir()
        .ok()
        .map(|dir| dir.join("google_oauth_client.json"))
}

pub fn load_client_secret_from_default_path(expected_client_id: &str) -> Option<String> {
    let Some(path) = default_credentials_path() else {
        return None;
    };
    let Ok(contents) = fs::read_to_string(&path) else {
        return None;
    };
    let Ok(parsed) = serde_json::from_str::<CredentialsShape>(&contents) else {
        return None;
    };

    let (found_id, found_secret) = match parsed {
        CredentialsShape::Google { installed } => (installed.client_id, installed.client_secret),
        CredentialsShape::Flat {
            client_id,
            client_secret,
        } => (client_id, client_secret),
    };

    match (found_id, found_secret) {
        (Some(id), Some(secret)) if id == expected_client_id => {
            info!("Loaded client_secret from {}", path.display());
            Some(secret)
        }
        // If file does not contain an id, assume it matches the intended client id
        (None, Some(secret)) => {
            info!(
                "Loaded client_secret from {} (no client_id in file)",
                path.display()
            );
            Some(secret)
        }
        _ => None,
    }
}
