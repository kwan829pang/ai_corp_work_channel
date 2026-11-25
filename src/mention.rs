use std::collections::BTreeSet;

/// Extract unique `#nickname` tokens from content (letters, digits, `_`).
pub fn extract_mentions(content: &str) -> Vec<String> {
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    let bytes = content.as_bytes();
    let mut i = 0;

    while i < bytes.len() {
        if bytes[i] != b'#' {
            i += 1;
            continue;
        }
        // Require start or non-word char before '#'.
        if i > 0 {
            let prev = bytes[i - 1];
            if is_nick_byte(prev) {
                i += 1;
                continue;
            }
        }

        let start = i + 1;
        let mut end = start;
        while end < bytes.len() && is_nick_byte(bytes[end]) {
            end += 1;
        }
        if end > start {
            if let Ok(nick) = std::str::from_utf8(&bytes[start..end]) {
                let key = nick.to_ascii_lowercase();
                if seen.insert(key) {
                    out.push(nick.to_string());
                }
            }
            i = end;
        } else {
            i += 1;
        }
    }

    out
}

fn is_nick_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_unique_mentions() {
        let got = extract_mentions("How you message #user_b and #Alice #user_b");
        assert_eq!(got, vec!["user_b", "Alice"]);
    }

    #[test]
    fn ignores_mid_token_hash() {
        assert!(extract_mentions("price#tag and x#y").is_empty());
    }

    #[test]
    fn allows_start_of_string() {
        assert_eq!(extract_mentions("#bob hi"), vec!["bob"]);
    }
}
