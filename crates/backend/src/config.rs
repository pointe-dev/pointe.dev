use std::{fs, path::PathBuf};

use uuid::Uuid;

fn read_trimmed_file(path: &PathBuf) -> Option<String> {
    fs::read_to_string(path)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn write_secret_file(path: &PathBuf, secret: &str, label: &str) {
    if let Some(parent) = path.parent() {
        if let Err(e) = fs::create_dir_all(parent) {
            tracing::warn!("{label} secret parent dir could not be created at {}: {e}", parent.display());
            return;
        }
    }

    if let Err(e) = fs::write(path, secret) {
        tracing::warn!("{label} secret could not be persisted at {}: {e}", path.display());
    } else {
        tracing::info!("{label} secret initialized at {}", path.display());
    }
}

fn generated_secret() -> String {
    format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple())
}

fn load_secret_string(env_name: &str, file_env_name: &str, default_path: &str, label: &str) -> String {
    if let Ok(value) = std::env::var(env_name) {
        return value;
    }

    let path = std::env::var(file_env_name)
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(default_path));

    if let Some(value) = read_trimmed_file(&path) {
        return value;
    }

    let secret = generated_secret();
    tracing::warn!(
        "{label} not set — generated a new secret (persist it with {file_env_name} or {env_name})"
    );
    write_secret_file(&path, &secret, label);
    secret
}

pub fn load_session_secret() -> Vec<u8> {
    load_secret_string(
        "SESSION_SECRET",
        "SESSION_SECRET_FILE",
        "./secrets/session_secret.txt",
        "SESSION_SECRET",
    )
    .into_bytes()
}

pub fn load_admin_ingest_token() -> Option<String> {
    match std::env::var("ADMIN_INGEST_TOKEN") {
        Ok(value) if !value.trim().is_empty() => Some(value),
        _ => {
            let path = std::env::var("ADMIN_INGEST_TOKEN_FILE")
                .map(PathBuf::from)
                .unwrap_or_else(|_| PathBuf::from("./secrets/admin_ingest_token.txt"));
            read_trimmed_file(&path)
        }
    }
}
