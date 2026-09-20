use serde::{Deserialize, Serialize};
use weavatrix_graph::SourceSpan;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ByteSpan {
    pub start: u64,
    pub end: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvidenceRef {
    pub artifact_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    pub content_digest: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub byte_span: Option<ByteSpan>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_span: Option<SourceSpan>,
    pub extractor_id: String,
    pub evidence_class: String,
}

impl EvidenceRef {
    #[must_use]
    pub fn observed_file(path: &str, content_digest: &str, bytes: &[u8]) -> Self {
        let start = raw_byte_span(bytes, &[0xef, 0xbb, 0xbf]).map_or(0, |bom| bom.end);
        Self {
            artifact_id: format!("config:{path}"),
            path: Some(path.to_owned()),
            content_digest: content_digest.to_owned(),
            byte_span: Some(ByteSpan {
                start,
                end: u64::try_from(bytes.len()).unwrap_or(0),
            }),
            source_span: None,
            extractor_id: super::identity::CONFIG_INVENTORY_DETECTOR.to_owned(),
            evidence_class: "observed".to_owned(),
        }
    }
}

/// Byte offsets into the raw file, including a leading UTF-8 BOM when present.
#[must_use]
pub(crate) fn raw_byte_span(bytes: &[u8], needle: &[u8]) -> Option<ByteSpan> {
    let start = bytes
        .windows(needle.len())
        .position(|window| window == needle)?;
    Some(ByteSpan {
        start: u64::try_from(start).unwrap_or(u64::MAX),
        end: u64::try_from(start.saturating_add(needle.len())).unwrap_or(u64::MAX),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_span_counts_a_leading_bom() {
        let bytes = [0xef, 0xbb, 0xbf, b'n', b'a', b'm', b'e'];
        let span = raw_byte_span(&bytes, b"name").unwrap();
        assert_eq!(span.start, 3);
        assert_eq!(span.end, 7);
    }
}
