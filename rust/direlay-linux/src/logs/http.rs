// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-27

//! Verified HTTPS, single POST, cancellable sockets and progress after successful writes.
use super::jobs::Transfer;
use crate::public_name::http::{Failure, Request, Response};
use std::{
    io::{Read, Write},
    net::TcpStream,
    ops::Range,
    sync::Arc,
    time::{Duration, Instant},
};
use ureq::unversioned::{
    resolver::DefaultResolver,
    transport::{
        Buffers, ConnectionDetails, Connector, LazyBuffers, NextTimeout, RustlsConnector, Transport,
    },
};

pub trait Http: Send + Sync {
    fn send(
        &self,
        request: Request,
        transfer: &Arc<Transfer>,
        payload: Range<usize>,
    ) -> Result<Response, Failure>;
}
pub(crate) struct Client {
    timeout: Duration,
    limit: u64,
    user_agent: String,
}
impl Client {
    pub fn new(timeout: Duration, limit: u64, user_agent: &str) -> Self {
        Self {
            timeout,
            limit,
            user_agent: user_agent.into(),
        }
    }
}
fn failure(error: ureq::Error) -> Failure {
    match error {
        ureq::Error::Timeout(_) => Failure::Timeout,
        ureq::Error::BodyExceedsLimit(_) => Failure::TooLarge,
        ureq::Error::RequireHttpsOnly(_) => Failure::Refused,
        _ => Failure::Unreachable,
    }
}
impl Http for Client {
    fn send(
        &self,
        request: Request,
        transfer: &Arc<Transfer>,
        payload: Range<usize>,
    ) -> Result<Response, Failure> {
        self.send_request(request, transfer, payload, true)
    }
}
impl Client {
    fn send_request(
        &self,
        request: Request,
        transfer: &Arc<Transfer>,
        payload: Range<usize>,
        https_only: bool,
    ) -> Result<Response, Failure> {
        let is_post = request.method == "POST";
        let body = request.body.unwrap_or_default();
        let config = ureq::Agent::config_builder()
            .proxy(None)
            .https_only(https_only)
            .http_status_as_error(false)
            .max_redirects(0)
            .max_redirects_will_error(false)
            .user_agent(&self.user_agent)
            .timeout_global(Some(self.timeout))
            .timeout_connect(Some(self.timeout.min(Duration::from_secs(30))))
            .build();
        let connector = Socket(transfer.clone(), self.timeout.min(Duration::from_secs(30)))
            .chain(RustlsConnector::default())
            .chain(Progress {
                transfer: transfer.clone(),
                payload,
                body_length: body.len(),
            });
        let agent = ureq::Agent::with_parts(config, connector, DefaultResolver::default());
        let mut builder = ureq::http::Request::builder()
            .method(request.method)
            .uri(request.url);
        for (key, value) in request.headers {
            builder = builder.header(key, value);
        }
        let request = builder.body(body).map_err(|_| Failure::Refused)?;
        let mut response = agent.run(request).map_err(failure)?;
        let status = response.status().as_u16();
        let headers = response
            .headers()
            .iter()
            .map(|(k, v)| {
                (
                    k.as_str().to_ascii_lowercase(),
                    String::from_utf8_lossy(v.as_bytes()).into_owned(),
                )
            })
            .collect();
        // A rejection needs only status and Retry-After; never read or expose its body.
        let body = if status != 201 && is_post {
            Vec::new()
        } else {
            response
                .body_mut()
                .with_config()
                .limit(self.limit)
                .read_to_vec()
                .map_err(failure)?
        };
        Ok(Response {
            status,
            headers,
            body,
        })
    }
}
#[derive(Debug)]
struct Socket(Arc<Transfer>, Duration);
impl<In: Transport> Connector<In> for Socket {
    type Out = SocketTransport;
    fn connect(
        &self,
        details: &ConnectionDetails,
        _: Option<In>,
    ) -> Result<Option<Self::Out>, ureq::Error> {
        let deadline = Instant::now() + self.1;
        let mut last = std::io::Error::from(std::io::ErrorKind::NotConnected);
        for address in &details.addrs {
            if self.0.cancelled() {
                return Err(std::io::Error::from(std::io::ErrorKind::Interrupted).into());
            }
            let timeout = deadline
                .saturating_duration_since(Instant::now())
                .min(self.1 / details.addrs.len().max(1) as u32);
            if timeout.is_zero() {
                return Err(ureq::Error::Timeout(ureq::Timeout::Connect));
            }
            match TcpStream::connect_timeout(address, timeout) {
                Ok(socket) => {
                    self.0.attach(socket.try_clone()?)?;
                    socket.set_nodelay(true)?;
                    return Ok(Some(SocketTransport {
                        socket,
                        buffers: LazyBuffers::new(
                            details.config.input_buffer_size(),
                            details.config.output_buffer_size(),
                        ),
                    }));
                }
                Err(error) => last = error,
            }
        }
        Err(last.into())
    }
}
#[derive(Debug)]
struct Progress {
    transfer: Arc<Transfer>,
    payload: Range<usize>,
    body_length: usize,
}
impl<In: Transport> Connector<In> for Progress {
    type Out = ProgressTransport<In>;
    fn connect(
        &self,
        _: &ConnectionDetails,
        chained: Option<In>,
    ) -> Result<Option<Self::Out>, ureq::Error> {
        Ok(chained.map(|inner| ProgressTransport {
            inner,
            transfer: self.transfer.clone(),
            payload: self.payload.clone(),
            body_length: self.body_length,
            header: Vec::new(),
            offset: None,
        }))
    }
}
#[derive(Debug)]
struct ProgressTransport<T> {
    inner: T,
    transfer: Arc<Transfer>,
    payload: Range<usize>,
    body_length: usize,
    header: Vec<u8>,
    offset: Option<usize>,
}
impl<T: Transport> Transport for ProgressTransport<T> {
    fn buffers(&mut self) -> &mut dyn Buffers {
        self.inner.buffers()
    }
    fn transmit_output(&mut self, amount: usize, timeout: NextTimeout) -> Result<(), ureq::Error> {
        if self.transfer.cancelled() {
            return Err(std::io::Error::from(std::io::ErrorKind::Interrupted).into());
        }
        let offset = if let Some(offset) = self.offset {
            Some(offset + amount)
        } else {
            self.header
                .extend_from_slice(&self.inner.buffers().output()[..amount]);
            self.header
                .windows(4)
                .position(|v| v == b"\r\n\r\n")
                .map(|end| self.header.len() - end - 4)
        };
        self.inner.transmit_output(amount, timeout)?;
        self.offset = offset;
        if let Some(offset) = offset {
            self.header.clear();
            self.transfer.progress(
                offset
                    .saturating_sub(self.payload.start)
                    .min(self.payload.len()) as u64,
                offset >= self.body_length,
            );
        }
        Ok(())
    }
    fn await_input(&mut self, timeout: NextTimeout) -> Result<bool, ureq::Error> {
        self.inner.await_input(timeout)
    }
    fn is_open(&mut self) -> bool {
        !self.transfer.cancelled() && self.inner.is_open()
    }
    fn is_tls(&self) -> bool {
        self.inner.is_tls()
    }
}

// This connection is never pooled: dropping the per-request agent closes the socket.
#[derive(Debug)]
struct SocketTransport {
    socket: TcpStream,
    buffers: LazyBuffers,
}
fn io_failure(error: std::io::Error, timeout: NextTimeout) -> ureq::Error {
    if matches!(
        error.kind(),
        std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
    ) {
        ureq::Error::Timeout(timeout.reason)
    } else {
        error.into()
    }
}
impl Transport for SocketTransport {
    fn buffers(&mut self) -> &mut dyn Buffers {
        &mut self.buffers
    }
    fn transmit_output(&mut self, amount: usize, timeout: NextTimeout) -> Result<(), ureq::Error> {
        self.socket
            .set_write_timeout(timeout.not_zero().map(|t| *t))?;
        self.socket
            .write_all(&self.buffers.output()[..amount])
            .map_err(|e| io_failure(e, timeout))
    }
    fn await_input(&mut self, timeout: NextTimeout) -> Result<bool, ureq::Error> {
        self.socket
            .set_read_timeout(timeout.not_zero().map(|t| *t))?;
        let read = self
            .socket
            .read(self.buffers.input_append_buf())
            .map_err(|e| io_failure(e, timeout))?;
        self.buffers.input_appended(read);
        Ok(read > 0)
    }
    fn is_open(&mut self) -> bool {
        false
    }
}
// Update checking also uses this bounded HTTP client, without an upload job.
impl crate::public_name::http::Http for Client {
    fn send(&self, request: Request) -> Result<Response, Failure> {
        Http::send(
            self,
            request,
            &Transfer::new(String::new(), String::new(), 0),
            0..0,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{io::Read, net::TcpListener, sync::mpsc};
    #[test]
    fn cancellation_closes_socket_while_awaiting_receipt_after_all_bytes_sent() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let (sent, received) = mpsc::channel();
        let server = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut bytes = Vec::new();
            let mut buffer = [0; 4096];
            loop {
                let size = socket.read(&mut buffer).unwrap();
                assert!(size > 0);
                bytes.extend_from_slice(&buffer[..size]);
                if let Some(end) = bytes.windows(4).position(|v| v == b"\r\n\r\n") {
                    if bytes.len() >= end + 4 + 3 {
                        break;
                    }
                }
            }
            sent.send(()).unwrap();
            assert_eq!(socket.read(&mut buffer).unwrap(), 0);
        });
        let job = Transfer::new("id".into(), "export".into(), 3);
        let transfer = job.clone();
        let worker = std::thread::spawn(move || {
            Client::new(Duration::from_secs(5), 65536, "test").send_request(
                Request::with_body("POST", format!("http://{address}/upload"), b"zip".to_vec()),
                &transfer,
                0..3,
                false,
            )
        });
        received.recv_timeout(Duration::from_secs(5)).unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while job.status().phase != "AwaitingReceipt" {
            assert!(Instant::now() < deadline);
            std::thread::yield_now();
        }
        let status = job.status();
        assert_eq!(status.sent, 3);
        assert_eq!(status.state, "running");
        assert!(status.receipt.is_none());
        job.cancel();
        assert!(worker.join().unwrap().is_err());
        server.join().unwrap();
    }
}
