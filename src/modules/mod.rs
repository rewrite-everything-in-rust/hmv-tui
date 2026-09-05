//! Shared application state and error types.

pub mod flag;
pub mod machines;
pub mod releases;
pub mod session;
pub mod stats;
pub mod writeups;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum HmvError {
    #[error("Authentication failed. Please check your username and password.")]
    AuthFailed,
    #[error("Error: Machine '{0}' not found.")]
    MachineNotFound(String),
}
