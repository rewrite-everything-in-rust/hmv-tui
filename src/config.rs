//! Secure credential storage: username in ~/.hmv/config.json, password in the
//! OS credential vault (keyutils / Secret Service on Linux, Credential Manager
//! on Windows, Keychain on macOS). Ported from hmv/modules/auth.py.

use std::fs;
use std::io::{self, IsTerminal, Write};
use std::path::PathBuf;

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};

const SERVICE_NAME: &str = "hmv-cli";
const CONFIG_DIR_NAME: &str = ".hmv";
const CONFIG_FILE_NAME: &str = "config.json";

#[derive(Serialize, Deserialize)]
struct ConfigFile {
    username: String,
}

pub struct ConfigManager {
    config_file: PathBuf,
}

impl ConfigManager {
    pub fn new() -> Self {
        let dir = home::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(CONFIG_DIR_NAME);
        let _ = fs::create_dir_all(&dir);
        Self {
            config_file: dir.join(CONFIG_FILE_NAME),
        }
    }

    fn read_username(&self) -> Result<String> {
        let raw = fs::read_to_string(&self.config_file)
            .with_context(|| "Configuration not found. Run 'hmv config' first.")?;
        let cfg: ConfigFile = serde_json::from_str(&raw)
            .with_context(|| "Configuration file is corrupted. Run 'hmv config' again.")?;
        Ok(cfg.username)
    }

    fn keyring_entry(&self, username: &str) -> Result<keyring::Entry> {
        keyring::Entry::new(SERVICE_NAME, username)
            .map_err(|e| anyhow!("Keyring storage system not found: {e}"))
    }

    /// Persists the username on disk and the password in the OS vault.
    pub fn save_credentials(&self, username: &str, password: &str) -> Result<()> {
        let json = serde_json::to_string(&ConfigFile {
            username: username.to_string(),
        })?;
        fs::write(&self.config_file, json)
            .with_context(|| "Failed to write configuration file")?;

        let entry = self.keyring_entry(username)?;
        entry
            .set_password(password)
            .map_err(|e| anyhow!("Failed to save configuration to system vault: {e}"))?;

        println!(
            "{} Configuration saved successfully!",
            console::style("[✓]").green().bold()
        );
        Ok(())
    }

    /// Loads the username from disk and the password from the OS vault.
    pub fn load_credentials(&self) -> Result<(String, String)> {
        let username = self.read_username()?;
        let entry = self.keyring_entry(&username)?;
        match entry.get_password() {
            Ok(password) => Ok((username, password)),
            Err(keyring::Error::NoEntry) => Err(anyhow!(
                "Password not found. Please run 'hmv config' again."
            )),
            Err(e) => Err(anyhow!("Error while accessing vault: {e}")),
        }
    }
}

impl Default for ConfigManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Reads the username, honoring non-interactive (piped) stdin for testing.
pub fn prompt_username() -> Result<String> {
    print!("Username: ");
    io::stdout().flush()?;
    let mut buf = String::new();
    io::stdin().read_line(&mut buf)?;
    Ok(buf.trim().to_string())
}

/// Reads the password without echo when attached to a TTY.
pub fn prompt_password() -> Result<String> {
    if io::stdin().is_terminal() {
        Ok(rpassword::prompt_password("Password: ")?)
    } else {
        let mut buf = String::new();
        io::stdin().read_line(&mut buf)?;
        Ok(buf.trim().to_string())
    }
}
