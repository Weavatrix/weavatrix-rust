use weavatrix_graph::{EdgeKind, NodeKind, SourcePosition, SourceSpan};
use weavatrix_parse::{DeclarationKind, ReferenceKind, Span};

pub(super) fn node_kind(kind: DeclarationKind) -> NodeKind {
    match kind {
        DeclarationKind::Function | DeclarationKind::Procedure => NodeKind::Function,
        DeclarationKind::Method => NodeKind::Method,
        DeclarationKind::Class | DeclarationKind::Struct => NodeKind::Struct,
        DeclarationKind::Interface | DeclarationKind::Trait => NodeKind::Trait,
        DeclarationKind::Enum => NodeKind::Enum,
        DeclarationKind::TypeAlias => NodeKind::TypeAlias,
        DeclarationKind::Constant => NodeKind::Constant,
        DeclarationKind::Module => NodeKind::Module,
        DeclarationKind::Field => NodeKind::Custom("field".to_owned()),
        DeclarationKind::Variable => NodeKind::Custom("variable".to_owned()),
        DeclarationKind::Table => NodeKind::Table,
        DeclarationKind::View => NodeKind::Custom("view".to_owned()),
        DeclarationKind::Selector => NodeKind::Custom("selector".to_owned()),
        DeclarationKind::Resource => NodeKind::Custom("resource".to_owned()),
        DeclarationKind::Heading => NodeKind::Custom("heading".to_owned()),
        _ => NodeKind::Custom(format!("parser:{kind:?}").to_ascii_lowercase()),
    }
}

pub(super) fn edge_kind(kind: ReferenceKind) -> EdgeKind {
    match kind {
        ReferenceKind::Call => EdgeKind::Calls,
        ReferenceKind::Inherits => EdgeKind::Inherits,
        ReferenceKind::Implements => EdgeKind::Implements,
        _ => EdgeKind::References,
    }
}

pub(super) fn span(span: &Span, path: &str) -> SourceSpan {
    SourceSpan::new(
        path,
        SourcePosition {
            line: span.line,
            column: span.column,
        },
        SourcePosition {
            line: span.end_line,
            column: span.end_column,
        },
    )
}
