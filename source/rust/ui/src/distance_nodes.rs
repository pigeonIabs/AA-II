use crate::*;

#[derive(Clone, Copy, PartialEq)]
struct PointState {
    highlighted: bool,
    opacity: u8,
    halo: bool,
}
struct PointNode {
    id: String,
    distance: f64,
    fill: Element,
    halo: Element,
    state: Option<PointState>,
}
#[derive(Clone, Copy, PartialEq)]
struct RowState {
    reached: bool,
    highlighted: bool,
    pinned: bool,
}
struct RowNode {
    id: String,
    distance: f64,
    rank: usize,
    row: Element,
    number: Element,
    state: Option<RowState>,
}
#[derive(Default)]
struct Scene {
    revision: String,
    points: Vec<PointNode>,
    rows: Vec<RowNode>,
}
thread_local! {static SCENE: RefCell<Scene> = RefCell::new(Scene::default());}
pub fn invalidate() {
    SCENE.with(|s| *s.borrow_mut() = Scene::default());
}

fn elements(selector: &str) -> Vec<Element> {
    document()
        .query_selector_all(selector)
        .ok()
        .map(|nodes| {
            (0..nodes.length())
                .filter_map(|i| nodes.item(i)?.dyn_into().ok())
                .collect()
        })
        .unwrap_or_default()
}
impl Scene {
    fn new(revision: String) -> Self {
        let points = elements(".model-point[data-point-distance]")
            .into_iter()
            .filter_map(|g| {
                Some(PointNode {
                    id: attribute(&g, "data-model-id"),
                    distance: attribute(&g, "data-point-distance").parse().ok()?,
                    fill: g.query_selector("[data-point-fill]").ok()??,
                    halo: g.query_selector("[data-point-halo]").ok()??,
                    state: None,
                })
            })
            .collect();
        let rows = elements("[data-rank-distance]")
            .into_iter()
            .filter_map(|row| {
                Some(RowNode {
                    id: attribute(&row, "data-value"),
                    distance: attribute(&row, "data-rank-distance").parse().ok()?,
                    rank: attribute(&row, "data-rank-number").parse().ok()?,
                    number: row.query_selector(".rank-number").ok()??,
                    row,
                    state: None,
                })
            })
            .collect();
        Self {
            revision,
            points,
            rows,
        }
    }
}

// Keep browser handles and previous visual states. Most sweep frames cross no
// model, so only the changing sphere needs DOM writes on those frames.
pub fn update(a: &App, radius: f64) {
    let revision = element("workspace")
        .map(|e| attribute(&e, "data-rust-retain"))
        .unwrap_or_default();
    SCENE.with(|cell| {
        let mut scene = cell.borrow_mut();
        if scene.revision != revision {
            *scene = Scene::new(revision);
        }
        let highlighted = |id: &str| distance::is_pinned(a, id) || a.hovered == id;
        let any = scene.points.iter().any(|p| highlighted(&p.id));
        for p in &mut scene.points {
            let active = highlighted(&p.id);
            let inside = p.distance <= radius;
            let next = PointState {
                highlighted: active,
                opacity: if active {
                    0
                } else if any {
                    1
                } else if a.scan_active && !inside {
                    2
                } else {
                    0
                },
                halo: active || inside && a.scan_active,
            };
            if p.state == Some(next) {
                continue;
            }
            if p.state.is_none_or(|old| old.highlighted != active) {
                let _ = p
                    .fill
                    .set_attribute("r", if active { "5.5" } else { "4.4" });
                let _ = p.halo.set_attribute("r", if active { "10" } else { "8" });
                let _ = p
                    .halo
                    .set_attribute("stroke-opacity", if active { "1" } else { "0.55" });
            }
            if p.state.is_none_or(|old| old.opacity != next.opacity) {
                let _ = p
                    .fill
                    .set_attribute("opacity", ["1", "0.35", "0.38"][next.opacity as usize]);
            }
            if p.state.is_none_or(|old| old.halo != next.halo) {
                let _ = p
                    .halo
                    .set_attribute("display", if next.halo { "inline" } else { "none" });
            }
            p.state = Some(next);
        }
        for r in &mut scene.rows {
            let next = RowState {
                reached: a.scan_active && r.distance <= radius,
                highlighted: a.hovered == r.id,
                pinned: distance::is_pinned(a, &r.id),
            };
            if r.state == Some(next) {
                continue;
            }
            if r.state.is_none_or(|old| old.reached != next.reached) {
                let _ = r
                    .row
                    .class_list()
                    .toggle_with_force("reached", next.reached);
                if next.reached {
                    dom::render(&r.number, &visual::icon("Check", 13, ""));
                } else {
                    r.number.set_text_content(Some(&format!("{:02}", r.rank)));
                }
            }
            if r.state
                .is_none_or(|old| old.highlighted != next.highlighted)
            {
                let _ = r
                    .row
                    .class_list()
                    .toggle_with_force("highlighted", next.highlighted);
            }
            if r.state.is_none_or(|old| old.pinned != next.pinned) {
                let _ = r
                    .row
                    .set_attribute("aria-pressed", if next.pinned { "true" } else { "false" });
            }
            r.state = Some(next);
        }
    });
}
