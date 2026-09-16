#[must_use]
pub(crate) fn secret_label(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.contains("password")
        || lower.contains("token")
        || lower.contains("secret")
        || lower.contains("api_key")
        || lower.contains("apikey")
        || lower.contains("authorization")
        || lower.contains("credential")
        || (lower.contains("://") && lower.contains('@'))
}
