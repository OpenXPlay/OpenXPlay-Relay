// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-15

/// Dispatch the Linux receiver CLI and retain machine-readable fatal errors.
fn main() {
    if let Err(error) = openxplay_relay::cli::run() {
        let events =
            openxplay_relay::JsonEvents::new(std::sync::Arc::new(|line| println!("{line}")));
        events.emit("error", &serde_json::json!({"message":error.to_string()}));
        std::process::exit(if error.kind() == std::io::ErrorKind::InvalidInput {
            2
        } else {
            1
        });
    }
}
