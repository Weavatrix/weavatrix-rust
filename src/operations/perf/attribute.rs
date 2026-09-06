//! Argument handling, the pairwise revision walk, and report assembly.

use super::steps::Changed;
use super::{rollup, round, series, sources, steps};
use crate::analyzer::Analyzer;
use crate::engine::RepositoryState;
use crate::operations::history::resolve_revision;
use crate::operations::history::revision::revision_graph;
use crate::operations::{arg_str, health, optional_str, optional_u64};
use blazingly_json::{Value, json};
use weavatrix_git::{ObjectId, Repository};
use weavatrix_graph::Graph;

const MODEL: &str = "each step pairs two measured revisions and lists the declarations that \
     differ between them; a declaration is credited with the step delta divided by the number of \
     declarations that changed with it. This is co-occurrence between a caller's measurement and \
     static structural change, not profiler attribution: a step that moved many declarations is \
     weak evidence for each of them, and a step inside the measured noise floor is evidence for \
     none.";

#[derive(Clone, Copy, PartialEq, Eq)]
enum Direction {
    LowerIsBetter,
    HigherIsBetter,
}

impl Direction {
    fn parse(args: &Value) -> Result<Self, String> {
        match optional_str(args, "direction")?.unwrap_or("lower_is_better") {
            "lower_is_better" => Ok(Self::LowerIsBetter),
            "higher_is_better" => Ok(Self::HigherIsBetter),
            other => Err(format!(
                "direction {other:?} is invalid; expected lower_is_better or higher_is_better"
            )),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::LowerIsBetter => "lower_is_better",
            Self::HigherIsBetter => "higher_is_better",
        }
    }

    /// Anything inside the caller's noise band is flat, not a result.
    fn verdict(self, delta: f64, percent: Option<f64>, flat_percent: f64) -> &'static str {
        if percent.unwrap_or(f64::INFINITY).abs() <= flat_percent {
            return "flat";
        }
        let better = match self {
            Self::LowerIsBetter => delta < 0.0,
            Self::HigherIsBetter => delta > 0.0,
        };
        if better { "improved" } else { "regressed" }
    }
}

struct Settings {
    direction: Direction,
    flat_percent: f64,
    top: usize,
    scope: Option<String>,
}

/// One measured revision that this repository actually contains.
struct Point {
    row: usize,
    revision: String,
    resolved: ObjectId,
    value: f64,
}

struct Walked {
    steps: Vec<Value>,
    by_symbol: Vec<Value>,
    noise: Vec<Value>,
    analyses: usize,
}

pub(in crate::operations) fn attribution(
    state: &RepositoryState,
    args: &Value,
) -> Result<Value, String> {
    let metric = arg_str(args, "metric")?;
    let file = arg_str(args, "measurements_file")?;
    let revision_column = optional_str(args, "revision_column")?.unwrap_or("commit");
    let max_revisions = usize::try_from(optional_u64(args, "max_revisions")?.unwrap_or(12))
        .map_err(|_| "max_revisions is too large".to_owned())?;
    if max_revisions < 2 {
        return Err("max_revisions must be at least 2: one step needs two measurements".to_owned());
    }
    let settings = Settings {
        direction: Direction::parse(args)?,
        flat_percent: percent_band(args)?,
        top: usize::try_from(optional_u64(args, "top_n")?.unwrap_or(20))
            .map_err(|_| "top_n is too large".to_owned())?,
        scope: health::paths::requested_path_scope(args)?,
    };

    let repository = Repository::open(state.root()).map_err(|error| error.to_string())?;
    let mut parsed = series::read(state.root(), file, revision_column, metric)?;
    let points = resolve(&repository, &mut parsed, max_revisions);
    if points.len() < 2 {
        return Err(format!(
            "{file} yields {} usable revision(s) for column {metric}; one step needs two",
            points.len()
        ));
    }
    let walked = walk(state, &repository, &points, args, &settings)?;
    Ok(report(file, metric, &settings, &parsed, &points, &walked))
}

/// The band inside which a step counts as noise rather than as a result.
fn percent_band(args: &Value) -> Result<f64, String> {
    let value = optional_u64(args, "min_delta_percent")?.unwrap_or(0);
    Ok(f64::from(u32::try_from(value).map_err(|_| {
        "min_delta_percent is too large".to_owned()
    })?))
}

/// Resolves every measured revision, recording the ones this repository does
/// not contain instead of dropping them silently, then keeps the most recent
/// `max_revisions` points.
fn resolve(
    repository: &Repository,
    parsed: &mut series::Series,
    max_revisions: usize,
) -> Vec<Point> {
    let mut points = Vec::new();
    for measurement in &parsed.measurements {
        match resolve_revision(repository, &measurement.revision) {
            Ok(resolved) => points.push(Point {
                row: measurement.row,
                revision: measurement.revision.clone(),
                resolved,
                value: measurement.value,
            }),
            Err(error) => parsed.skipped.push((measurement.row, error)),
        }
    }
    if points.len() > max_revisions {
        points.drain(..points.len() - max_revisions);
    }
    points
}

/// Walks the series in order, holding at most two revision graphs at a time.
/// Each distinct revision is analyzed once even though it takes part in two
/// steps.
fn walk(
    state: &RepositoryState,
    repository: &Repository,
    points: &[Point],
    args: &Value,
    settings: &Settings,
) -> Result<Walked, String> {
    let analyzer = Analyzer::default();
    let mut rollup = rollup::Rollup::new();
    let mut walked = Walked {
        steps: Vec::new(),
        by_symbol: Vec::new(),
        noise: Vec::new(),
        analyses: 0,
    };
    let mut previous: Option<(&Point, Graph)> = None;
    for point in points {
        let Some((before, before_graph)) = previous.take() else {
            walked.analyses += 1;
            previous = Some((
                point,
                revision_graph(&analyzer, repository, state, point.resolved)?,
            ));
            continue;
        };
        // A repeated revision measures the harness, not the code: there is
        // nothing to analyze and nothing to attribute.
        if before.resolved == point.resolved {
            let (rendered, _) = step(before, point, &Changed::none(), settings, "same_revision");
            walked.noise.push(rendered.clone());
            walked.steps.push(rendered);
            previous = Some((point, before_graph));
            continue;
        }
        walked.analyses += 1;
        let after_graph = revision_graph(&analyzer, repository, state, point.resolved)?;
        let texts = sources::changed_files(repository, &before_graph, &after_graph);
        let changed = steps::changed(
            &before_graph,
            &after_graph,
            &texts,
            args,
            settings.scope.as_deref(),
        );
        let (rendered, verdict) = step(before, point, &changed, settings, "compared");
        rollup.record(&changed.symbols, delta(before, point), verdict);
        walked.steps.push(rendered);
        previous = Some((point, after_graph));
    }
    walked.by_symbol = rollup.into_values(settings.top);
    Ok(walked)
}

fn step(
    before: &Point,
    after: &Point,
    changed: &Changed,
    settings: &Settings,
    kind: &'static str,
) -> (Value, &'static str) {
    let delta = delta(before, after);
    let percent = percent(before.value, delta);
    let verdict = settings
        .direction
        .verdict(delta, percent, settings.flat_percent);
    let rendered = json!({
        "kind": kind,
        "from": endpoint(before),
        "to": endpoint(after),
        "delta": round(delta),
        "delta_percent": percent.map(round),
        "verdict": verdict,
        "changed_symbols": changed.symbols.len(),
        "changed_files": changed.files,
        "symbols": changed.symbols.iter().take(settings.top)
            .map(steps::render).collect::<Vec<_>>()
    });
    (rendered, verdict)
}

fn endpoint(point: &Point) -> Value {
    json!({
        "revision": point.revision,
        "resolved": point.resolved.to_string(),
        "row": point.row,
        "value": point.value
    })
}

fn delta(before: &Point, after: &Point) -> f64 {
    after.value - before.value
}

/// Relative change, or nothing when the baseline is zero and a percentage
/// would be an invented number.
fn percent(baseline: f64, delta: f64) -> Option<f64> {
    (baseline.abs() > f64::EPSILON).then(|| delta / baseline * 100.0)
}

fn report(
    file: &str,
    metric: &str,
    settings: &Settings,
    parsed: &series::Series,
    points: &[Point],
    walked: &Walked,
) -> Value {
    json!({
        "status": "COMPLETE",
        "git_evidence": {"present": true},
        "metric": metric,
        "direction": settings.direction.as_str(),
        "series": {
            "file": file,
            "columns": parsed.columns,
            "data_rows": parsed.data_rows,
            "measurements_used": points.len(),
            "skipped_rows": parsed.skipped.len(),
            "skipped": parsed.skipped.iter().take(settings.top)
                .map(|(row, reason)| json!({"row": row, "reason": reason}))
                .collect::<Vec<_>>()
        },
        "revisions_analyzed": walked.analyses,
        "noise_floor": noise_floor(&walked.noise),
        "steps": walked.steps,
        "by_symbol": walked.by_symbol,
        "model": MODEL,
        "source_mutation": "NONE"
    })
}

/// Steps whose two revisions are identical measure the harness, not the code.
fn noise_floor(samples: &[Value]) -> Value {
    if samples.is_empty() {
        return json!({
            "measured": false,
            "reason": "no step repeated a revision; measure one revision twice to establish the harness noise band"
        });
    }
    let worst = samples
        .iter()
        .filter_map(|sample| sample["delta_percent"].as_f64())
        .fold(0.0_f64, |worst, value| worst.max(value.abs()));
    json!({
        "measured": true,
        "samples": samples.len(),
        "max_abs_percent": round(worst),
        "note": "a step delta inside this band is not attributable to any declaration"
    })
}
