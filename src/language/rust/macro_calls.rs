use syn::parse::Parser;
use syn::punctuated::Punctuated;
use syn::{Expr, ExprMacro, Token};

pub(super) fn for_each_standard_argument(node: &ExprMacro, mut visit: impl FnMut(&Expr)) {
    let Some(name) = node.mac.path.segments.last().map(|segment| &segment.ident) else {
        return;
    };
    if !matches!(
        name.to_string().as_str(),
        "format"
            | "format_args"
            | "format_args_nl"
            | "print"
            | "println"
            | "eprint"
            | "eprintln"
            | "write"
            | "writeln"
            | "dbg"
    ) {
        return;
    }
    let Ok(arguments) =
        Punctuated::<Expr, Token![,]>::parse_terminated.parse2(node.mac.tokens.clone())
    else {
        return;
    };
    for argument in &arguments {
        visit(argument);
    }
}

#[cfg(test)]
mod tests {
    use super::super::*;

    #[test]
    fn extracts_calls_inside_standard_formatting_macros() {
        let facts = RustAdapter
            .parse(SourceFile {
                path: "src/lib.rs",
                text: r#"
fn resolve_target(selector: &str) -> String { selector.to_owned() }
fn resolve_target_path(selector: &str) -> String {
    format!("/{}", resolve_target(selector))
}
"#,
            })
            .unwrap();

        let call = facts
            .references
            .iter()
            .find(|item| item.name == "resolve_target")
            .expect("the call inside format! must remain graph evidence");
        assert_eq!(call.kind, EdgeKind::Calls);
        assert_eq!(
            call.owner.as_ref().map(|owner| owner.name.as_str()),
            Some("resolve_target_path")
        );
    }

    #[test]
    fn does_not_guess_that_arbitrary_macro_tokens_are_calls() {
        let facts = RustAdapter
            .parse(SourceFile {
                path: "src/lib.rs",
                text: r"
fn resolve_target(selector: &str) -> String { selector.to_owned() }
fn declaration_dsl() {
    unknown_dsl!(resolve_target(selector));
}
",
            })
            .unwrap();

        assert!(
            facts
                .references
                .iter()
                .all(|item| item.name != "resolve_target")
        );
    }
}
