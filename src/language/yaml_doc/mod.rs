//! Source-aware YAML subset used by Dify exports. Not a second `.yml` adapter.

mod parse;
mod scan;
mod value;

pub(crate) use parse::parse;
pub(crate) use value::{Node, Scalar, span_for};

#[cfg(test)]
mod tests {
    use super::parse;

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
