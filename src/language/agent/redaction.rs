#[must_use]
pub(super) fn secret_key(name: &str) -> bool {
    let haystack = name.to_ascii_lowercase();
    [
        "password",
        "token",
        "secret",
        "apikey",
        "api_key",
        "authorization",
        "cookie",
        "credential",
        "privatekey",
        "private_key",
        "accesskey",
        "clientsecret",
    ]
    .iter()
    .any(|marker| haystack.contains(marker))
}

#[must_use]
pub(super) fn secret_value(text: &str) -> bool {
    text.contains("://") && text.contains('@')
}

#[must_use]
pub(super) fn redact_label(key: &str, value: &str) -> String {
    if secret_key(key) || secret_value(value) {
        format!("{key}:[redacted]")
    } else {
        format!("{key}:{value}")
    }
}

#[cfg(test)]
mod tests {
    use super::redact_label;

    #[test]
    fn authorization_never_reaches_a_label() {
        assert_eq!(
            redact_label("Authorization", "Bearer abc"),
            "Authorization:[redacted]"
        );
        assert_eq!(
            redact_label("url", "https://user:pass@host/path"),
            "url:[redacted]"
        );
        assert_eq!(redact_label("X-Tenant", "public"), "X-Tenant:public");
    }
}
