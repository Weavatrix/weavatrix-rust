//! Deterministic placement for the module map.
//!
//! Positions start on a circle in a fixed order and are relaxed by a fixed
//! number of Fruchterman-Reingold iterations. No randomness enters the layout,
//! so the same graph always renders the same picture and a report can be
//! compared byte for byte between two runs.

pub(super) const WIDTH: f64 = 960.0;
pub(super) const HEIGHT: f64 = 600.0;
const MARGIN: f64 = 72.0;
const ITERATIONS: usize = 240;
const COOLING: f64 = 0.94;
/// A tight cluster fills the frame, but never so far that two modules a few
/// pixels apart are thrown to opposite corners.
const MAX_MAGNIFICATION: f64 = 4.0;

/// One module-to-module coupling: the two module indexes and its weight.
pub(super) type Link = (usize, usize, u64);

pub(super) fn place(count: usize, links: &[Link]) -> Vec<(f64, f64)> {
    let mut positions = ring(count);
    if count < 2 {
        return positions;
    }
    let ideal = (WIDTH * HEIGHT / scale(count)).sqrt();
    let mut temperature = WIDTH / 8.0;
    for _ in 0..ITERATIONS {
        let mut push = vec![(0.0_f64, 0.0_f64); count];
        repel(&positions, ideal, &mut push);
        attract(&positions, links, ideal, &mut push);
        advance(&mut positions, &push, temperature);
        temperature *= COOLING;
    }
    normalize(&mut positions);
    positions
}

/// Rescales the relaxed layout to fill the frame.
///
/// Repulsion pushes unconnected modules against the edges and lets the coupled
/// cluster collapse into a corner, which wastes most of the picture. A uniform
/// rescale keeps every relative distance and spends the whole frame on them.
fn normalize(positions: &mut [(f64, f64)]) {
    let (mut min_x, mut min_y) = (f64::MAX, f64::MAX);
    let (mut max_x, mut max_y) = (f64::MIN, f64::MIN);
    for (x, y) in &*positions {
        min_x = min_x.min(*x);
        max_x = max_x.max(*x);
        min_y = min_y.min(*y);
        max_y = max_y.max(*y);
    }
    let span_x = (max_x - min_x).max(0.01);
    let span_y = (max_y - min_y).max(0.01);
    let factor = ((WIDTH - 2.0 * MARGIN) / span_x)
        .min((HEIGHT - 2.0 * MARGIN) / span_y)
        .min(MAX_MAGNIFICATION);
    let centre = (f64::midpoint(min_x, max_x), f64::midpoint(min_y, max_y));
    for point in positions.iter_mut() {
        point.0 = WIDTH / 2.0 + (point.0 - centre.0) * factor;
        point.1 = HEIGHT / 2.0 + (point.1 - centre.1) * factor;
    }
}

/// Evenly spaced on a circle, in the caller's order.
fn ring(count: usize) -> Vec<(f64, f64)> {
    let centre = (WIDTH / 2.0, HEIGHT / 2.0);
    let radius = (HEIGHT / 2.0) - MARGIN;
    (0..count)
        .map(|index| {
            let angle = std::f64::consts::TAU * scale(index) / scale(count.max(1));
            (
                centre.0 + radius * angle.cos(),
                centre.1 + radius * angle.sin(),
            )
        })
        .collect()
}

fn repel(positions: &[(f64, f64)], ideal: f64, push: &mut [(f64, f64)]) {
    for left in 0..positions.len() {
        for right in (left + 1)..positions.len() {
            let (dx, dy, distance) = delta(positions[left], positions[right]);
            let force = ideal * ideal / distance;
            let (ux, uy) = (dx / distance * force, dy / distance * force);
            push[left].0 += ux;
            push[left].1 += uy;
            push[right].0 -= ux;
            push[right].1 -= uy;
        }
    }
}

/// Coupled modules pull together, and a heavier coupling pulls harder. The
/// weight is dampened by a logarithm so one enormous import count cannot
/// collapse the whole picture onto a single pair.
fn attract(positions: &[(f64, f64)], links: &[Link], ideal: f64, push: &mut [(f64, f64)]) {
    for (left, right, weight) in links {
        let (Some(from), Some(to)) = (positions.get(*left), positions.get(*right)) else {
            continue;
        };
        let (dx, dy, distance) = delta(*from, *to);
        let force = distance * distance / ideal * (1.0 + scale_u64(*weight)).ln();
        let (ux, uy) = (dx / distance * force, dy / distance * force);
        push[*left].0 -= ux;
        push[*left].1 -= uy;
        push[*right].0 += ux;
        push[*right].1 += uy;
    }
}

fn advance(positions: &mut [(f64, f64)], push: &[(f64, f64)], temperature: f64) {
    for (position, (dx, dy)) in positions.iter_mut().zip(push.iter()) {
        let length = (dx * dx + dy * dy).sqrt().max(0.01);
        let step = length.min(temperature);
        position.0 = (position.0 + dx / length * step).clamp(MARGIN, WIDTH - MARGIN);
        position.1 = (position.1 + dy / length * step).clamp(MARGIN, HEIGHT - MARGIN);
    }
}

/// The vector between two points, with a floor on the distance so coincident
/// nodes produce a finite force instead of an infinity.
fn delta(from: (f64, f64), to: (f64, f64)) -> (f64, f64, f64) {
    let (dx, dy) = (from.0 - to.0, from.1 - to.1);
    (dx, dy, (dx * dx + dy * dy).sqrt().max(0.01))
}

fn scale(count: usize) -> f64 {
    f64::from(u32::try_from(count).unwrap_or(u32::MAX))
}

fn scale_u64(count: u64) -> f64 {
    f64::from(u32::try_from(count).unwrap_or(u32::MAX))
}
