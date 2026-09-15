//! Default reports must not carry secret parameter values.

#[must_use]
pub(super) fn skip_pointer(pointer: &str) -> bool {
    let haystack = pointer.to_ascii_lowercase();
    haystack.contains("pindata")
        || haystack.contains("staticdata")
        || haystack.contains("authentication")
        || haystack.contains("headerparameters")
        || secret_key(&haystack)
}

#[must_use]
pub(super) fn looks_secret(pointer: &str, text: &str) -> bool {
    let haystack = pointer.to_ascii_lowercase();
    skip_pointer(pointer) || secret_key(&haystack) || url_with_userinfo(text)
}

#[must_use]
fn secret_key(pointer: &str) -> bool {
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
    .any(|marker| pointer.contains(marker))
}

#[must_use]
fn url_with_userinfo(text: &str) -> bool {
    text.contains("://") && text.contains('@')
}

#[cfg(test)]
mod tests {
    use super::looks_secret;

    #[test]
    fn authorization_header_is_secret() {
        assert!(looks_secret(
            "/parameters/headerParameters/0/value",
            "Bearer abc"
        ));
        assert!(looks_secret("/url", "https://user:pass@host/path"));
        assert!(!looks_secret(
            "/parameters/url",
            "https://api.example/items"
        ));
    }
}
