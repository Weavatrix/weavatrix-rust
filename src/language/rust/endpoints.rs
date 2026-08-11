use proc_macro2::Span;
use syn::punctuated::Punctuated;
use syn::{Attribute, Expr, Lit, Member, Token};

/// The verb reported for a route whose method the source never names.
const ANY: &str = "ANY";

pub(super) fn route_call(node: &syn::ExprMethodCall) -> Vec<(&'static str, String)> {
    let mut arguments = node.args.iter();
    let Some(path) = arguments.next().and_then(string_literal) else {
        return Vec::new();
    };
    let Some(handler) = arguments.next() else {
        return Vec::new();
    };
    if !path.starts_with('/') {
        return Vec::new();
    }
    served_methods(handler_methods(handler))
        .into_iter()
        .map(|method| (method, path.clone()))
        .collect()
}

pub(super) fn attribute_routes(attributes: &[Attribute]) -> Vec<(&'static str, String, Span)> {
    attributes.iter().flat_map(attribute_route).collect()
}

/// A route attribute carries more than its path. `#[get("/users/{id}", id =
/// "users.read", summary = "Read a user")]` is the ordinary blazingly form,
/// Rocket adds `rank` and `format`, and actix-web names the verb in the list
/// rather than in the attribute. Reading the arguments as one lone string
/// literal dropped every route the moment a second argument appeared, which is
/// every route in a repository that records operation identity next to it.
fn attribute_route(attribute: &Attribute) -> Vec<(&'static str, String, Span)> {
    let Some(declared) = attribute_method(attribute) else {
        return Vec::new();
    };
    let Ok(arguments) = attribute.parse_args_with(Punctuated::<Expr, Token![,]>::parse_terminated)
    else {
        return Vec::new();
    };
    let Some(path) = arguments.iter().find_map(argument_path) else {
        return Vec::new();
    };
    if !path.starts_with('/') {
        return Vec::new();
    }
    let methods = if declared == ANY {
        served_methods(arguments.iter().filter_map(argument_method).collect())
    } else {
        vec![declared]
    };
    let span = syn::spanned::Spanned::span(attribute);
    methods
        .into_iter()
        .map(|method| (method, path.clone(), span))
        .collect()
}

/// A route whose verbs the source never spells out still serves something.
fn served_methods(methods: Vec<&'static str>) -> Vec<&'static str> {
    if methods.is_empty() {
        vec![ANY]
    } else {
        methods
    }
}

/// The path is the first positional string literal, or the value of the one
/// key a framework uses to name it: `path` in blazingly's universal
/// `#[operation(...)]`, `uri` in Rocket's `#[route(...)]`. Other keys are never
/// read as a path, so a `format` or `external_docs` value cannot become one.
fn argument_path(argument: &Expr) -> Option<String> {
    let Expr::Assign(assignment) = argument else {
        return string_literal(argument);
    };
    let key = callable_name(&assignment.left)?;
    matches!(key.as_str(), "path" | "uri")
        .then(|| string_literal(&assignment.right))
        .flatten()
}

/// actix-web and Rocket keep the verb inside the argument list:
/// `#[route("/p", method = "GET")]` and `#[route(GET, uri = "/p")]` both serve
/// GET, and both read as an untyped `ANY` from the attribute name alone.
fn argument_method(argument: &Expr) -> Option<&'static str> {
    let Expr::Assign(assignment) = argument else {
        return http_method(&callable_name(argument)?);
    };
    if callable_name(&assignment.left)? != "method" {
        return None;
    }
    let value = &assignment.right;
    http_method(&string_literal(value).or_else(|| callable_name(value))?)
}

/// `get(list)` names one method; axum chains `get(list).post(create)` onto a
/// single route, and every verb in the chain is served at that path.
fn handler_methods(handler: &Expr) -> Vec<&'static str> {
    match unwrapped(handler) {
        Expr::MethodCall(chained) => {
            let mut methods = handler_methods(&chained.receiver);
            methods.extend(http_method(&chained.method.to_string()));
            methods
        }
        Expr::Call(call) => handler_method(&call.func),
        expression => handler_method(expression),
    }
}

fn handler_method(expression: &Expr) -> Vec<&'static str> {
    callable_name(expression)
        .as_deref()
        .and_then(http_method)
        .into_iter()
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

/// A one-segment expression path can be passed around as a function value:
/// `iter.and_then(validate)`. It is still a direct symbol reference even
/// though Rust does not spell it as an `ExprCall` at that use site.
pub(super) fn bare_path_name(expression: &Expr) -> Option<String> {
    let Expr::Path(path) = unwrapped(expression) else {
        return None;
    };
    if path.qself.is_some() || path.path.segments.len() != 1 {
        return None;
    }
    path.path
        .segments
        .first()
        .map(|segment| segment.ident.to_string())
}

/// The receiver in `ArchiveOptions::default()` is itself a type reference.
/// Keep this deliberately bounded to a two-segment path: longer paths require
/// module resolution evidence before their penultimate segment can bind.
pub(super) fn associated_owner_name(expression: &Expr) -> Option<String> {
    let Expr::Path(path) = unwrapped(expression) else {
        return None;
    };
    if path.qself.is_some() || path.path.segments.len() != 2 {
        return None;
    }
    path.path
        .segments
        .first()
        .map(|segment| segment.ident.to_string())
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
    let name = attribute.path().segments.last()?.ident.to_string();
    match name.as_str() {
        // The verb is declared by an argument, not by the attribute name.
        "route" | "operation" => Some(ANY),
        _ => http_method(&name),
    }
}

fn http_method(name: &str) -> Option<&'static str> {
    Some(match name.to_ascii_uppercase().as_str() {
        "GET" => "GET",
        "POST" => "POST",
        "PUT" => "PUT",
        "PATCH" => "PATCH",
        "DELETE" => "DELETE",
        "HEAD" => "HEAD",
        "OPTIONS" => "OPTIONS",
        "TRACE" => "TRACE",
        "CONNECT" => "CONNECT",
        _ => return None,
    })
}
