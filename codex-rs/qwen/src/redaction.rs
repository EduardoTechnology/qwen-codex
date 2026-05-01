use regex_lite::Regex;

const REDACTED: &str = "[REDACTED]";

pub fn redact_text(input: &str) -> String {
    let mut redacted = input.to_string();

    for (pattern, replacement) in [
        (
            r"(?is)-----BEGIN [A-Z ]*PRIVATE KEY-----.*?-----END [A-Z ]*PRIVATE KEY-----",
            REDACTED.to_string(),
        ),
        (
            r#"(?i)(authorization\s*:\s*(?:bearer|basic|token)\s+)[^\s\r\n]+"#,
            format!("$1{REDACTED}"),
        ),
        (
            r#"(?i)("(?:api[_-]?key|token|secret|password|credential|authorization)"\s*:\s*")[^"]+(")"#,
            format!("$1{REDACTED}$2"),
        ),
        (
            r#"(?i)((?:api[_-]?key|token|secret|password|credential|authorization)\s*=\s*)[^\s\r\n]+"#,
            format!("$1{REDACTED}"),
        ),
        (
            r#"(?i)\b(?:sk|hf)_[A-Za-z0-9][A-Za-z0-9_\-]{12,}\b"#,
            REDACTED.to_string(),
        ),
    ] {
        let Ok(regex) = Regex::new(pattern) else {
            continue;
        };
        redacted = regex
            .replace_all(&redacted, replacement.as_str())
            .to_string();
    }

    redacted
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn redacts_authorization_headers() {
        assert_eq!(
            redact_text("Authorization: Bearer sk_test_123456789abcdef"),
            "Authorization: Bearer [REDACTED]"
        );
    }

    #[test]
    fn redacts_env_assignments() {
        assert_eq!(
            redact_text("QWEN_CODEX_API_KEY=local-secret\nOTHER=value"),
            "QWEN_CODEX_API_KEY=[REDACTED]\nOTHER=value"
        );
    }

    #[test]
    fn redacts_json_secret_fields() {
        assert_eq!(
            redact_text(r#"{"api_key":"abc123","model":"qwen35-local"}"#),
            r#"{"api_key":"[REDACTED]","model":"qwen35-local"}"#
        );
    }

    #[test]
    fn redacts_private_key_blocks() {
        assert_eq!(
            redact_text(
                "before\n-----BEGIN PRIVATE KEY-----\nsecret\n-----END PRIVATE KEY-----\nafter"
            ),
            "before\n[REDACTED]\nafter"
        );
    }
}
