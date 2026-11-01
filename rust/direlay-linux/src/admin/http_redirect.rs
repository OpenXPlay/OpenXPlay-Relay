// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-31

use super::{Response, MAX_BODY, MAX_HEADERS};
use std::{
    collections::BTreeMap,
    io::Read,
    net::TcpStream,
    time::{Duration, Instant},
};

/// Plain HTTP has one total read budget, including clients that send a byte at a time.
pub(super) fn serve(socket: &mut TcpStream, budget: Duration) {
    let deadline = Instant::now() + budget;
    let read = |socket: &mut TcpStream, bytes: &mut [u8]| -> Result<usize, u16> {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(400);
        }
        socket
            .set_read_timeout(Some(remaining))
            .map_err(|_| 400u16)?;
        socket.read(bytes).map_err(|_| 400u16)
    };
    let request = (|| {
        let mut header = Vec::new();
        while !header.ends_with(b"\r\n\r\n") {
            if header.len() > MAX_HEADERS {
                return Err(413);
            }
            let mut byte = [0];
            if read(socket, &mut byte)? == 0 {
                return Err(400);
            }
            header.push(byte[0]);
        }
        let (location, mut length) = redirect(&header)?;
        let mut buffer = [0; 1024];
        while length > 0 {
            let count = read(socket, &mut buffer[..length.min(1024)])?;
            if count == 0 {
                return Err(400);
            }
            length -= count;
        }
        Ok(location)
    })();
    let response = match request {
        Ok(location) => Response {
            status: 308,
            headers: BTreeMap::from([("Location".into(), location)]),
            body: Vec::new(),
            files: Vec::new(),
        },
        Err(status) => Response::error(status, "error.request"),
    };
    let _ = response.write(socket);
    let _ = socket.shutdown(std::net::Shutdown::Write);
}
fn redirect(header: &[u8]) -> Result<(String, usize), u16> {
    let text = std::str::from_utf8(header).map_err(|_| 400u16)?;
    let mut lines = text.split("\r\n");
    let first: Vec<_> = lines.next().ok_or(400u16)?.split(' ').collect();
    if first.len() != 3
        || !["HTTP/1.0", "HTTP/1.1"].contains(&first[2])
        || first[0].is_empty()
        || !first[0].bytes().all(|b| b.is_ascii_uppercase())
        || !first[1].starts_with('/')
        || first[1].bytes().any(|b| b <= 32 || b == 127)
    {
        return Err(400);
    }
    let mut headers = BTreeMap::new();
    for line in lines.filter(|line| !line.is_empty()) {
        let (key, value) = line.split_once(':').ok_or(400u16)?;
        if key.is_empty()
            || !key.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
            || value.bytes().any(|b| b < 32 && b != b'\t' || b == 127)
            || headers
                .insert(key.to_ascii_lowercase(), value.trim())
                .is_some()
        {
            return Err(400);
        }
    }
    let host = headers.get("host").ok_or(400u16)?;
    if host.is_empty()
        || host
            .bytes()
            .any(|b| b <= 32 || b == 127 || b"/\\?#@".contains(&b))
        || headers.contains_key("transfer-encoding")
    {
        return Err(400);
    }
    let length = headers
        .get("content-length")
        .map(|value| value.parse::<usize>())
        .transpose()
        .map_err(|_| 400u16)?
        .unwrap_or(0);
    if length > MAX_BODY {
        return Err(413);
    }
    Ok((format!("https://{host}{}", first[1]), length))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{Read, Write},
        net::TcpListener,
    };
    #[test]
    fn preserves_host_port_path_and_query_for_http_1_x() {
        for (host, version) in [
            ("openxplay-car.local:6975", "HTTP/1.1"),
            ("openxplay-admin.local", "HTTP/1.0"),
            ("[::1]:6975", "HTTP/1.1"),
        ] {
            let header = format!("GET /settings?q=radio HTTP/1.1\r\nHost: {host}\r\n\r\n")
                .replace("HTTP/1.1", version);
            assert_eq!(
                redirect(header.as_bytes()).unwrap(),
                (format!("https://{host}/settings?q=radio"), 0)
            );
        }
        for header in [
            "GET / HTTP/1.1\r\nHost: a\r\nHost: b\r\n\r\n",
            "GET / HTTP/1.1\r\nHost: a/b\r\n\r\n",
            "GET / HTTP/1.1\r\n\r\n",
        ] {
            assert_eq!(redirect(header.as_bytes()), Err(400));
        }
    }
    fn exchange(request: Vec<u8>, budget: Duration, keep_open: bool) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let mut client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let worker = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            serve(&mut socket, budget);
        });
        client.write_all(&request).unwrap();
        if !keep_open {
            client.shutdown(std::net::Shutdown::Write).unwrap();
        }
        client
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let mut response = String::new();
        let _ = client.read_to_string(&mut response);
        worker.join().unwrap();
        response
    }
    #[test]
    fn sends_308_and_closes_oversized_and_slow_requests() {
        let response = exchange(
            b"GET /path HTTP/1.1\r\nHost: 192.168.5.201:6975\r\n\r\n".to_vec(),
            Duration::from_secs(1),
            false,
        );
        assert!(response.starts_with("HTTP/1.1 308 Permanent Redirect\r\n"));
        assert!(response.contains("Location: https://192.168.5.201:6975/path\r\n"));
        assert!(response.contains("Connection: close\r\n"));
        assert!(
            exchange(vec![b'x'; MAX_HEADERS + 1], Duration::from_secs(5), false)
                .starts_with("HTTP/1.1 413")
        );
        let started = Instant::now();
        assert!(
            exchange(b"G".to_vec(), Duration::from_millis(50), true).starts_with("HTTP/1.1 400")
        );
        assert!(started.elapsed() < Duration::from_secs(2));
    }
}
