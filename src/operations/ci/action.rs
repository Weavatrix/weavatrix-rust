use super::read::Loaded;
use super::workflow::{self, Step};

pub(super) struct Action {
    pub path: String,
    pub digest: String,
    pub bytes: Vec<u8>,
    pub steps: Vec<Step>,
}

pub(super) fn parse(input: &Loaded) -> Result<Action, String> {
    let root = workflow::load(input)?;
    let runs = workflow::key(&root, "runs").ok_or_else(|| "missing runs".to_owned())?;
    if workflow::key(runs, "using")
        .and_then(workflow::scalar)
        .as_deref()
        != Some("composite")
    {
        return Err("non-composite action body unsupported".to_owned());
    }
    Ok(Action {
        path: input.path.clone(),
        digest: input.digest.clone(),
        bytes: input.bytes.clone(),
        steps: workflow::steps(workflow::key(runs, "steps"))?,
    })
}

pub(super) fn local_path(reference: &str) -> Option<String> {
    let relative = reference.strip_prefix("./")?;
    if relative.is_empty()
        || relative.contains("${{")
        || relative.split('/').any(|part| part == ".." || part == ".")
    {
        return None;
    }
    Some(relative.to_owned())
}
