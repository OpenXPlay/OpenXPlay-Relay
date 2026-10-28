// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-27

//! One explicit upload attempt. Cancellation closes its socket; a late validated receipt survives.
use super::upload::{Receipt, UploadError};
use serde::Serialize;
use std::{
    net::{Shutdown, TcpStream},
    sync::{Arc, Mutex},
    time::Instant,
};

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub id: String,
    pub export_id: String,
    pub state: &'static str,
    pub phase: &'static str,
    pub sent: u64,
    pub total: u64,
    pub elapsed_millis: u64,
    pub receipt: Option<Receipt>,
    pub error: Option<Failure>,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Failure {
    pub code: &'static str,
    pub status: Option<u16>,
    pub retry_after_seconds: Option<u64>,
}
#[derive(Debug)]
struct State {
    status: Status,
    socket: Option<TcpStream>,
    finished: Option<Instant>,
    worker_done: bool,
}
#[derive(Debug)]
pub struct Transfer {
    started: Instant,
    state: Mutex<State>,
}
impl Transfer {
    pub fn new(id: String, export_id: String, total: u64) -> Arc<Self> {
        Arc::new(Self {
            started: Instant::now(),
            state: Mutex::new(State {
                status: Status {
                    id,
                    export_id,
                    state: "running",
                    phase: "Connecting",
                    sent: 0,
                    total,
                    elapsed_millis: 0,
                    receipt: None,
                    error: None,
                },
                socket: None,
                finished: None,
                worker_done: false,
            }),
        })
    }
    pub fn status(&self) -> Status {
        let state = self.state.lock().unwrap();
        let mut status = state.status.clone();
        status.elapsed_millis = state
            .finished
            .unwrap_or_else(Instant::now)
            .saturating_duration_since(self.started)
            .as_millis() as u64;
        status
    }
    pub fn worker_done(&self) -> bool {
        self.state.lock().unwrap().worker_done
    }
    pub fn cancelled(&self) -> bool {
        self.state.lock().unwrap().status.state == "cancelled"
    }
    pub fn attach(&self, socket: TcpStream) -> std::io::Result<()> {
        let mut state = self.state.lock().unwrap();
        if state.status.state == "cancelled" {
            let _ = socket.shutdown(Shutdown::Both);
            return Err(std::io::ErrorKind::Interrupted.into());
        }
        state.socket = Some(socket);
        Ok(())
    }
    pub fn progress(&self, sent: u64, awaiting: bool) {
        let mut state = self.state.lock().unwrap();
        if state.status.state != "running" {
            return;
        }
        state.status.sent = sent.min(state.status.total);
        state.status.phase = if awaiting {
            "AwaitingReceipt"
        } else {
            "Sending"
        };
    }
    pub fn cancel(&self) -> Status {
        {
            let mut state = self.state.lock().unwrap();
            if state.status.state == "running" {
                state.status.state = "cancelled";
                state.finished = Some(Instant::now());
                if let Some(socket) = state.socket.take() {
                    let _ = socket.shutdown(Shutdown::Both);
                }
            }
        }
        self.status()
    }
    pub fn finish(&self, result: Result<Receipt, UploadError>) {
        let mut state = self.state.lock().unwrap();
        state.worker_done = true;
        state.socket = None;
        state.finished.get_or_insert_with(Instant::now);
        match result {
            Ok(receipt) => {
                state.status.receipt = Some(receipt);
                if state.status.state == "running" {
                    state.status.state = "completed";
                }
            }
            Err(error) if state.status.state == "running" => {
                let (status, retry_after_seconds) = match error {
                    UploadError::Http {
                        status,
                        retry_after_seconds,
                    } => (Some(status), retry_after_seconds),
                    _ => (None, None),
                };
                state.status.state = "failed";
                state.status.error = Some(Failure {
                    code: error.code(),
                    status,
                    retry_after_seconds,
                });
            }
            _ => {}
        }
    }
}
