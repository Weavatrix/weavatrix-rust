use proc_macro2::Span;
use syn::{Attribute, Expr, Lit, Member};

pub(super) fn route_call(node: &syn::ExprMethodCall) -> Option<(&'static str, String)> {
    let mut arguments = node.args.iter();
    let path = string_literal(arguments.next()?)?;
    if !path.starts_with('/') {
        return None;
    }
    let handler = arguments.next()?;
    let method = match callable_name(match handler {
        Expr::Call(call) => &call.func,
        expression => expression,
    })?
    .as_str()
    {
        "get" => "GET",
        "post" => "POST",
        "put" => "PUT",
        "patch" => "PATCH",
        "delete" => "DELETE",
        "head" => "HEAD",
        "options" => "OPTIONS",
        _ => "ANY",
    };
    Some((method, path))
}

pub(super) fn attribute_routes(attributes: &[Attribute]) -> Vec<(&'static str, String, Span)> {
    attributes
        .iter()
        .filter_map(|attribute| {
            let method = attribute_method(attribute)?;
            let path = attribute.parse_args::<syn::LitStr>().ok()?.value();
            path.starts_with('/')
                .then(|| (method, path, syn::spanned::Spanned::span(attribute)))
        })
        .collect()
}

fn string_literal(expression: &Expr) -> Option<String> {
    match unwrapped(expression) {
        Expr::Lit(literal) => match &literal.lit {
            Lit::Str(value) => Some(value.value()),
            _ => None,
        },
        _ => None,
    }
}

pub(super) fn callable_name(expression: &Expr) -> Option<String> {
    match unwrapped(expression) {
        Expr::Path(path) => path
            .path
            .segments
            .last()
            .map(|segment| segment.ident.to_string()),
        Expr::Field(field) => match &field.member {
            Member::Named(name) => Some(name.to_string()),
            Member::Unnamed(_) => None,
        },
        _ => None,
    }
}

fn unwrapped(mut expression: &Expr) -> &Expr {
    loop {
        expression = match expression {
            Expr::Group(group) => &group.expr,
            Expr::Paren(parenthesized) => &parenthesized.expr,
            _ => return expression,
        };
    }
}

fn attribute_method(attribute: &Attribute) -> Option<&'static str> {
    match attribute.path().segments.last()?.ident.to_string().as_str() {
        "get" => Some("GET"),
        "post" => Some("POST"),
        "put" => Some("PUT"),
        "patch" => Some("PATCH"),
        "delete" => Some("DELETE"),
        "head" => Some("HEAD"),
        "options" => Some("OPTIONS"),
        "route" => Some("ANY"),
        _ => None,
    }
}
