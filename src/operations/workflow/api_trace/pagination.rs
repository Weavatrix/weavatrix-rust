use super::model::EvidencePage;
use crate::operations::{optional_str, optional_u64};
use blazingly_json::Value;

pub(super) fn paginate(evidence: &[Value], args: &Value) -> Result<EvidencePage, String> {
    let page_size = usize::try_from(optional_u64(args, "page_size")?.unwrap_or(10))
        .map_err(|_| "page_size is too large".to_owned())?
        .clamp(1, 50);
    let offset = cursor_offset(args)?;
    if offset > evidence.len() {
        return Err("cursor offset is outside the current evidence set".to_owned());
    }
    let detail = optional_str(args, "response_detail")?.unwrap_or("compact");
    if !matches!(detail, "compact" | "full") {
        return Err("response_detail must be compact or full".to_owned());
    }
    let end = offset.saturating_add(page_size).min(evidence.len());
    Ok(EvidencePage {
        detail: detail.to_owned(),
        offset,
        page_size,
        total_items: evidence.len(),
        end,
        items: evidence[offset..end].to_vec(),
    })
}

fn cursor_offset(args: &Value) -> Result<usize, String> {
    let Some(cursor) = optional_str(args, "cursor")? else {
        return Ok(0);
    };
    let offset = cursor
        .strip_prefix("v1:")
        .ok_or_else(|| "cursor format is invalid; expected v1:<offset>".to_owned())?
        .parse::<usize>()
        .map_err(|_| "cursor offset is invalid".to_owned())?;
    Ok(offset)
}
