use weavatrix_graph::{SourcePosition, SourceSpan};

#[derive(Debug, Clone)]
pub(crate) enum Node {
    Scalar(Scalar),
    Sequence {
        items: Vec<Node>,
        start: usize,
        end: usize,
    },
    Mapping {
        entries: Vec<(Scalar, Node)>,
        start: usize,
        end: usize,
    },
}

#[derive(Debug, Clone)]
pub(crate) struct Scalar {
    pub decoded: String,
    pub raw_start: usize,
    pub raw_end: usize,
}

impl Node {
    pub(crate) fn get(&self, key: &str) -> Option<&Node> {
        self.entries()?
            .iter()
            .find(|(name, _)| name.decoded == key)
            .map(|(_, value)| value)
    }

    pub(crate) fn entries(&self) -> Option<&[(Scalar, Node)]> {
        match self {
            Self::Mapping { entries, .. } => Some(entries),
            _ => None,
        }
    }

    pub(crate) fn items(&self) -> Option<&[Node]> {
        match self {
            Self::Sequence { items, .. } => Some(items),
            _ => None,
        }
    }

    pub(crate) fn as_str(&self) -> Option<&str> {
        match self {
            Self::Scalar(scalar) => Some(scalar.decoded.as_str()),
            _ => None,
        }
    }

    pub(crate) fn as_bool(&self) -> Option<bool> {
        match self.as_str()? {
            "true" | "True" | "yes" => Some(true),
            "false" | "False" | "no" => Some(false),
            _ => None,
        }
    }

    pub(crate) fn range(&self) -> (usize, usize) {
        match self {
            Self::Scalar(scalar) => (scalar.raw_start, scalar.raw_end),
            Self::Sequence { start, end, .. } | Self::Mapping { start, end, .. } => (*start, *end),
        }
    }

    pub(crate) fn strings(&self) -> Vec<&Scalar> {
        let mut out = Vec::new();
        collect_strings(self, &mut out);
        out
    }
}

fn collect_strings<'a>(node: &'a Node, out: &mut Vec<&'a Scalar>) {
    match node {
        Node::Scalar(scalar) => out.push(scalar),
        Node::Sequence { items, .. } => {
            for item in items {
                collect_strings(item, out);
            }
        }
        Node::Mapping { entries, .. } => {
            for (key, value) in entries {
                out.push(key);
                collect_strings(value, out);
            }
        }
    }
}

pub(crate) fn span_for(path: &str, raw: &str, start: usize, end: usize) -> SourceSpan {
    let start = start.min(raw.len());
    let end = end.max(start + usize::from(start == end)).min(raw.len());
    SourceSpan {
        file: path.to_owned(),
        start: position(raw, start),
        end: position(raw, end),
    }
}

fn position(raw: &str, offset: usize) -> SourcePosition {
    let prefix = &raw[..offset.min(raw.len())];
    let line =
        u32::try_from(prefix.bytes().filter(|byte| *byte == b'\n').count() + 1).unwrap_or(u32::MAX);
    let column = u32::try_from(
        prefix
            .rsplit_once('\n')
            .map_or(prefix, |(_, last)| last)
            .chars()
            .count()
            + 1,
    )
    .unwrap_or(u32::MAX);
    SourcePosition { line, column }
}
