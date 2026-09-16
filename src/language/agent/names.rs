#[must_use]
pub(super) fn plugin_name_ok(name: &str) -> bool {
    let len = name.chars().count();
    if !(1..=64).contains(&len) {
        return false;
    }
    let bytes = name.as_bytes();
    let first = *bytes.first().unwrap_or(&0);
    let last = *bytes.last().unwrap_or(&0);
    if !first.is_ascii_alphanumeric() || !last.is_ascii_alphanumeric() {
        return false;
    }
    if name.contains("--") || name.contains("..") {
        return false;
    }
    name.chars().all(|character| {
        character.is_ascii_lowercase()
            || character.is_ascii_digit()
            || matches!(character, '-' | '.')
    })
}

#[must_use]
pub(super) fn skill_name_ok(name: &str) -> bool {
    let len = name.chars().count();
    if !(1..=64).contains(&len) {
        return false;
    }
    if name.starts_with('-') || name.ends_with('-') || name.contains("--") {
        return false;
    }
    name.chars().all(|character| {
        character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-'
    })
}

#[cfg(test)]
mod tests {
    use super::{plugin_name_ok, skill_name_ok};

    #[test]
    fn plugin_and_skill_names_follow_the_specs() {
        assert!(plugin_name_ok("my-plugin"));
        assert!(plugin_name_ok("acme.tools"));
        assert!(!plugin_name_ok("My-Plugin"));
        assert!(!plugin_name_ok("has--double"));
        assert!(skill_name_ok("pdf-processing"));
        assert!(!skill_name_ok("PDF-Processing"));
        assert!(!skill_name_ok("-pdf"));
    }
}
