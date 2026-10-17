// SPDX-FileCopyrightText: DiPlay and xcertplay contributors
// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: GPL-3.0-only
// Derived from DiPlay (https://github.com/shihabal3amri/DiPlay), based on xcertplay (https://github.com/shilapi/xcertplay). Modified by OpenXPlay.org on 2026-10-15.

use crate::Result;
use std::collections::BTreeMap;
#[derive(Clone, Debug)]
pub struct RtspRequest {
    pub method: String,
    pub path: String,
    pub protocol: String,
    pub headers: BTreeMap<String, String>,
    pub body: Vec<u8>,
}
#[derive(Clone, Debug, Default)]
pub struct RtspResponse {
    pub protocol: String,
    pub status: u16,
    pub status_text: String,
    pub headers: BTreeMap<String, String>,
    pub body: Vec<u8>,
}
#[derive(Clone, Debug)]
pub struct ParsedRequest {
    pub messages: Vec<RtspRequest>,
    pub rest: Vec<u8>,
}
pub struct RtspMessage;
impl RtspMessage {
    pub fn parse_messages(buffer: &[u8]) -> Result<ParsedRequest> {
        let mut messages = Vec::new();
        let mut offset = 0;
        while let Some(end) = buffer[offset..].windows(4).position(|w| w == b"\r\n\r\n") {
            let end = offset + end;
            let head: String = buffer[offset..end]
                .iter()
                .map(|b| if b.is_ascii() { *b as char } else { '\u{fffd}' })
                .collect();
            let mut lines = head.split("\r\n");
            let mut request_line = lines.next().unwrap_or("").splitn(3, ' ');
            let method = request_line.next().unwrap_or("").to_string();
            let path = request_line.next().unwrap_or("").to_string();
            let protocol = request_line.next().unwrap_or("RTSP/1.0").to_string();
            let mut headers = BTreeMap::new();
            for line in lines {
                if let Some((key, value)) = line.split_once(':') {
                    headers.insert(key.trim().to_lowercase(), value.trim().to_string());
                }
            }
            let len = headers
                .get("content-length")
                .and_then(|s| s.parse::<i32>().ok())
                .unwrap_or(0)
                .max(0) as usize;
            let start = end + 4;
            let body_end = start
                .checked_add(len)
                .ok_or_else(|| std::io::Error::other("RTSP content length overflow"))?;
            if body_end > buffer.len() {
                break;
            }
            messages.push(RtspRequest {
                method,
                path,
                protocol,
                headers,
                body: buffer[start..body_end].to_vec(),
            });
            offset = body_end;
        }
        Ok(ParsedRequest {
            messages,
            rest: buffer[offset..].to_vec(),
        })
    }
    pub fn build_response(request: &RtspRequest, response: &RtspResponse) -> Result<Vec<u8>> {
        let protocol = if response.protocol.is_empty() {
            &request.protocol
        } else {
            &response.protocol
        };
        let status = if response.status == 0 {
            200
        } else {
            response.status
        };
        let text = if response.status_text.is_empty() {
            match status {
                400 => "Bad Request",
                404 => "Not Found",
                500 => "Internal Server Error",
                _ => "OK",
            }
        } else {
            &response.status_text
        };
        let mut headers = response.headers.clone();
        if let Some(cseq) = request.headers.get("cseq") {
            headers.insert("CSeq".into(), cseq.clone());
        }
        headers.insert("Content-Length".into(), response.body.len().to_string());
        let mut head = format!("{protocol} {status} {text}\r\n");
        for (k, v) in headers {
            head.push_str(&format!("{k}: {v}\r\n"));
        }
        head.push_str("\r\n");
        let mut out = crate::bytes::ascii_bytes(&head)?;
        out.extend_from_slice(&response.body);
        Ok(out)
    }
}
