use weavatrix_parse::Token;

pub(crate) fn matching_delimiter(
    tokens: &[Token],
    source: &str,
    open: usize,
    delimiters: (&str, &str),
) -> Option<usize> {
    let mut depth = 0_usize;
    for (index, token) in tokens.iter().enumerate().skip(open) {
        match token.text(source) {
            value if value == delimiters.0 => depth += 1,
            value if value == delimiters.1 => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return Some(index);
                }
            }
            _ => {}
        }
    }
    None
}
