use crate::engine::RepositoryState;
use blazingly_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use weavatrix_graph::EdgeKind;

const DEFAULT_MAX_VISITED: usize = 2_000;
const HARD_MAX_VISITED: usize = 10_000;
const DEFAULT_MAX_EDGES: usize = 4_000;

pub(super) struct WalkSpec<'a> {
    pub start: &'a str,
    pub depth: usize,
    pub max_nodes: usize,
    pub offset: usize,
    pub relations: &'a [&'a str],
    pub port_kind: &'a str,
    pub owner_kinds: &'a [&'a str],
    pub incoming: bool,
    pub revision: &'a str,
}

pub(super) fn trace(state: &RepositoryState, spec: WalkSpec<'_>) -> Value {
    let index = Adjacency::build(state, spec.port_kind, spec.owner_kinds);
    let mut cycle = false;
    let mut steps = Vec::new();
    let mut reasons = BTreeSet::new();
    for relation in spec.relations {
        walk_relation(
            &index,
            spec.start,
            spec.depth,
            relation,
            spec.incoming,
            spec.max_nodes,
            &mut steps,
            &mut reasons,
        );
        cycle |= relation_has_cycle(&steps, relation);
    }
    let total = steps.len();
    let end = spec.offset.saturating_add(spec.max_nodes).min(total);
    let page = if spec.offset > total {
        Vec::new()
    } else {
        steps[spec.offset..end].to_vec()
    };
    if end < total {
        reasons.insert("page");
    }
    json!({
        "start": spec.start,
        "steps": page,
        "cycle": cycle,
        "page": {
            "offset": spec.offset,
            "returned": page.len(),
            "total": total,
            "has_more": end < total,
            "next_cursor": (end < total).then(|| format!("v1:{end}:{}", spec.revision))
        },
        "bounds": {
            "truncated": !reasons.is_empty(),
            "found": total,
            "shown": page.len(),
            "reasons": reasons.into_iter().collect::<Vec<_>>(),
            "revision": spec.revision,
            "runtime": false
        }
    })
}

pub(super) fn page_offset_for(args: &Value, revision: &str) -> Result<usize, String> {
    let Some(cursor) = args.get("cursor").and_then(Value::as_str) else {
        return Ok(0);
    };
    let Some(rest) = cursor.strip_prefix("v1:") else {
        return Err(
            "cursor format is invalid; expected v1:<offset> or v1:<offset>:<revision>".to_owned(),
        );
    };
    let mut parts = rest.splitn(2, ':');
    let offset = parts
        .next()
        .ok_or_else(|| "cursor offset is invalid".to_owned())?
        .parse::<usize>()
        .map_err(|_| "cursor offset is invalid".to_owned())?;
    if let Some(bound) = parts.next()
        && bound != revision
    {
        return Err("cursor belongs to a different revision".to_owned());
    }
    Ok(offset)
}

pub(super) fn incoming_arg(args: &Value) -> Result<bool, String> {
    match args.get("direction").and_then(Value::as_str) {
        None | Some("outgoing") => Ok(false),
        Some("incoming") => Ok(true),
        Some(other) => Err(format!(
            "direction must be outgoing or incoming, not {other}"
        )),
    }
}

struct Adjacency {
    outgoing: BTreeMap<String, Vec<Hop>>,
    incoming: BTreeMap<String, Vec<Hop>>,
    children: BTreeMap<String, Vec<String>>,
    parent: BTreeMap<String, String>,
    kinds: BTreeMap<String, String>,
    port_kind: String,
    owner_kinds: BTreeSet<String>,
}

#[derive(Clone)]
struct Hop {
    from: String,
    to: String,
    relation: String,
    detail: String,
}

impl Adjacency {
    fn build(state: &RepositoryState, port_kind: &str, owner_kinds: &[&str]) -> Self {
        let kinds = state
            .graph()
            .nodes()
            .iter()
            .map(|node| (node.id.as_str().to_owned(), node.kind.as_str().to_owned()))
            .collect::<BTreeMap<_, _>>();
        let mut outgoing = BTreeMap::<String, Vec<Hop>>::new();
        let mut incoming = BTreeMap::<String, Vec<Hop>>::new();
        let mut children = BTreeMap::<String, Vec<String>>::new();
        let mut parent = BTreeMap::<String, String>::new();
        for edge in state.graph().edges() {
            if edge.kind == EdgeKind::Contains {
                children
                    .entry(edge.source.as_str().to_owned())
                    .or_default()
                    .push(edge.target.as_str().to_owned());
                parent
                    .entry(edge.target.as_str().to_owned())
                    .or_insert_with(|| edge.source.as_str().to_owned());
                continue;
            }
            let hop = Hop {
                from: edge.source.as_str().to_owned(),
                to: edge.target.as_str().to_owned(),
                relation: edge.kind.as_str().to_owned(),
                detail: edge.provenance.detail.clone().unwrap_or_default(),
            };
            outgoing
                .entry(hop.from.clone())
                .or_default()
                .push(hop.clone());
            incoming.entry(hop.to.clone()).or_default().push(hop);
        }
        Self {
            outgoing,
            incoming,
            children,
            parent,
            kinds,
            port_kind: port_kind.to_owned(),
            owner_kinds: owner_kinds.iter().map(|kind| (*kind).to_owned()).collect(),
        }
    }

    fn hops(&self, id: &str, incoming: bool) -> &[Hop] {
        if incoming {
            self.incoming.get(id).map_or(&[], Vec::as_slice)
        } else {
            self.outgoing.get(id).map_or(&[], Vec::as_slice)
        }
    }

    fn expand(&self, id: &str) -> Vec<String> {
        let mut ids = vec![id.to_owned()];
        if self.is_owner(id)
            && let Some(children) = self.children.get(id)
        {
            for child in children {
                if self.is_port(child) {
                    ids.push(child.clone());
                }
            }
        }
        if self.is_port(id)
            && let Some(owner) = self.parent.get(id)
            && self.is_owner(owner)
        {
            ids.push(owner.clone());
            if let Some(siblings) = self.children.get(owner) {
                for sibling in siblings {
                    if self.is_port(sibling) {
                        ids.push(sibling.clone());
                    }
                }
            }
        }
        ids.sort();
        ids.dedup();
        ids
    }

    fn is_port(&self, id: &str) -> bool {
        self.kinds
            .get(id)
            .is_some_and(|kind| kind == &self.port_kind)
    }

    fn is_owner(&self, id: &str) -> bool {
        self.kinds
            .get(id)
            .is_some_and(|kind| self.owner_kinds.contains(kind))
    }
}

#[allow(clippy::too_many_arguments)]
fn walk_relation(
    index: &Adjacency,
    start: &str,
    depth: usize,
    relation: &str,
    incoming: bool,
    page_cap: usize,
    steps: &mut Vec<Value>,
    reasons: &mut BTreeSet<&'static str>,
) {
    let mut expanded = BTreeSet::<String>::new();
    let mut witnessed = BTreeSet::<(String, String, String)>::new();
    let mut queue = VecDeque::new();
    for seed in index.expand(start) {
        queue.push_back((seed, 0_usize));
    }
    let max_visited = DEFAULT_MAX_VISITED.min(HARD_MAX_VISITED);
    let max_edges = DEFAULT_MAX_EDGES.max(page_cap.saturating_mul(8));
    while let Some((id, hop)) = queue.pop_front() {
        if hop >= depth {
            reasons.insert("depth");
            continue;
        }
        if expanded.len() >= max_visited {
            reasons.insert("max_visited");
            break;
        }
        if !expanded.insert(id.clone()) {
            continue;
        }
        for hop_edge in index.hops(&id, incoming) {
            if hop_edge.relation != relation {
                continue;
            }
            let next = if incoming {
                hop_edge.from.as_str()
            } else {
                hop_edge.to.as_str()
            };
            let key = (
                hop_edge.from.clone(),
                hop_edge.to.clone(),
                hop_edge.relation.clone(),
            );
            if witnessed.insert(key) {
                if steps.len() >= max_edges {
                    reasons.insert("max_edges");
                    return;
                }
                steps.push(json!({
                    "from": hop_edge.from,
                    "to": hop_edge.to,
                    "relation": hop_edge.relation,
                    "detail": hop_edge.detail,
                    "hop": hop + 1
                }));
            }
            if hop + 1 >= depth {
                reasons.insert("depth");
                continue;
            }
            for next in index.expand(next) {
                if !expanded.contains(&next) {
                    queue.push_back((next, hop + 1));
                }
            }
        }
    }
}

fn relation_has_cycle(steps: &[Value], relation: &str) -> bool {
    let mut outgoing = BTreeMap::<&str, Vec<&str>>::new();
    for step in steps {
        if step["relation"].as_str() != Some(relation) {
            continue;
        }
        let Some(from) = step["from"].as_str() else {
            continue;
        };
        let Some(to) = step["to"].as_str() else {
            continue;
        };
        outgoing.entry(from).or_default().push(to);
    }
    let mut color = BTreeMap::<&str, u8>::new();
    fn dfs<'a>(
        node: &'a str,
        outgoing: &BTreeMap<&'a str, Vec<&'a str>>,
        color: &mut BTreeMap<&'a str, u8>,
    ) -> bool {
        color.insert(node, 1);
        for next in outgoing.get(node).into_iter().flatten() {
            match color.get(next).copied().unwrap_or(0) {
                1 => return true,
                0 if dfs(next, outgoing, color) => return true,
                _ => {}
            }
        }
        color.insert(node, 2);
        false
    }
    outgoing
        .keys()
        .copied()
        .any(|node| color.get(node).copied().unwrap_or(0) == 0 && dfs(node, &outgoing, &mut color))
}
