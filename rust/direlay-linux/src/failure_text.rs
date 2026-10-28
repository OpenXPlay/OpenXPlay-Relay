// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-27

//! Failure messages retain upstream codes and causes, with credentials removed before storage.

pub fn error_text(error: &dyn std::error::Error) -> String {
    let mut lines = vec![error.to_string()];
    let mut source = error.source();
    while let Some(error) = source.filter(|_| lines.len() < 5) {
        lines.push(format!("Caused by: {error}"));
        source = error.source();
    }
    lines.join("\n")
}

pub fn redact(text: &str, secrets: &[String]) -> String {
    let mut text = text.to_owned();
    let mut secrets: Vec<_> = secrets.iter().filter(|s| !s.is_empty()).collect();
    secrets.sort_by_key(|s| std::cmp::Reverse(s.len()));
    for secret in secrets {
        text = text.replace(secret.as_str(), "[redacted]");
    }
    let mut out = String::new();
    let mut at = 0;
    while at < text.len() {
        let part = &text[at..];
        let boundary = at == 0 || !text.as_bytes()[at - 1].is_ascii_alphanumeric();
        if let Some(end) = boundary.then(|| url_credentials(part)).flatten() {
            let scheme = part.find("://").unwrap() + 3;
            out.push_str(&part[..scheme]);
            out.push_str("[redacted]@");
            at += end;
            continue;
        }
        if boundary {
            if let Some(start) = credential_start(part) {
                out.push_str(&part[..start]);
                out.push_str("[redacted]");
                at += value_end(part, start);
                continue;
            }
        }
        let c = part.chars().next().unwrap();
        // Preserve readable multiline errors, without terminal escape/control sequences.
        if !c.is_control() || c == '\n' || c == '\t' {
            out.push(c);
        }
        at += c.len_utf8();
    }
    out
}

fn url_credentials(part: &str) -> Option<usize> {
    if !part.as_bytes().first()?.is_ascii_alphabetic() {
        return None;
    }
    let scheme = part
        .bytes()
        .take(64)
        .position(|b| !b.is_ascii_alphanumeric() && !b"+.-".contains(&b))?;
    if !part.get(scheme..)?.starts_with("://") {
        return None;
    }
    let rest = &part[scheme + 3..];
    let end = rest.find('@')?;
    (!rest[..end].contains(|c: char| c.is_whitespace() || c == '/')).then_some(scheme + 3 + end + 1)
}
fn credential_start(part: &str) -> Option<usize> {
    if part
        .get(..7)
        .is_some_and(|s| s.eq_ignore_ascii_case("bearer "))
    {
        return Some(7);
    }
    // Android's diagnostic credential labels, accepting spaces, dashes and underscores.
    let key_end = part
        .bytes()
        .take(64)
        .take_while(|b| b.is_ascii_alphanumeric() || b"_- ".contains(b))
        .count();
    let lower = part[..key_end].to_ascii_lowercase();
    for end in (1..=key_end).rev() {
        let key = lower[..end].replace(['_', '-', ' '], "");
        if !matches!(
            key.as_str(),
            "remotemfitoken"
                | "remotetoken"
                | "accesstoken"
                | "token"
                | "password"
                | "passphrase"
                | "passcode"
                | "pin"
                | "psk"
        ) {
            continue;
        }
        let mut start = end;
        while part
            .as_bytes()
            .get(start)
            .is_some_and(|b| b.is_ascii_whitespace() || matches!(b, b'"' | b'\''))
        {
            start += 1;
        }
        if part
            .as_bytes()
            .get(start)
            .is_some_and(|b| matches!(b, b':' | b'='))
        {
            start += 1;
        } else if part
            .get(start..start + 3)
            .is_some_and(|s| s.eq_ignore_ascii_case("is "))
        {
            start += 3;
        } else {
            continue;
        }
        while part
            .as_bytes()
            .get(start)
            .is_some_and(u8::is_ascii_whitespace)
        {
            start += 1;
        }
        return Some(start);
    }
    None
}
fn value_end(part: &str, start: usize) -> usize {
    let value = &part[start..];
    if let Some(quote @ ('"' | '\'')) = value.chars().next() {
        return start + value[1..].find(quote).map_or(value.len(), |n| n + 2);
    }
    start
        + value
            .find(|c: char| c.is_whitespace() || ",;}&".contains(c))
            .unwrap_or(value.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn keeps_multiline_codes_and_redacts_android_credential_forms() {
        let text = "RFCOMM error=17\nCaused by: password='two words'; remote_mfi_token is abc, PIN: 1234 psk=wifi\nhttps://user:secret@host/path Bearer abc.def known";
        let safe = redact(text, &["known".into()]);
        assert!(safe.contains("RFCOMM error=17\nCaused by:"));
        assert!(safe.contains("https://[redacted]@host/path"));
        for value in [
            "two words",
            "abc",
            "1234",
            "psk=wifi",
            "user:secret",
            "known",
        ] {
            assert!(!safe.contains(value), "{safe}");
        }
        assert_eq!(
            redact("错误 password=秘密", &[]),
            "错误 password=[redacted]"
        );
    }
}
