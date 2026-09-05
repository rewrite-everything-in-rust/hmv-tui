//! Shared terminal UI helpers.

use indicatif::ProgressBar;
use std::time::Duration;

/// Transient spinner, the equivalent of rich's `console.status`.
pub fn spinner(msg: impl Into<String>) -> ProgressBar {
    let pb = ProgressBar::new_spinner();
    pb.set_style(
        indicatif::ProgressStyle::with_template("{spinner:.blue} {msg}")
            .expect("static template"),
    );
    pb.set_message(msg.into());
    pb.enable_steady_tick(Duration::from_millis(120));
    pb
}
