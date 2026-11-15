use crate::*;
use std::collections::{HashMap, HashSet};
#[derive(Clone, Copy, Default)]
pub struct Box2 {
    pub left: f64,
    pub right: f64,
    pub top: f64,
    pub bottom: f64,
}
impl Box2 {
    fn overlaps(self, b: Self) -> bool {
        self.left < b.right && self.right > b.left && self.top < b.bottom && self.bottom > b.top
    }
    fn area(self, b: Self) -> f64 {
        (self.right.min(b.right) - self.left.max(b.left)).max(0.)
            * (self.bottom.min(b.bottom) - self.top.max(b.top)).max(0.)
    }
    fn distance(self, p: Point) -> f64 {
        (self.left - p.x)
            .max(0.)
            .max(p.x - self.right)
            .hypot((self.top - p.y).max(0.).max(p.y - self.bottom))
    }
    fn translated(self, x: f64, y: f64) -> Self {
        Self {
            left: self.left + x,
            right: self.right + x,
            top: self.top + y,
            bottom: self.bottom + y,
        }
    }
}
#[derive(Clone, Copy)]
pub struct Measurement {
    pub width: f64,
    pub left: f64,
    pub right: f64,
    pub ascent: f64,
    pub descent: f64,
}
thread_local! {static CACHE:RefCell<HashMap<(String,u64),Measurement>>=RefCell::new(HashMap::new());static CONTEXT:Option<CanvasRenderingContext2d>=document().create_element("canvas").ok().and_then(|c|c.dyn_into::<HtmlCanvasElement>().ok()).and_then(|c|c.get_context("2d").ok().flatten()).and_then(|c|c.dyn_into().ok());}
pub fn invalidate() {
    CACHE.with(|c| c.borrow_mut().clear());
    retained::invalidate();
}
pub fn ink(text: &str, font: &str) -> Option<TextMetrics> {
    CONTEXT.with(|c| {
        c.as_ref().and_then(|c| {
            c.set_font(font);
            c.measure_text(text).ok()
        })
    })
}
pub fn measure(text: &str, size: f64) -> Measurement {
    if text.is_empty() {
        return Measurement {
            width: 0.,
            left: 0.,
            right: 0.,
            ascent: 0.,
            descent: 0.,
        };
    }
    let key = (text.to_string(), size.to_bits());
    if let Some(value) = CACHE.with(|c| c.borrow().get(&key).copied()) {
        return value;
    }
    let value = CONTEXT.with(|c| {
        c.as_ref()
            .and_then(|c| {
                c.set_font(&format!("{size}px 'DM Sans Variable'"));
                c.measure_text(text).ok()
            })
            .map(|m| Measurement {
                width: m.width(),
                left: -m.actual_bounding_box_left(),
                right: m.actual_bounding_box_right(),
                ascent: m.actual_bounding_box_ascent(),
                descent: m.actual_bounding_box_descent(),
            })
            .unwrap_or(Measurement {
                width: text.encode_utf16().count() as f64 * size * 0.6,
                left: 0.,
                right: text.encode_utf16().count() as f64 * size * 0.6,
                ascent: size * 0.8,
                descent: size * 0.2,
            })
    });
    CACHE.with(|c| {
        let mut c = c.borrow_mut();
        if c.len() > 10000 {
            c.clear();
        }
        c.insert(key, value);
    });
    value
}
#[derive(Clone)]
pub struct LabelPoint {
    pub point: Point,
    pub family: String,
    pub name: String,
    pub effort: String,
    pub suffix: String,
    pub priority: u8,
}
#[derive(Clone)]
pub struct Line {
    pub text: String,
    pub x: f64,
    pub y: f64,
    pub bounds: Box2,
}
#[derive(Clone)]
pub struct Label {
    pub name: Option<Line>,
    pub effort: Option<Line>,
    pub bounds: Box2,
    pub cost: f64,
}
impl Label {
    fn lines(&self) -> impl Iterator<Item = &Line> {
        self.name.iter().chain(&self.effort)
    }
    fn reading_box(&self) -> Box2 {
        let b = self.bounds;
        let (cx, cy) = ((b.left + b.right) / 2., (b.top + b.bottom) / 2.);
        let (hw, hh) = (
            ((b.right - b.left) / 2.).max(30.) + 7.,
            ((b.bottom - b.top) / 2.).max(12.) + 6.,
        );
        Box2 {
            left: cx - hw,
            right: cx + hw,
            top: cy - hh,
            bottom: cy + hh,
        }
    }
}
fn density(candidate: &Label, layout: &HashMap<usize, Label>, frame: Box2) -> bool {
    let cb = candidate.reading_box();
    let boxes: Vec<_> = layout
        .values()
        .map(Label::reading_box)
        .chain([cb])
        .collect();
    for b in &boxes {
        let (cx, cy) = ((b.left + b.right) / 2., (b.top + b.bottom) / 2.);
        let neighborhood = Box2 {
            left: frame.left.max(cx - 110.),
            right: frame.right.min(cx + 110.),
            top: frame.top.max(cy - 70.),
            bottom: frame.bottom.min(cy + 70.),
        };
        if !neighborhood.overlaps(cb) {
            continue;
        }
        let areas: Vec<_> = boxes.iter().map(|b| b.area(neighborhood)).collect();
        let available =
            (neighborhood.right - neighborhood.left) * (neighborhood.bottom - neighborhood.top);
        if areas.iter().sum::<f64>() > areas.iter().copied().fold(available * 0.32, f64::max) + 0.01
        {
            return false;
        }
    }
    true
}
fn readable(candidate: &Label, layout: &HashMap<usize, Label>, frame: Box2) -> bool {
    !layout
        .values()
        .any(|label| candidate.reading_box().overlaps(label.reading_box()))
        && density(candidate, layout, frame)
}
fn candidates(point: &LabelPoint, points: &[LabelPoint], frame: Box2, compact: bool) -> Vec<Label> {
    let name = if compact { "" } else { &point.name };
    let effort = format!(
        "{}{}",
        point.effort,
        if compact { &point.suffix } else { "" }
    );
    let ns = measure(name, 11.);
    let es = measure(&effort, 10.);
    let (nw, ew) = (ns.right - ns.left, es.right - es.left);
    let (nh, eh) = (ns.ascent + ns.descent, es.ascent + es.descent);
    let width = nw.max(ew);
    let first = if name.is_empty() { eh } else { nh };
    let height = first
        + if !name.is_empty() && !effort.is_empty() {
            4. + eh
        } else {
            0.
        };
    let mut result = Vec::new();
    let mut seen = HashSet::new();
    let mut add = |align: f64, left: f64, top: f64, preference: f64| {
        if width + 4. > frame.right - frame.left || height + 4. > frame.bottom - frame.top {
            return;
        }
        let left = left.max(frame.left + 2.).min(frame.right - width - 2.);
        let top = top.max(frame.top + 2.).min(frame.bottom - height - 2.);
        let make_line = |text: &str, size: Measurement, x: f64, y: f64| Line {
            text: text.into(),
            x: x - size.left + left,
            y: y + size.ascent + top,
            bounds: Box2 {
                left: x - 2.,
                right: x + size.right - size.left + 2.,
                top: y - 2.,
                bottom: y + size.ascent + size.descent + 2.,
            }
            .translated(left, top),
        };
        let mut label = Label {
            name: (!name.is_empty()).then(|| make_line(name, ns, (width - nw) * align, 0.)),
            effort: (!effort.is_empty()).then(|| {
                make_line(
                    &effort,
                    es,
                    (width - ew) * align,
                    if name.is_empty() { 0. } else { nh + 4. },
                )
            }),
            bounds: Box2 {
                left: left - 2.,
                right: left + width + 2.,
                top: top - 2.,
                bottom: top + height + 2.,
            },
            cost: 0.,
        };
        let key = label
            .lines()
            .map(|line| format!("{:.2},{:.2}", line.x, line.y))
            .collect::<Vec<_>>()
            .join("/");
        if !seen.insert(key) {
            return;
        }
        let Some(primary) = label.name.as_ref().or(label.effort.as_ref()) else {
            return;
        };
        let distance = primary.bounds.distance(point.point);
        if distance > 26. {
            return;
        }
        let mut ambiguity = 0.;
        for other in points {
            if label
                .lines()
                .any(|line| line.bounds.distance(other.point) < 7.)
            {
                return;
            }
            if std::ptr::eq(other, point) {
                continue;
            }
            let od = primary.bounds.distance(other.point);
            if od + 3. < distance {
                return;
            }
            ambiguity += (distance + 2. - od).max(0.) * 4.;
        }
        label.cost = distance * 4.
            + label
                .effort
                .as_ref()
                .map(|line| line.bounds.distance(point.point) * 0.5)
                .unwrap_or(0.)
            + preference
            + ambiguity;
        result.push(label);
    };
    for offset in [0_f64, -6., 6., -12., 12.] {
        add(
            0.,
            point.point.x + 10.,
            point.point.y - first / 2. + offset,
            offset.abs() * 0.15,
        );
        add(
            1.,
            point.point.x - 10. - width,
            point.point.y - first / 2. + offset,
            0.5 + offset.abs() * 0.15,
        );
    }
    for align in [0.5, 0., 1.] {
        add(
            align,
            point.point.x - width * align,
            point.point.y + 10.,
            1.,
        );
        add(
            align,
            point.point.x - width * align,
            point.point.y - 10. - height,
            2.,
        );
    }
    result.sort_by(|a, b| a.cost.total_cmp(&b.cost));
    result
}
pub fn layout(points: &[LabelPoint], frame: Box2) -> HashMap<usize, Label> {
    let options: Vec<_> = points
        .iter()
        .map(|p| candidates(p, points, frame, false))
        .collect();
    let mut preferred: Vec<_> = (0..points.len()).collect();
    preferred.sort_by(|a, b| {
        points[*a]
            .priority
            .cmp(&points[*b].priority)
            .then(points[*a].point.y.total_cmp(&points[*b].point.y))
            .then(options[*b].len().cmp(&options[*a].len()))
    });
    let mut represented = HashSet::new();
    let mut ordered = Vec::new();
    let mut repeated = Vec::new();
    for i in preferred {
        if !options[i].is_empty() && represented.insert(points[i].family.clone()) {
            ordered.push(i);
        } else {
            repeated.push(i);
        }
    }
    ordered.extend(repeated);
    let mut placed = HashMap::new();
    for i in &ordered {
        if let Some(candidate) = options[*i]
            .iter()
            .find(|label| readable(label, &placed, frame))
        {
            placed.insert(*i, candidate.clone());
        }
    }
    for _ in 0..2 {
        let mut changed = false;
        for i in &ordered {
            let previous = placed.get(i);
            let mut best = None;
            let mut best_change = if previous.is_some() {
                -0.1
            } else {
                f64::INFINITY
            };
            for candidate in &options[*i] {
                let mut neighbors: Vec<_> = placed
                    .iter()
                    .filter(|(key, label)| {
                        **key != *i && candidate.reading_box().overlaps(label.reading_box())
                    })
                    .map(|(k, l)| (*k, l.clone()))
                    .collect();
                if neighbors.len() > 2 {
                    continue;
                }
                let mut trial = placed.clone();
                trial.remove(i);
                for (key, _) in &neighbors {
                    trial.remove(key);
                }
                if !density(candidate, &trial, frame) {
                    continue;
                }
                trial.insert(*i, candidate.clone());
                neighbors.sort_by_key(|(key, _)| options[*key].len());
                let mut cost_change = candidate.cost - previous.map(|l| l.cost).unwrap_or(0.);
                let mut complete = true;
                for (key, old) in neighbors {
                    if let Some(replacement) = options[key]
                        .iter()
                        .find(|label| readable(label, &trial, frame))
                    {
                        trial.insert(key, replacement.clone());
                        cost_change += replacement.cost - old.cost;
                    } else {
                        complete = false;
                        break;
                    }
                }
                if complete && cost_change < best_change {
                    best = Some(trial);
                    best_change = cost_change;
                }
            }
            if let Some(best) = best {
                placed = best;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    for i in ordered {
        if placed.contains_key(&i) || points[i].effort.is_empty() {
            continue;
        }
        if let Some(candidate) = candidates(&points[i], points, frame, true)
            .into_iter()
            .find(|label| readable(label, &placed, frame))
        {
            placed.insert(i, candidate);
        }
    }
    placed
}
