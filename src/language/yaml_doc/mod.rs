//! Source-aware YAML subset used by Dify exports. Not a second `.yml` adapter.

mod block;
mod parse;
mod scan;
mod value;

pub(crate) use parse::parse;
pub(crate) use value::{Node, Scalar, span_for};

#[cfg(test)]
mod tests {
    use super::parse;

    #[test]
    fn literal_and_folded_scalars_keep_yaml_values() {
        let docs = parse(
            "scalars.yml",
            "literal: |\n  alpha\n  beta\nfolded: >\n  alpha\n  beta\nurl: http://x#frag\n",
        )
        .unwrap();
        assert_eq!(
            docs[0].get("literal").and_then(|node| node.as_str()),
            Some("alpha\nbeta\n")
        );
        assert_eq!(
            docs[0].get("folded").and_then(|node| node.as_str()),
            Some("alpha beta\n")
        );
        assert_eq!(
            docs[0].get("url").and_then(|node| node.as_str()),
            Some("http://x#frag")
        );
        assert!(
            docs[0]
                .get("url")
                .is_some_and(|node| !node.strings().is_empty())
        );
    }

    #[test]
    fn block_scalar_headers_cover_chomp_and_indent() {
        let docs = parse(
            "chomp.yml",
            "keep: |+\n  a\n\nstrip: |-\n  a\n\nhinted: |2\n    a\n",
        )
        .unwrap();
        assert!(docs[0].get("keep").and_then(|node| node.as_str()).is_some());
        assert_eq!(
            docs[0].get("strip").and_then(|node| node.as_str()),
            Some("a")
        );
        assert!(parse("bad.yml", "x: |?\n  a\n").is_err());
    }

    #[test]
    fn invalid_unicode_escape_is_a_diagnostic() {
        let error = parse("bad.yml", "name: \"\\uD800\"\n").unwrap_err();
        assert_eq!(error.code, "yaml.limit");
        assert!(
            error.message.contains("invalid \\u") || error.message.contains("unsupported"),
            "{}",
            error.message
        );
    }

    #[test]
    fn sequence_item_mapping_keeps_sibling_keys() {
        let raw = "items:\n- id: start\n  type: custom\n";
        let docs = parse("demo.yml", raw).unwrap();
        let item = docs[0]
            .get("items")
            .and_then(|items| items.items())
            .and_then(|items| items.first())
            .unwrap();
        assert_eq!(item.get("id").and_then(|node| node.as_str()), Some("start"));
        assert_eq!(
            item.get("type").and_then(|node| node.as_str()),
            Some("custom")
        );
    }
}
