//! Pure text parsing of stack-trace frames. No process is executed and no
//! file is read: a frame is only what the trace text itself states.

pub(super) struct ParsedFrame {
    pub(super) raw: String,
    pub(super) symbol: Option<String>,
    pub(super) path: Option<String>,
    pub(super) line: Option<u32>,
    pub(super) column: Option<u32>,
}

pub(super) fn parse(text: &str, max_frames: usize) -> Vec<ParsedFrame> {
    let mut frames = Vec::new();
    // A numbered Rust backtrace frame names the symbol one line before the
    // `at path:line:col` line that locates it.
    let mut pending_symbol: Option<String> = None;
    for raw_line in text.lines() {
        if frames.len() >= max_frames {
            break;
        }
        let line = raw_line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some(frame) = python_frame(line) {
            pending_symbol = None;
            frames.push(frame);
        } else if let Some(frame) = panic_frame(line) {
            pending_symbol = None;
            frames.push(frame);
        } else if let Some(symbol) = rust_numbered_symbol(line) {
            pending_symbol = Some(symbol);
        } else if let Some(rest) = line.strip_prefix("at ") {
            let mut frame = at_frame(raw_line, rest);
            if frame.symbol.is_none() {
                frame.symbol = pending_symbol.take();
            }
            pending_symbol = None;
            frames.push(frame);
        }
    }
    frames
}

/// `File "path", line N, in name` - the `CPython` traceback frame.
fn python_frame(line: &str) -> Option<ParsedFrame> {
    let rest = line.strip_prefix("File \"")?;
    let (path, rest) = rest.split_once('"')?;
    let line_number = rest
        .split_once("line ")
        .and_then(|(_, tail)| numeric_prefix(tail));
    let symbol = rest
        .split_once(", in ")
        .map(|(_, name)| name.trim().to_owned())
        .filter(|name| !name.is_empty());
    Some(ParsedFrame {
        raw: line.to_owned(),
        symbol,
        path: Some(path.to_owned()),
        line: line_number,
        column: None,
    })
}

/// `thread 'main' panicked at src/main.rs:10:5:` and the pre-2021 variant
/// `panicked at 'message', src/main.rs:10:5`.
fn panic_frame(line: &str) -> Option<ParsedFrame> {
    let rest = line.split_once("panicked at ")?.1.trim();
    let candidate = rest
        .rsplit(|character: char| character.is_whitespace() || character == ',')
        .find(|token| !token.is_empty() && token.matches(':').count() >= 1)?;
    let (path, line_number, column) = location(candidate)?;
    line_number?;
    Some(ParsedFrame {
        raw: line.to_owned(),
        symbol: None,
        path: Some(path),
        line: line_number,
        column,
    })
}

/// `12: crate::module::function` inside `RUST_BACKTRACE=1` output.
fn rust_numbered_symbol(line: &str) -> Option<String> {
    let (number, rest) = line.split_once(':')?;
    if number.is_empty() || !number.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let symbol = rest.trim();
    (!symbol.is_empty() && !symbol.contains(' ')).then(|| symbol.to_owned())
}

/// V8 (`at name (path:line:col)`, `at path:line:col`) and JVM
/// (`at pkg.Class.method(Class.java:123)`) frames.
fn at_frame(raw: &str, rest: &str) -> ParsedFrame {
    let rest = rest.trim();
    let (symbol, location_text) = if let Some(open) = rest.rfind('(') {
        let inside = rest[open + 1..].trim_end_matches(')');
        let symbol = rest[..open].trim();
        (
            (!symbol.is_empty()).then(|| strip_symbol_qualifiers(symbol)),
            inside,
        )
    } else {
        (None, rest)
    };
    let (path, line, column) = location(location_text).unwrap_or((String::new(), None, None));
    ParsedFrame {
        raw: raw.trim().to_owned(),
        symbol,
        path: (!path.is_empty()).then_some(path),
        line,
        column,
    }
}

fn strip_symbol_qualifiers(symbol: &str) -> String {
    symbol
        .trim_start_matches("async ")
        .trim_start_matches("new ")
        .trim()
        .to_owned()
}

/// Splits `path:line:column` accepting Windows drive letters, `file://`
/// schemes and the line-only JVM form `File.java:123`.
fn location(text: &str) -> Option<(String, Option<u32>, Option<u32>)> {
    let text = text.trim().trim_end_matches(':');
    if text.is_empty() || text == "native" || text.starts_with('<') {
        return None;
    }
    let text = text
        .strip_prefix("file:///")
        .or_else(|| text.strip_prefix("file://"))
        .unwrap_or(text);
    let mut path = text;
    let mut line = None;
    let mut column = None;
    if let Some((rest, last)) = path.rsplit_once(':')
        && let Some(number) = parse_u32(last)
        && !rest.is_empty()
    {
        path = rest;
        line = Some(number);
        if let Some((rest, middle)) = path.rsplit_once(':')
            && let Some(inner) = parse_u32(middle)
            && !rest.is_empty()
        {
            path = rest;
            column = line;
            line = Some(inner);
        }
    }
    let path = path.split('?').next().unwrap_or(path).replace('\\', "/");
    (!path.is_empty()).then_some((path, line, column))
}

fn parse_u32(text: &str) -> Option<u32> {
    (!text.is_empty() && text.bytes().all(|byte| byte.is_ascii_digit()))
        .then(|| text.parse().ok())
        .flatten()
}

fn numeric_prefix(text: &str) -> Option<u32> {
    let digits = text.bytes().take_while(u8::is_ascii_digit).count();
    (digits > 0).then(|| text[..digits].parse().ok()).flatten()
}
