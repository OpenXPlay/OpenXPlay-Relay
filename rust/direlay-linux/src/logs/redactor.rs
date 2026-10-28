// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-27

//! OpenXPlay Android's value tokens and key classes; technical network identifiers stay useful.
use hmac::{Hmac, Mac};
use sha2::Sha256;

pub struct Redactor {
    key: [u8; 32],
    journal: bool,
    known: Vec<(String, String, bool)>,
    pem_end: Option<String>,
    personal_frame: bool,
}
fn marker(text: &str) -> Option<usize> {
    if text.starts_with('[') {
        let end = text.find(']')? + 1;
        let value = &text[1..end - 1];
        let token = value.split_once('#').is_some_and(|(class, hash)| {
            let (hash, length) = hash
                .split_once(" len=")
                .map_or((hash, None), |(h, n)| (h, Some(n)));
            matches!(class, "secret" | "name" | "phone" | "email" | "text")
                && hash.len() == 6
                && hash
                    .bytes()
                    .all(|b| b.is_ascii_digit() || matches!(b, b'a'..=b'f'))
                && length.is_none_or(|n| {
                    class == "text" && !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit())
                })
        });
        let counted = ["hidden ", "redacted "].iter().any(|prefix| {
            value
                .strip_prefix(prefix)
                .and_then(|v| v.strip_suffix(" bytes"))
                .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
        });
        if token
            || counted
            || matches!(
                value,
                "secret" | "location" | "hidden" | "redacted" | "redacted location"
            )
        {
            return Some(end);
        }
    }
    if text.starts_with("<binary ") || text.starts_with("<data ") {
        let end = text.find('>')?;
        if text[..end]
            .split_once(' ')
            .and_then(|(_, v)| v.strip_suffix(" bytes"))
            .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
        {
            return Some(end + 1);
        }
    }
    None
}
fn public(value: &str) -> bool {
    value.is_empty()
        || matches!(
            value.to_ascii_lowercase().as_str(),
            "true"
                | "false"
                | "null"
                | "nil"
                | "none"
                | "empty"
                | "<empty>"
                | "\"\""
                | "''"
                | "[]"
                | "{}"
        )
        || marker(value) == Some(value.len())
}
fn word(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b >= 128
}
fn normalized(key: &str) -> String {
    key.chars()
        .filter(|c| !matches!(c, '_' | '-'))
        .flat_map(char::to_lowercase)
        .collect()
}
pub(super) fn kind(key: &str) -> Option<&'static str> {
    let mut boundaries = vec![0];
    let bytes = key.as_bytes();
    for i in 1..bytes.len() {
        if matches!(bytes[i - 1], b'_' | b'-')
            || (bytes[i].is_ascii_uppercase()
                && (bytes[i - 1].is_ascii_lowercase()
                    || bytes[i - 1].is_ascii_digit()
                    || bytes.get(i + 1).is_some_and(u8::is_ascii_lowercase)))
        {
            boundaries.push(i);
        }
    }
    for start in boundaries {
        let key = normalized(&key[start..]);
        let class = match key.as_str() {
            "password" | "passwd" | "pwd" | "passphrase" | "psk" | "presharedkey" | "secret"
            | "token" | "accesstoken" | "remotetoken" | "apikey" | "authorization" | "cookie"
            | "setcookie" | "pin" | "pincode" | "passcode" | "setupcode" | "pairingcode" => {
                "secret"
            }
            "privatekey" | "publickey" | "pk8" | "sessionkey" | "readkey" | "writekey" | "shk"
            | "ekey" | "eiv" | "pk" | "salt" | "proof" | "signature" | "certificate" | "srp"
            | "x25519" => "material",
            "latitude" | "longitude" | "lat" | "lon" | "lng" => "location",
            "devicename" | "displayname" | "contactname" | "callername" | "iphonename"
            | "bluetoothname" => "name",
            "remoteid" | "phonenumber" => "phone",
            "email" => "email",
            "title" | "artist" | "album" | "destination" | "roadname" | "messagebody" => "text",
            _ => continue,
        };
        return Some(class);
    }
    None
}
impl Redactor {
    pub fn new(key: [u8; 32]) -> Self {
        Self {
            key,
            journal: false,
            known: Vec::new(),
            pem_end: None,
            personal_frame: false,
        }
    }
    pub fn for_journal() -> Self {
        let mut value = Self::new([0; 32]);
        value.journal = true;
        value
    }
    pub fn reset_stream(&mut self) {
        self.pem_end = None;
        self.personal_frame = false;
    }
    pub fn token(&self, value: &str, class: &str) -> String {
        if public(value) {
            return value.into();
        }
        if self.journal {
            return "[secret]".into();
        }
        if class == "material" {
            return "[secret]".into();
        }
        if class == "location" {
            return "[location]".into();
        }
        let mut mac = Hmac::<Sha256>::new_from_slice(&self.key).unwrap();
        mac.update(value.as_bytes());
        let digest = mac.finalize().into_bytes();
        let suffix = if class == "text" {
            format!(" len={}", value.chars().count())
        } else {
            String::new()
        };
        format!(
            "[{class}#{:02x}{:02x}{:02x}{suffix}]",
            digest[0], digest[1], digest[2]
        )
    }
    pub fn remember(&mut self, value: &str, class: &str) {
        if public(value) {
            return;
        }
        if value.contains(['\r', '\n']) {
            for line in value.split(['\r', '\n']) {
                self.remember(line, class);
            }
        }
        let replacement = self.token(value, class);
        let bounded =
            value.bytes().all(|b| b.is_ascii_digit()) || (class != "secret" && value.len() < 8);
        let json = serde_json::to_string(value).unwrap();
        let escaped = &json[1..json.len() - 1];
        let encoded = crate::public_name::http::encode(value);
        for form in [
            value.to_string(),
            escaped.into(),
            escaped.replace('/', "\\/"),
            encoded.clone(),
            encoded.replace("%20", "+"),
            lower_percent(&encoded),
        ] {
            if !self.known.iter().any(|(v, _, _)| *v == form) {
                if self.known.len() >= 4096 {
                    self.known.remove(0);
                }
                self.known.push((form, replacement.clone(), bounded));
            }
        }
        self.known
            .sort_by_key(|(v, _, _)| std::cmp::Reverse(v.len()));
    }
    fn replace_known(&self, text: &str) -> String {
        let mut out = String::new();
        let mut at = 0;
        while at < text.len() {
            let part = &text[at..];
            if let Some(n) = marker(part) {
                out.push_str(&part[..n]);
                at += n;
                continue;
            }
            if let Some((value, replacement, _)) = self.known.iter().find(|(value, _, bounded)| {
                part.starts_with(value)
                    && (!bounded
                        || (at == 0 || !word(text.as_bytes()[at - 1]))
                            && part.as_bytes().get(value.len()).is_none_or(|b| !word(*b)))
            }) {
                out.push_str(replacement);
                at += value.len();
                continue;
            }
            let c = part.chars().next().unwrap();
            out.push(c);
            at += c.len_utf8();
        }
        out
    }
    pub fn redact(&mut self, line: &str) -> String {
        // Journal headers are diagnostic; never reinterpret their timestamps or process names.
        let (header, message) = journal_header(line);
        if let Some(start) = message.find("-----BEGIN ") {
            if let Some(end) = message[start..].find("PRIVATE KEY-----") {
                let label = &message[start + 11..start + end + 11];
                self.pem_end = Some(format!("-----END {label}-----"));
            }
        }
        if let Some(end) = &self.pem_end {
            if message.contains(end) {
                self.pem_end = None;
            }
            return format!("{header}[secret]");
        }
        if message.contains("IAP2 ") {
            self.personal_frame = message.find("0x").and_then(|at| message.get(at+2..at+6))
                .and_then(|v| u16::from_str_radix(v,16).ok()).is_some_and(|id|
                    matches!(id, 0x4154..=0x4161 | 0x4170..=0x4172 | 0x5000..=0x5003 | 0x5200..=0x5204 | 0x4c00..=0x4c09 | 0x4e09));
        } else if !message.starts_with(char::is_whitespace) {
            self.personal_frame = false;
        }
        let message = self.listed_phone_names(message);
        let mut text = self.plist(&self.replace_known(&message));
        // Raw payloads and previews can contain credentials in encodings unknown to the shell.
        text = self.keys(&text);
        text = scan(
            &text,
            |part| {
                for scheme in ["Bearer ", "Basic "] {
                    if part
                        .get(..scheme.len())
                        .is_some_and(|s| s.eq_ignore_ascii_case(scheme))
                    {
                        let rest = &part[scheme.len()..];
                        let n = value_end(rest, false);
                        return Some((
                            scheme.len() + n,
                            format!(
                                "{}{}",
                                &part[..scheme.len()],
                                self.token(&rest[..n], "secret")
                            ),
                        ));
                    }
                }
                if let Some(at) = part.find("://").filter(|at| *at < 16) {
                    let rest = &part[at + 3..];
                    if let Some(end) = rest
                        .find('@')
                        .filter(|end| !rest[..*end].contains(['/', ' ', '\t']))
                    {
                        return Some((
                            at + 3 + end + 1,
                            format!("{}{}@", &part[..at + 3], self.token(&rest[..end], "secret")),
                        ));
                    }
                }
                address(part).map(|(n, class)| (n, self.token(&part[..n], class)))
            },
            true,
        );
        format!("{header}{}", self.replace_known(&text))
    }
    fn keys(&mut self, text: &str) -> String {
        let mut out = String::new();
        let mut at = 0;
        while at < text.len() {
            let rest = &text[at..];
            if let Some(n) = marker(rest) {
                out.push_str(&rest[..n]);
                at += n;
                continue;
            }
            let b = rest.as_bytes()[0];
            if b.is_ascii_alphabetic() || b == b'_' {
                let n = rest
                    .bytes()
                    .take_while(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
                    .count();
                let key = &rest[..n];
                let tail = rest[n..].trim_start_matches(['\"', '\'']).trim_start();
                let separator = if tail.starts_with(['=', ':']) {
                    1
                } else if tail.starts_with("is ") {
                    2
                } else {
                    0
                };
                if separator > 0 {
                    let value = tail[separator..].trim_start();
                    let start = rest.len() - value.len();
                    let norm = normalized(key);
                    let binary = norm.ends_with("hex") || norm == "rawbody" || norm == "preview";
                    let class = kind(key).or_else(|| {
                        (norm == "name"
                            && (text.to_ascii_lowercase().contains("bluetooth")
                                || text.contains("/info")
                                || text.contains("AirPlay device info")))
                        .then_some("name")
                    });
                    if binary || class.is_some() {
                        let cookie = norm.ends_with("cookie");
                        let auth = norm.ends_with("authorization")
                            && (value.starts_with("Bearer ") || value.starts_with("Basic "));
                        let typed = value.starts_with("bytes=")
                            || ["u8=", "u16=", "u32=", "u64=", "i8=", "i16=", "i32=", "i64="]
                                .iter()
                                .any(|p| value.starts_with(p));
                        let n = if typed || norm == "rawbody" {
                            value.len()
                        } else if auth {
                            let at = value.find(' ').unwrap() + 1;
                            at + value_end(&value[at..], false)
                        } else {
                            value_end(value, cookie || class == Some("name"))
                        };
                        let raw = &value[..n];
                        let (bare, quote) = unquote(raw);
                        let replacement = if binary && !public(&bare) {
                            format!("<binary {} bytes>", bare.len().div_ceil(2))
                        } else if auth {
                            let (scheme, credential) = bare.split_once(' ').unwrap();
                            format!("{scheme} {}", self.token(credential, "secret"))
                        } else if typed && matches!(class, Some("secret" | "material")) {
                            "[secret]".into()
                        } else {
                            let class = class.unwrap_or("material");
                            if !public(&bare)
                                && ((class == "secret" && bare.len() >= 8)
                                    || (class != "secret" && bare.len() >= 4))
                            {
                                self.remember(&bare, class);
                            }
                            self.token(&bare, class)
                        };
                        out.push_str(&rest[..start]);
                        if let Some(quote) = quote {
                            out.push(quote);
                        }
                        out.push_str(&replacement);
                        if let Some(quote) = quote.filter(|q| raw.ends_with(*q) && raw.len() > 1) {
                            out.push(quote);
                        }
                        at += start + n;
                        continue;
                    }
                }
                out.push_str(key);
                at += n;
            } else if b == b'"' && self.personal_frame {
                let n = value_end(rest, false);
                let (bare, _) = unquote(&rest[..n]);
                let bundle = bare.starts_with("com.") || bare.starts_with("org.");
                out.push('"');
                out.push_str(&if bundle {
                    bare
                } else {
                    self.token(&bare, "text")
                });
                out.push('"');
                at += n;
            } else {
                let c = rest.chars().next().unwrap();
                out.push(c);
                at += c.len_utf8();
            }
        }
        redact_nmea(&out)
    }
    fn plist(&self, text: &str) -> String {
        let mut out = String::new();
        let mut rest = text;
        while let Some(at) = rest.find("<key>") {
            out.push_str(&rest[..at]);
            rest = &rest[at..];
            let Some(end) = rest.find("</key>") else {
                break;
            };
            let key = &rest[5..end];
            let value_start = end + 6;
            let tail = rest[value_start..].trim_start();
            let Some(tag_end) = tail.find('>') else {
                break;
            };
            let tag = &tail[..tag_end + 1];
            if let Some(class) =
                kind(key).filter(|_| matches!(tag, "<string>" | "<data>" | "<integer>" | "<real>"))
            {
                let start = rest.len() - tail.len() + tag.len();
                let end = rest[start..]
                    .find("</")
                    .map_or(rest.len(), |end| start + end);
                out.push_str(&rest[..start]);
                out.push_str(&self.token(&rest[start..end], class));
                rest = &rest[end..];
            } else {
                out.push_str(&rest[..value_start]);
                rest = &rest[value_start..];
            }
        }
        out.push_str(rest);
        out
    }
    fn listed_phone_names(&mut self, text: &str) -> String {
        let prefix = "Multiple connected iPhones found: ";
        let Some((head, tail)) = text.split_once(prefix) else {
            return text.into();
        };
        let mut devices = Vec::new();
        for device in tail.split(", ") {
            let (name, suffix) = device
                .rsplit_once(" (")
                .map_or((device, String::new()), |(name, address)| {
                    (name, format!(" ({address}"))
                });
            let token = self.token(name, "name");
            self.remember(name, "name");
            devices.push(format!("{token}{suffix}"));
        }
        format!("{head}{prefix}{}", devices.join(", "))
    }
}
fn journal_header(line: &str) -> (&str, &str) {
    if line.as_bytes().get(4) == Some(&b'-')
        && line.as_bytes().first().is_some_and(u8::is_ascii_digit)
    {
        let mut fields = line.splitn(4, ' ');
        if let (Some(time), Some(host), Some(process), Some(message)) =
            (fields.next(), fields.next(), fields.next(), fields.next())
        {
            if time.contains('T') && !host.is_empty() && process.ends_with(':') {
                return line.split_at(line.len() - message.len());
            }
        }
    }
    ("", line)
}
fn lower_percent(value: &str) -> String {
    let mut bytes = value.as_bytes().to_vec();
    for i in 0..bytes.len().saturating_sub(2) {
        if bytes[i] == b'%' {
            bytes[i + 1].make_ascii_lowercase();
            bytes[i + 2].make_ascii_lowercase();
        }
    }
    String::from_utf8(bytes).unwrap()
}
fn unquote(raw: &str) -> (String, Option<char>) {
    if raw.starts_with('"') {
        return (
            serde_json::from_str(raw).unwrap_or_else(|_| raw.trim_matches('"').into()),
            Some('"'),
        );
    }
    if raw.starts_with('\'') {
        return (raw.trim_matches('\'').into(), Some('\''));
    }
    (raw.into(), None)
}
fn value_end(value: &str, multiword: bool) -> usize {
    if let Some(n) = marker(value) {
        return n;
    }
    let quote = value.chars().next().filter(|c| matches!(c, '"' | '\''));
    if quote.is_some() {
        let mut escape = false;
        for (at, c) in value.char_indices().skip(1) {
            if escape {
                escape = false;
            } else if c == '\\' {
                escape = true;
            } else if Some(c) == quote {
                return at + 1;
            }
        }
        return value.len();
    }
    if value.starts_with(['[', '{']) {
        let mut depth = 0;
        let mut quote = None;
        let mut escape = false;
        for (at, c) in value.char_indices() {
            if escape {
                escape = false;
                continue;
            }
            if c == '\\' {
                escape = true;
                continue;
            }
            if quote.is_some() {
                if quote == Some(c) {
                    quote = None;
                }
                continue;
            }
            match c {
                '"' | '\'' => quote = Some(c),
                '[' | '{' => depth += 1,
                ']' | '}' => {
                    depth -= 1;
                    if depth == 0 {
                        return at + 1;
                    }
                }
                _ => {}
            }
        }
        return value.len();
    }
    for (at, c) in value.char_indices() {
        if matches!(c, '\r' | '\n' | ',' | ')' | ']' | '}' | '>')
            || (!multiword && (c.is_whitespace() || matches!(c, ';' | '&')))
        {
            return at;
        }
        if multiword && c == ' ' {
            let tail = &value[at + 1..];
            let key_len = tail
                .bytes()
                .take_while(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
                .count();
            if tail[key_len..].starts_with(['=', ':']) {
                return at;
            }
        }
    }
    value.len()
}
fn scan(
    text: &str,
    mut replace: impl FnMut(&str) -> Option<(usize, String)>,
    bounded: bool,
) -> String {
    let mut out = String::new();
    let mut at = 0;
    while at < text.len() {
        let rest = &text[at..];
        if let Some(n) = marker(rest) {
            out.push_str(&rest[..n]);
            at += n;
            continue;
        }
        if !bounded || at == 0 || !word(text.as_bytes()[at - 1]) {
            if let Some((n, value)) = replace(rest).filter(|(n, _)| *n > 0) {
                out.push_str(&value);
                at += n;
                continue;
            }
        }
        let c = rest.chars().next().unwrap();
        out.push(c);
        at += c.len_utf8();
    }
    out
}
fn address(part: &str) -> Option<(usize, &'static str)> {
    let n = part
        .bytes()
        .take_while(|b| b.is_ascii_alphanumeric() || b"._%+-@".contains(b))
        .count();
    let contact = part[..n].trim_end_matches('.');
    if let Some((local, domain)) = contact.split_once('@') {
        if !local.is_empty() && domain.contains('.') {
            return Some((contact.len(), "email"));
        }
    }
    let phone_len = part
        .bytes()
        .take_while(|b| b.is_ascii_digit() || b"+ ()-".contains(b))
        .count();
    let phone = part[..phone_len].trim_end_matches([' ', ')', '-']);
    let digits = phone.bytes().filter(u8::is_ascii_digit).count();
    if ((phone.starts_with('+') && (7..=15).contains(&digits))
        || (phone.len() == 11
            && phone.starts_with('1')
            && matches!(phone.as_bytes()[1], b'3'..=b'9')))
        && part
            .as_bytes()
            .get(phone.len())
            .is_none_or(|b| !word(*b) && !b".:/-".contains(b))
    {
        return Some((phone.len(), "phone"));
    }
    None
}
fn redact_nmea(text: &str) -> String {
    scan(
        text,
        |part| {
            if !part.starts_with('$') {
                return None;
            }
            let n = part
                .find(|c: char| c.is_whitespace() || matches!(c, '"' | '\'' | '<'))
                .unwrap_or(part.len());
            let sentence = &part[..n];
            let mut fields: Vec<_> = sentence.split(',').map(str::to_string).collect();
            let positions = if fields[0].ends_with("GGA") {
                [2, 4]
            } else if fields[0].ends_with("RMC") {
                [3, 5]
            } else {
                return Some((n, "[location]".into()));
            };
            if fields.len() <= positions[1] {
                return Some((n, "[location]".into()));
            }
            for at in positions {
                if !fields[at].is_empty() {
                    fields[at] = "[location]".into();
                }
            }
            Some((n, fields.join(",")))
        },
        false,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    fn redactor() -> Redactor {
        Redactor::new(std::array::from_fn(|i| i as u8))
    }
    #[test]
    fn android_auth_plist_typed_previews_and_listed_names_never_expose_values() {
        let mut r = redactor();
        assert_eq!(
            r.redact("Authorization: Bearer topsecret"),
            format!("Authorization: Bearer {}", r.token("topsecret", "secret"))
        );
        assert_eq!(
            r.redact("  [0] privateKey: bytes=5 [01 02 03 04 05]"),
            "  [0] privateKey: [secret]"
        );
        for line in [
            "<key>password</key><string>super-private</string>",
            "Multiple connected iPhones found: Alice's iPhone (AA:BB:CC:DD:EE:FF)",
            "password=[secret super-private]",
            "Cookie=session=super-private; other=secret, CSeq=3",
        ] {
            let once = r.redact(line);
            assert!(!once.contains("super-private"));
            assert!(!once.contains("Alice's iPhone"));
            assert_eq!(r.redact(&once), once);
        }
        r.remember("a\rb\nc", "name");
        assert_eq!(
            r.redact("a b c"),
            format!(
                "{} {} {}",
                r.token("a", "name"),
                r.token("b", "name"),
                r.token("c", "name")
            )
        );
        let header = "2026-10-27T12:00:00.123456+00:00 host openxplay-relay[42]: ";
        assert_eq!(
            r.redact(&format!("{header}password=long-password")),
            format!("{header}password={}", r.token("long-password", "secret"))
        );
    }
    #[test]
    fn android_key_classes_suffixes_and_public_markers() {
        let mut r = redactor();
        for key in [
            "password",
            "passwd",
            "pwd",
            "manualPassphrase",
            "remoteMfiToken",
            "pre_shared_key",
            "api-key",
            "set-cookie",
            "pinCode",
            "carlinkPin",
        ] {
            assert_eq!(
                r.redact(&format!("{key}=short")),
                format!("{key}={}", r.token("short", "secret"))
            );
        }
        for (key, class) in [
            ("privateKey", "material"),
            ("shk", "material"),
            ("latitude", "location"),
            ("deviceName", "name"),
            ("remoteId", "phone"),
            ("email", "email"),
        ] {
            assert_eq!(
                r.redact(&format!("{key}=q")),
                format!("{key}={}", r.token("q", class))
            );
            for value in [
                "true",
                "null",
                "[]",
                "[secret]",
                "[hidden 24 bytes]",
                "[name#123abc]",
            ] {
                let line = format!("{key}={value}");
                assert_eq!(r.redact(&line), line);
            }
        }
        for line in [
            "passphraseLength=12",
            "pinRequired=true",
            "hasPasswordSet=false",
            "latency=12",
            "mapping=1",
            "keyframe=true",
        ] {
            assert_eq!(r.redact(line), line);
        }
    }
    #[test]
    fn android_tokens_learning_and_encoded_known_values_are_stable() {
        let mut r = redactor();
        assert_eq!(
            r.token("alpha", "secret"),
            redactor().token("alpha", "secret")
        );
        assert_ne!(
            r.token("alpha", "secret"),
            Redactor::new([42; 32]).token("alpha", "secret")
        );
        let secret = "abc\"\\/ def";
        r.remember(secret, "secret");
        let json = serde_json::to_string(secret).unwrap();
        for input in [
            secret.to_string(),
            json[1..json.len() - 1].into(),
            crate::public_name::http::encode(secret),
        ] {
            assert_eq!(r.redact(&input), r.token(secret, "secret"));
        }
        r.redact("password=learned-passphrase");
        assert_eq!(
            r.redact("later learned-passphrase"),
            format!("later {}", r.token("learned-passphrase", "secret"))
        );
        r.remember("1234", "secret");
        assert_eq!(
            r.redact("counter=123456 a1234 1234b"),
            "counter=123456 a1234 1234b"
        );
    }
    #[test]
    fn network_identifiers_match_android_privacy_list() {
        let mut r = redactor();
        let input = "ssid=\"Alice WiFi\" password=secret-pass deviceName=\"Alice iPhone\" mac=AA:BB:CC:DD:EE:FF public=8.8.8.8:7000 v6=2001:4860:4860::8888 private=192.168.1.3:7000 code=4301";
        let clean = r.redact(input);
        for value in ["secret-pass", "Alice iPhone"] {
            assert!(!clean.contains(value), "{clean}");
        }
        for value in [
            "Alice WiFi",
            "AA:BB:CC:DD:EE:FF",
            "8.8.8.8",
            "2001:4860:4860::8888",
            ":7000",
            "192.168.1.3",
            "code=4301",
        ] {
            assert!(clean.contains(value));
        }
        assert_eq!(r.redact(&clean), clean);
        for address in [
            "10.1.2.3",
            "172.16.0.1",
            "172.31.255.255",
            "127.0.0.1",
            "169.254.1.1",
        ] {
            assert_eq!(r.redact(address), address);
        }
    }
    #[test]
    fn android_auth_contacts_names_pem_and_location_cases() {
        let mut r = redactor();
        for line in [
            "Bearer abc123 Basic YWJj",
            "Authorization: Bearer topsecret",
            "https://user:pass@example.org/path",
            "email=a@example.org",
            "call (+14155551234) status=2",
            "AirPlay device info name=Alice's iPhone deviceId=AA:BB:CC:DD:EE:FF model=iPhone16,2",
        ] {
            let once = r.redact(line);
            assert_ne!(once, line);
            assert_eq!(r.redact(&once), once);
        }
        for line in [
            "-----BEGIN RSA PRIVATE KEY-----",
            "very private material",
            "-----END EC PRIVATE KEY-----",
            "still private",
            "-----END RSA PRIVATE KEY-----",
        ] {
            assert_eq!(r.redact(line), "[secret]");
        }
        assert_eq!(r.redact("normal"), "normal");
        assert_eq!(
            r.redact("$GPRMC,123519,A,4807.038,N,01131.000,E,022.4,084.4,230394"),
            "$GPRMC,123519,A,[location],N,[location],E,022.4,084.4,230394"
        );
        assert_eq!(
            r.redact("bodyHex=0102abff next=ok"),
            "bodyHex=<binary 4 bytes> next=ok"
        );
    }
}
