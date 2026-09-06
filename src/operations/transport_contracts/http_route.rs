pub(super) fn route_query(route: &str) -> &str {
    let boundary = route
        .char_indices()
        .find(|(_, character)| matches!(character, ':' | '{' | '$' | '*'))
        .map_or(route.len(), |(index, _)| index);
    let prefix = &route[..boundary];
    prefix.trim_end_matches('/').rsplit_once('/').map_or_else(
        || prefix.trim_end_matches('/'),
        |(_, tail)| {
            if tail.is_empty() {
                prefix.trim_end_matches('/')
            } else {
                prefix
            }
        },
    )
}

pub(super) fn route_matches(route: &str, line: &str) -> bool {
    if line.contains(route) {
        return true;
    }
    let static_parts = route
        .split('/')
        .filter(|part| {
            !part.is_empty()
                && !part.starts_with(':')
                && !part.starts_with('{')
                && !part.contains('$')
                && *part != "*"
        })
        .collect::<Vec<_>>();
    !static_parts.is_empty() && static_parts.iter().all(|part| line.contains(part))
}

#[cfg(test)]
mod tests {
    use super::{route_matches, route_query};

    #[test]
    fn route_query_keeps_static_prefix() {
        assert_eq!(route_query("/api/orders/:id"), "/api/orders/");
        assert_eq!(route_query("/api/orders"), "/api/orders");
    }

    #[test]
    fn route_matches_literal_and_template_parts() {
        assert!(route_matches("/api/orders", "fetch('/api/orders')"));
        assert!(route_matches(
            "/api/orders/:id",
            "fetch('/api/orders/' + id)"
        ));
    }
}
