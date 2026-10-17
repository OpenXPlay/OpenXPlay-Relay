// SPDX-FileCopyrightText: DiPlay and xcertplay contributors
// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: GPL-3.0-only
// Derived from DiPlay (https://github.com/shihabal3amri/DiPlay), based on xcertplay (https://github.com/shilapi/xcertplay). Modified by OpenXPlay.org on 2026-10-15.

use crate::*;

/// `Iap2TraceDirection` states; preserve wire values in its codec.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Iap2TraceDirection {
    Tx,
    Rx,
}
/// State owned by `Iap2FrameFormatter`; construct through its factory.
pub struct Iap2FrameFormatter {
    _private: (),
}
impl Iap2FrameFormatter {
    pub fn format(direction: Iap2TraceDirection, frame: &Iap2Frame) -> String {
        let endpoint = Iap2Endpoints::by_id(frame.message_id);
        let body = frame.body();
        let label = if direction == Iap2TraceDirection::Tx {
            "TX"
        } else {
            "RX"
        };
        let name = endpoint
            .as_ref()
            .map(|e| format!(" {}", e.name))
            .unwrap_or_default();
        let desc = body
            .as_ref()
            .map(|p| format!("{} params", p.parameters.len()))
            .unwrap_or_else(|_| "malformed".into());
        let mut out = format!(
            "IAP2 {label} [] 0x{:04x}{name} frame={}B body={desc}",
            frame.message_id,
            frame.payload.len() + 6
        );
        match body {
            Ok(p) => append_parameters(
                &mut out,
                &p,
                endpoint
                    .as_ref()
                    .map(|e| e.fields.as_slice())
                    .unwrap_or(&[]),
                1,
            ),
            Err(_) => out.push_str("\n  body=malformed TLV"),
        };
        out.push_str(&format!("\n  raw-body={}", hex_preview(&frame.payload)));
        out
    }
    pub fn format_with_context(
        direction: Iap2TraceDirection,
        context: &str,
        frame: &Iap2Frame,
    ) -> String {
        Self::format(direction, frame).replacen("[]", &format!("[{context}]"), 1)
    }

    pub fn format_failure(direction: Iap2TraceDirection, bytes: &[u8], reason: &str) -> String {
        let label = if direction == Iap2TraceDirection::Tx {
            "TX"
        } else {
            "RX"
        };
        let id = if bytes.len() >= 6 {
            let id = u16::from_be_bytes([bytes[4], bytes[5]]);
            format!(
                " 0x{id:04x}{}",
                Iap2Endpoints::by_id(id)
                    .map(|e| format!(" {}", e.name))
                    .unwrap_or_default()
            )
        } else {
            String::new()
        };
        format!("IAP2 {label} []{id} FAILED: {reason}")
    }
}

/// Protocol message, field, or framing constant.
pub const MAX_PARAMETERS: u64 = 80;
/// Protocol message, field, or framing constant.
pub const MAX_STRING_CHARS: u64 = 240;
/// Protocol message, field, or framing constant.
pub const MAX_HEX_BYTES: u64 = 24;

fn hex_preview(bytes: &[u8]) -> String {
    if bytes.is_empty() {
        return "<empty>".into();
    }
    let text = bytes
        .iter()
        .take(24)
        .map(|v| format!("{v:02x}"))
        .collect::<Vec<_>>()
        .join(" ");
    if bytes.len() > 24 {
        format!("{text} ... (+{}B)", bytes.len() - 24)
    } else {
        text
    }
}
fn quote(value: &str) -> String {
    let truncated = value.chars().count() > 240;
    let mut value = value.chars().take(240).collect::<String>();
    if truncated {
        value.push_str("...");
    }
    format!(
        "\"{}\"",
        value
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('\r', "\\r")
            .replace('\n', "\\n")
            .replace('\t', "\\t")
    )
}
fn bytes_value(b: &[u8]) -> String {
    format!("bytes={} [{}]", b.len(), hex_preview(b))
}
fn opaque(b: &[u8]) -> String {
    if b.is_empty() {
        return "void".into();
    }
    if b.last() == Some(&0) && b[..b.len() - 1].iter().all(|v| *v >= 32 || *v == 9) {
        if let Ok(s) = std::str::from_utf8(&b[..b.len() - 1]) {
            return quote(s);
        }
    }
    let n = match b.len() {
        1 | 2 | 4 | 8 => b.iter().fold(0u64, |n, v| (n << 8) | *v as u64),
        _ => return bytes_value(b),
    };
    format!("u{}={n} (0x{})", b.len() * 8, hex_preview(b))
}
fn append_parameters(
    out: &mut String,
    body: &Iap2ParameterList,
    specs: &[Iap2FieldSpec],
    depth: usize,
) {
    for p in body.parameters.iter().take(80) {
        let spec = specs.iter().find(|s| s.id == p.id);
        let name = spec.map(|s| s.name.as_str()).unwrap_or("param");
        let value = spec
            .and_then(|s| known(&p.payload, s, depth).ok())
            .unwrap_or_else(|| opaque(&p.payload));
        out.push_str(&format!(
            "\n{}[{}] {name}: {value}",
            "  ".repeat(depth),
            p.id
        ));
    }
    if body.parameters.len() > 80 {
        out.push_str(&format!(
            "\n{}... {} more parameters",
            "  ".repeat(depth),
            body.parameters.len() - 80
        ));
    }
}
fn known(b: &[u8], s: &Iap2FieldSpec, depth: usize) -> Result<String> {
    use Iap2WireType::*;
    let scalar =
        |name: &str, n: usize, value: std::string::String| -> Result<std::string::String> {
            if b.len() != n {
                return Err(invalid("scalar length"));
            }
            Ok(format!("{name}={value} (0x{})", hex_preview(b)))
        };
    match s.wire_type {
        Void => Ok(if b.is_empty() {
            "void".into()
        } else {
            format!("void? {}", hex_preview(b))
        }),
        U8 => scalar("u8", 1, Iap2WireCodec::read_u8(b)?.to_string()),
        I8 => scalar("i8", 1, Iap2WireCodec::read_i8(b)?.to_string()),
        U16 => scalar("u16", 2, Iap2WireCodec::read_u16(b)?.to_string()),
        I16 => scalar("i16", 2, Iap2WireCodec::read_i16(b)?.to_string()),
        U32 => scalar("u32", 4, Iap2WireCodec::read_u32(b)?.to_string()),
        I32 => scalar("i32", 4, Iap2WireCodec::read_i32(b)?.to_string()),
        U64 => scalar("u64", 8, Iap2WireCodec::read_u64(b)?.to_string()),
        U16List => Ok(format!(
            "[{}]",
            Iap2WireCodec::read_u16_list(b)?
                .iter()
                .map(|v| format!("0x{v:04x}"))
                .collect::<Vec<_>>()
                .join(", ")
        )),
        String => Ok(quote(&Iap2WireCodec::read_string(b)?)),
        StringList => Ok(format!(
            "[{}]",
            Iap2WireCodec::read_strings(b)?
                .iter()
                .map(|v| quote(v))
                .collect::<Vec<_>>()
                .join(", ")
        )),
        Bytes => Ok(bytes_value(b)),
        Opaque => Ok(opaque(b)),
        Group => {
            let body = Iap2ParameterList::parse(b)?;
            if body.parameters.is_empty() {
                return Ok("{}".into());
            }
            let mut value = "{".to_owned();
            append_parameters(&mut value, &body, &s.children, depth + 1);
            value.push_str(&format!("\n{}}}", "  ".repeat(depth)));
            Ok(value)
        }
    }
}
