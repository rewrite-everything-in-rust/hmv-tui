//! Authenticated HTTP session against hackmyvm.eu.
//! Ported from AuthManager.get_session (hmv/modules/auth.py).

use anyhow::{Context, Result};
use reqwest::Client;

use crate::config::ConfigManager;

pub const BASE_URL: &str = "https://hackmyvm.eu";
const LOGIN_PATH: &str = "/login/auth.php";
const USER_AGENT: &str = concat!(
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) ",
    "AppleWebKit/537.36 (KHTML, like Gecko) ",
    "Chrome/120.0.0.0 Safari/537.36 HMV-CLI/",
    env!("CARGO_PKG_VERSION")
);

/// A logged-in session. Cloning is cheap: reqwest::Client shares one
/// connection pool and cookie jar internally.
#[derive(Clone)]
pub struct HmvSession {
    client: Client,
}

impl HmvSession {
    /// Runs a GET through the authenticated client and returns the body text.
    pub async fn get(&self, path: &str) -> Result<String> {
        let resp = self
            .client
            .get(format!("{BASE_URL}{path}"))
            .send()
            .await
            .context("Connection error")?;
        Ok(resp.text().await?)
    }

    /// Runs a form POST through the authenticated client and returns the body text.
    pub async fn post_form(&self, path: &str, form: &[(&str, &str)]) -> Result<String> {
        let resp = self
            .client
            .post(format!("{BASE_URL}{path}"))
            .form(form)
            .send()
            .await
            .context("Connection error")?;
        Ok(resp.text().await?)
    }
}

/// Logs in using stored credentials and returns the authenticated session.
pub async fn login(cfg: &ConfigManager) -> Result<HmvSession> {
    let (username, password) = cfg.load_credentials()?;

    let client = Client::builder()
        .user_agent(USER_AGENT)
        .timeout(std::time::Duration::from_secs(60))
        .connect_timeout(std::time::Duration::from_secs(15))
        .cookie_store(true)
        .redirect(reqwest::redirect::Policy::limited(10))
        .build()
        .context("Failed to build HTTP client")?;

    let resp = client
        .post(format!("{BASE_URL}{LOGIN_PATH}"))
        .form(&[("admin", username.as_str()), ("password_usuario", password.as_str())])
        .send()
        .await
        .context("Connection error")?;

    let body = resp.text().await?;

    if body.contains("Logout") {
        return Ok(HmvSession { client });
    }

    Err(crate::modules::HmvError::AuthFailed.into())
}
