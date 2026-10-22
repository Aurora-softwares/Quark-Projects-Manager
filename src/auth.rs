//! Account management and GitHub OAuth device flow. Tokens are handed to `gh`
//! over stdin and never written to the application's settings or logs.
use serde_json::Value;
use std::{
    io::Write,
    process::{Command, Stdio},
    sync::atomic::{AtomicBool, Ordering},
    thread,
    time::{Duration, Instant},
};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Account {
    pub login: String,
    pub active: bool,
    pub valid: bool,
}

#[derive(Clone)]
pub struct DeviceCode {
    pub code: String,
    pub uri: String,
    device_code: String,
    expires_in: u64,
    interval: u64,
}

// Avoid accidentally disclosing the OAuth device secret through debug logging.
impl std::fmt::Debug for DeviceCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DeviceCode")
            .field("code", &self.code)
            .field("uri", &self.uri)
            .field("device_code", &"[redacted]")
            .field("expires_in", &self.expires_in)
            .field("interval", &self.interval)
            .finish()
    }
}

fn gh_binary() -> std::ffi::OsString {
    if let Some(custom) = std::env::var_os("QUARK_GH_BINARY") {
        return custom;
    }
    #[cfg(target_os = "macos")]
    for path in ["/opt/homebrew/bin/gh", "/usr/local/bin/gh"] {
        if std::path::Path::new(path).is_file() {
            return path.into();
        }
    }
    #[cfg(windows)]
    if let Some(program_files) = std::env::var_os("ProgramFiles") {
        let candidate = std::path::PathBuf::from(program_files)
            .join("GitHub CLI")
            .join("gh.exe");
        if candidate.is_file() {
            return candidate.into_os_string();
        }
    }
    "gh".into()
}

fn gh(args: &[&str]) -> Result<String, String> {
    let result = Command::new(gh_binary())
        .args(args)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("GitHub CLI unavailable: {e}"))?;
    if !result.status.success() {
        return Err(String::from_utf8_lossy(&result.stderr).trim().to_owned());
    }
    Ok(String::from_utf8_lossy(&result.stdout).trim().to_owned())
}

pub fn parse_accounts(value: &Value) -> Vec<Account> {
    let mut accounts = Vec::new();
    if let Some(entries) = value.pointer("/hosts/github.com").and_then(Value::as_array) {
        for value in entries {
            if let Some(login) = value.get("login").and_then(Value::as_str) {
                if !login.is_empty() {
                    accounts.push(Account {
                        login: login.into(),
                        active: value
                            .get("active")
                            .and_then(Value::as_bool)
                            .unwrap_or(false),
                        valid: value
                            .get("state")
                            .and_then(Value::as_str)
                            .is_none_or(|s| s == "success"),
                    });
                }
            }
        }
    }
    accounts
}

pub fn list_accounts() -> Result<Vec<Account>, String> {
    let raw = gh(&["auth", "status", "--json", "hosts"])?;
    let v: Value = serde_json::from_str(&raw)
        .map_err(|e| format!("Cannot parse GitHub account status: {e}"))?;
    Ok(parse_accounts(&v))
}

fn valid_login(login: &str) -> bool {
    !login.is_empty()
        && login.len() <= 39
        && !login.starts_with('-')
        && !login.ends_with('-')
        && login.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

pub fn switch_account(login: &str) -> Result<(), String> {
    if !valid_login(login) {
        return Err("Invalid GitHub username".into());
    }
    gh(&[
        "auth",
        "switch",
        "--hostname",
        "github.com",
        "--user",
        login,
    ])
    .map(|_| ())
}

pub fn logout(login: &str) -> Result<(), String> {
    if !valid_login(login) {
        return Err("Invalid GitHub username".into());
    }
    gh(&[
        "auth",
        "logout",
        "--hostname",
        "github.com",
        "--user",
        login,
    ])
    .map(|_| ())
}

fn client() -> Result<reqwest::blocking::Client, String> {
    reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(20))
        .user_agent("QuarkProjects/0.2.3")
        .build()
        .map_err(|e| e.to_string())
}

fn validate_client_id(client_id: &str) -> Result<&str, String> {
    let id = client_id.trim();
    if id.is_empty() || !id.chars().all(|c| c.is_ascii_alphanumeric()) {
        Err("Enter a GitHub OAuth App client ID (enable Device Flow in the app settings)".into())
    } else {
        Ok(id)
    }
}

fn parse_device_response(response: &Value) -> Result<DeviceCode, String> {
    if let Some(error) = response.get("error").and_then(Value::as_str) {
        return Err(format!("GitHub rejected device login: {error}"));
    }
    let required = |key: &str| {
        response
            .get(key)
            .and_then(Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| format!("Device response missing {key}"))
    };
    Ok(DeviceCode {
        code: required("user_code")?,
        uri: required("verification_uri")?,
        device_code: required("device_code")?,
        expires_in: response["expires_in"].as_u64().unwrap_or(900),
        interval: response["interval"].as_u64().unwrap_or(5).max(1),
    })
}

pub fn start_device_login(client_id: &str) -> Result<DeviceCode, String> {
    let client_id = validate_client_id(client_id)?;
    let response: Value = client()?
        .post("https://github.com/login/device/code")
        .header("Accept", "application/json")
        .form(&[("client_id", client_id), ("scope", "project repo read:org")])
        .send()
        .and_then(|r| r.error_for_status())
        .and_then(|r| r.json())
        .map_err(|e| format!("GitHub device login failed: {e}"))?;
    parse_device_response(&response)
}

fn device_error_action(error: &str) -> Result<bool, String> {
    match error {
        "authorization_pending" | "slow_down" => Ok(false),
        "access_denied" => Err("GitHub authorization was denied".into()),
        "expired_token" => Err("GitHub device code expired; start again".into()),
        "incorrect_client_credentials" => Err("The OAuth App client ID is not valid".into()),
        other => Err(format!("GitHub OAuth error: {other}")),
    }
}

pub fn finish_device_login(
    client_id: &str,
    device: &DeviceCode,
    cancelled: &AtomicBool,
) -> Result<String, String> {
    let client = client()?;
    let deadline = Instant::now() + Duration::from_secs(device.expires_in);
    let mut interval = device.interval;
    loop {
        for _ in 0..interval {
            if cancelled.load(Ordering::Relaxed) {
                return Err("GitHub sign-in cancelled".into());
            }
            if Instant::now() >= deadline {
                return Err("GitHub device code expired".into());
            }
            thread::sleep(Duration::from_secs(1));
        }
        let response: Value = client
            .post("https://github.com/login/oauth/access_token")
            .header("Accept", "application/json")
            .form(&[
                ("client_id", client_id),
                ("device_code", device.device_code.as_str()),
                ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
            ])
            .send()
            .and_then(|r| r.error_for_status())
            .and_then(|r| r.json())
            .map_err(|e| format!("GitHub OAuth polling failed: {e}"))?;
        if let Some(token) = response.get("access_token").and_then(Value::as_str) {
            return store_with_github_cli(token);
        }
        let error = response
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or("unexpected_response");
        device_error_action(error)?;
        if error == "slow_down" {
            interval = interval.saturating_add(5);
        }
    }
}

fn store_with_github_cli(token: &str) -> Result<String, String> {
    // Do not expose tokens in command arguments, UI, logs or configuration.
    let mut child = Command::new(gh_binary())
        .args(["auth", "login", "--hostname", "github.com", "--with-token"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Cannot save sign-in through GitHub CLI: {e}"))?;
    {
        let mut input = child
            .stdin
            .take()
            .ok_or("GitHub CLI did not accept token input")?;
        input
            .write_all(token.as_bytes())
            .and_then(|_| input.write_all(b"\n"))
            .map_err(|e| format!("Could not send approved token to GitHub CLI: {e}"))?;
    }
    let output = child.wait_with_output().map_err(|e| e.to_string())?;
    if !output.status.success() {
        return Err(format!(
            "GitHub CLI could not save authorization: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    gh(&["api", "user", "--jq", ".login"])
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_accounts_without_exposing_tokens() {
        let v = serde_json::json!({"hosts":{"github.com":[{"login":"alice","active":true,"state":"success","token":"secret"},{"login":"bob","active":false,"state":"error"}]}});
        let accounts = parse_accounts(&v);
        assert_eq!(accounts.len(), 2);
        assert_eq!(accounts[0].login, "alice");
        assert!(accounts[0].active);
        assert!(!accounts[1].valid);
        assert!(!format!("{accounts:?}").contains("secret"));
    }
    #[test]
    fn rejects_bad_usernames() {
        assert!(!valid_login("-x"));
        assert!(!valid_login("a b"));
        assert!(valid_login("aurora-dev"));
    }
    #[test]
    fn device_errors_are_explicit() {
        assert!(device_error_action("authorization_pending").is_ok());
        assert!(device_error_action("access_denied").is_err());
        assert!(device_error_action("expired_token").is_err());
    }
}

#[cfg(test)]
#[path = "tests/auth_extended.rs"]
mod extended_tests;
