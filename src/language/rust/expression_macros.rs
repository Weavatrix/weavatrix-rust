use super::Collector;
use syn::parse::Parser;
use syn::punctuated::Punctuated;
use syn::visit::Visit;
use syn::{Expr, Token};

pub(super) fn visit_arguments(collector: &mut Collector<'_>, node: &syn::ExprMacro) {
    if !is_expression_list(&node.mac) {
        return;
    }
    let Ok(arguments) =
        Punctuated::<Expr, Token![,]>::parse_terminated.parse2(node.mac.tokens.clone())
    else {
        return;
    };
    for argument in &arguments {
        collector.visit_expr(argument);
    }
}

fn is_expression_list(value: &syn::Macro) -> bool {
    value
        .path
        .segments
        .last()
        .map(|segment| segment.ident.to_string())
        .is_some_and(|name| {
            matches!(
                name.as_str(),
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
            )
        })
}

// Arbitrary macro tokens can be declarations, patterns, or prose. Only known
// expression-list macros are traversed so guesses never become graph evidence.
