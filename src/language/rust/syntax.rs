use proc_macro2::{LineColumn, Span};
use syn::UseTree;
use weavatrix_graph::{SourcePosition, SourceSpan};

pub(super) fn attributes_mark_test(attributes: &[syn::Attribute]) -> bool {
    attributes.iter().any(|attribute| {
        let attribute_name = attribute
            .path()
            .segments
            .last()
            .map(|part| part.ident.to_string());
        if attribute_name.as_deref().is_some_and(|name| {
            matches!(
                name,
                "test" | "rstest" | "proptest" | "wasm_bindgen_test" | "test_case"
            )
        }) {
            return true;
        }
        if !attribute.path().is_ident("cfg") {
            return false;
        }
        let syn::Meta::List(meta_list) = &attribute.meta else {
            return false;
        };
        cfg_list_marks_test(meta_list)
    })
}

fn cfg_list_marks_test(list: &syn::MetaList) -> bool {
    cfg_list_marks_test_with_negation(list, false)
}

fn cfg_list_marks_test_with_negation(list: &syn::MetaList, negated: bool) -> bool {
    let nested_negated = if list.path.is_ident("not") {
        !negated
    } else {
        negated
    };
    list.parse_args_with(syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated)
        .is_ok_and(|items| {
            items
                .iter()
                .any(|meta| cfg_meta_marks_test(meta, nested_negated))
        })
}

fn cfg_meta_marks_test(meta: &syn::Meta, negated: bool) -> bool {
    match meta {
        syn::Meta::Path(path) => path.is_ident("test") && !negated,
        syn::Meta::List(list) => cfg_list_marks_test_with_negation(list, negated),
        syn::Meta::NameValue(_) => false,
    }
}

pub(super) fn impl_owner(ty: &syn::Type) -> Option<String> {
    let syn::Type::Path(path) = ty else {
        return None;
    };
    path.path
        .segments
        .last()
        .map(|segment| segment.ident.to_string())
}

pub(super) fn use_tree_targets(tree: &UseTree) -> Vec<String> {
    use_tree_targets_with_prefix(tree, "")
}

fn use_tree_targets_with_prefix(tree: &UseTree, prefix: &str) -> Vec<String> {
    match tree {
        UseTree::Path(path) => {
            let prefix = join_use_path(prefix, &path.ident.to_string());
            use_tree_targets_with_prefix(&path.tree, &prefix)
        }
        UseTree::Name(name) => vec![join_use_path(prefix, &name.ident.to_string())],
        UseTree::Rename(rename) => vec![format!(
            "{} as {}",
            join_use_path(prefix, &rename.ident.to_string()),
            rename.rename
        )],
        UseTree::Glob(_) => vec![join_use_path(prefix, "*")],
        UseTree::Group(group) => group
            .items
            .iter()
            .flat_map(|item| use_tree_targets_with_prefix(item, prefix))
            .collect(),
    }
}

fn join_use_path(prefix: &str, item: &str) -> String {
    if prefix.is_empty() {
        item.to_owned()
    } else {
        format!("{prefix}::{item}")
    }
}

pub(super) fn source_span(path: &str, span: Span) -> SourceSpan {
    SourceSpan {
        file: path.to_owned(),
        start: position(span.start()),
        end: position(span.end()),
    }
}

fn position(point: LineColumn) -> SourcePosition {
    SourcePosition {
        line: u32::try_from(point.line).unwrap_or(u32::MAX),
        column: u32::try_from(point.column)
            .unwrap_or(u32::MAX)
            .saturating_add(1),
    }
}
