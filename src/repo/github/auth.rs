use std::process::Command;

use anyhow::{Result, anyhow};

fn token_from_env_var(name: &str) -> Result<Option<String>> {
    match std::env::var(name) {
        Ok(raw) => {
            let trimmed = raw.trim().to_string();
            if trimmed.is_empty() {
                return Err(anyhow!(
                    "GitHub token env {name} is empty after trimming; please re-export"
                ));
            }
            Ok(Some(trimmed))
        }
        Err(std::env::VarError::NotPresent) => Ok(None),
        Err(e) => Err(anyhow!("failed to read env {name}: {e}")),
    }
}

fn token_from_gh_auth_token() -> Result<String> {
    let mut cmd = Command::new("gh");
    cmd.args(["auth", "token"]);

    if let Ok(host) = std::env::var("GH_HOST") {
        let host = host.trim();
        if !host.is_empty() {
            cmd.args(["--hostname", host]);
        }
    }

    let output = cmd
        .output()
        .map_err(|e| anyhow!("failed to execute `gh auth token`: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!(
            "`gh auth token` failed (exit {}): {}",
            output.status,
            stderr.trim()
        ));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let token = stdout.trim();
    if token.is_empty() {
        return Err(anyhow!("`gh auth token` returned empty stdout"));
    }
    Ok(token.to_string())
}

/// Resolve GitHub token with env-first fallback to `gh auth token`.
///
/// Priority:
/// 1) `GITHUB_TOKEN`
/// 2) `gh auth token` (optionally with `GH_HOST`)
pub fn resolve_github_token_env_then_gh() -> Result<String> {
    if let Some(token) = token_from_env_var("GITHUB_TOKEN")? {
        return Ok(token);
    }
    token_from_gh_auth_token()
}
