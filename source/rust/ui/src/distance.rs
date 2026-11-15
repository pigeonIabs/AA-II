use crate::*;
use crate::{
    charts::Plot,
    visual::{btn, icon},
};
use std::{f64::consts::PI, fmt::Write as _};
const TICKS: [f64; 5] = [0., 0.25, 0.5, 0.75, 1.];
#[derive(Clone, Copy)]
pub struct Projection {
    pub width: f64,
    pub height: f64,
    pub left: f64,
    pub top: f64,
    pub right: f64,
    pub bottom: f64,
    pub center: Point,
    pub unit: f64,
    pub three: bool,
    pub camera: Quaternion,
}
impl Projection {
    pub fn new(a: &App) -> Self {
        let (width, height) = visual::measured(a, "orbit", 635., 480.);
        let three = a.distance_metrics().len() == 3;
        let (left, right, top, bottom) = if three {
            (72., 72., 48., 54.)
        } else {
            (64., 28., 32., 58.)
        };
        let aw = (width - left - right).max(1.);
        let ah = (height - top - bottom).max(1.);
        let unit = if three {
            aw.min(ah) / browser_hypot3(100., 100., 100.)
        } else {
            (aw / 100.).min(ah / 100.)
        };
        Self {
            width,
            height,
            left,
            right,
            top,
            bottom,
            center: Point {
                x: left + aw / 2.,
                y: top + ah / 2.,
            },
            unit,
            three,
            camera: a.camera,
        }
    }
    pub fn extents(&self) -> [f64; 3] {
        [100., 100., if self.three { 100. } else { 0. }]
    }
    pub fn point(&self, position: [f64; 3]) -> (Point, f64) {
        let e = self.extents();
        let centered = std::array::from_fn(|i| position[i] - e[i] / 2.);
        let p = if self.three {
            self.camera.rotate(centered)
        } else {
            centered
        };
        (
            Point {
                x: self.center.x + p[0] * self.unit,
                y: self.center.y - p[1] * self.unit,
            },
            p[2],
        )
    }
    pub fn path(&self, points: &[[f64; 3]]) -> String {
        let mut path = String::with_capacity(points.len() * 48);
        for (i, point) in points.iter().enumerate() {
            let (p, _) = self.point(*point);
            let _ = write!(
                path,
                "{}{}{},{}",
                if i == 0 { "" } else { " " },
                if i == 0 { "M" } else { "L" },
                p.x,
                p.y
            );
        }
        path
    }
    pub fn line(&self, a: [f64; 3], b: [f64; 3], class: &str) -> String {
        format!(
            "<path d=\"{}\" class=\"{class}\"></path>",
            self.path(&[a, b])
        )
    }
    pub fn plot(&self) -> Plot {
        Plot {
            x: self.left,
            y: self.top,
            width: (self.width - self.left - self.right).max(1.),
            height: (self.height - self.top - self.bottom).max(1.),
            canvas_width: self.width,
            canvas_height: self.height,
        }
    }
}
pub fn axes(a: &App) -> Vec<String> {
    let metrics = a.distance_metrics();
    let mut axes = metrics
        .into_iter()
        .filter(|m| m != "intelligence")
        .collect::<Vec<_>>();
    axes.insert(1, "intelligence".into());
    axes
}
fn coordinates(model: &Value, a: &App) -> [f64; 3] {
    a.coordinates(model)
}
pub fn max_radius(a: &App) -> f64 {
    a.distance_radius()
}
fn surface(p: Projection, distance: f64, sweep: bool, color: &str) -> String {
    let ideal = p.point(p.extents()).0;
    if !p.three {
        return format!(
            "<circle cx=\"{}\" cy=\"{}\" r=\"{}\" fill=\"{}\" fill-opacity=\"0.045\" stroke=\"{}\" stroke-opacity=\"{}\"{}></circle>",
            ideal.x,
            ideal.y,
            distance * p.unit,
            if sweep { color } else { "none" },
            html(color),
            if sweep { 0.75 } else { 0.35 },
            if sweep {
                ""
            } else {
                " stroke-dasharray=\"4 4\""
            }
        );
    }
    let mut output = format!(
        "<g class=\"distance-sphere{}\" style=\"stroke:{}\">",
        if sweep { " sweep-sphere" } else { "" },
        html(color)
    );
    SURFACE_TRIG.with(|(angles, fixed_angles)| {
        let mut curve = |fixed: Point, polar_curve: bool| {
            let mut d = String::with_capacity(81 * 48);
            let mut connected = false;
            for angle in angles {
                let shortfall = if polar_curve {
                    [
                        distance * angle.x * fixed.y,
                        distance * angle.y,
                        distance * angle.x * fixed.x,
                    ]
                } else {
                    [
                        distance * fixed.x * angle.y,
                        distance * fixed.y,
                        distance * fixed.x * angle.x,
                    ]
                };
                if shortfall.iter().any(|v| *v > 100. + 1e-8) {
                    connected = false;
                    continue;
                }
                let point = p
                    .point(std::array::from_fn(|i| p.extents()[i] - shortfall[i]))
                    .0;
                let _ = write!(
                    d,
                    "{}{},{} ",
                    if connected { "L" } else { "M" },
                    point.x,
                    point.y
                );
                connected = true;
            }
            if !d.is_empty() {
                let _ = write!(output, "<path d=\"{d}\"></path>");
            }
        };
        for (index, fixed) in fixed_angles.iter().enumerate() {
            curve(*fixed, true);
            if index > 0 {
                curve(*fixed, false);
            }
        }
    });
    output.push_str("</g>");
    output
}
// The sphere topology is fixed. Cache its trigonometry, retaining the original
// floating-point operation order when applying each radius and projection.
thread_local! { static SURFACE_TRIG: ([Point; 81], [Point; 9]) = (
    std::array::from_fn(|step| { let angle = step as f64 / 80. * PI / 2.; Point { x: angle.sin(), y: angle.cos() } }),
    std::array::from_fn(|index| { let fixed = index as f64 / 8. * PI / 2.; Point { x: fixed.sin(), y: fixed.cos() } })
); }
pub fn sweep(a: &App, p: Projection) -> String {
    let radius = a.sweep * max_radius(a);
    if a.scan_active && radius > 0. {
        surface(p, radius, true, "var(--chart-highlight, #c8f59a)")
    } else {
        String::new()
    }
}
pub(super) fn is_pinned(a: &App, id: &str) -> bool {
    a.pin.split('|').any(|p| p == id)
}
fn highlights(a: &App, p: Projection) -> String {
    let models = a.ranked();
    models
        .iter()
        .filter(|(m, _)| is_pinned(a, s(m, "id")) || a.hovered == s(m, "id"))
        .map(|(m, d)| {
            let c = coordinates(m, a);
            let mut guides = surface(p, *d, false, s(m, "color"))
                + &p.line(p.extents(), c, "model-distance-line");
            if p.three {
                for axis in 0..3 {
                    let mut foot = c;
                    foot[axis] = 0.;
                    guides.push_str(&p.line(c, foot, "model-coordinate-guide"));
                }
            }
            format!("<g style=\"color:{}\">{guides}</g>", html(s(m, "color")))
        })
        .collect()
}
fn labels(a: &App, p: Projection) -> String {
    let models = a.ranked();
    let any = models
        .iter()
        .any(|(m, _)| is_pinned(a, s(m, "id")) || a.hovered == s(m, "id"));
    let mut points = models
        .iter()
        .enumerate()
        .filter(|(i, (m, _))| {
            is_pinned(a, s(m, "id")) || a.hovered == s(m, "id") || !any && *i == 0
        })
        .map(|(_, item)| (*item, p.point(coordinates(item.0, a)).0))
        .collect::<Vec<_>>();
    points.sort_by(|a, b| a.1.y.total_cmp(&b.1.y));
    let mut bounds: Vec<(f64, f64, f64, f64)> = Vec::new();
    let mut output = String::new();
    for ((m, d), point) in points {
        let highlighted = is_pinned(a, s(m, "id")) || a.hovered == s(m, "id");
        let suffix = if highlighted {
            format!(" · {}", format_number(d, 2))
        } else {
            String::new()
        };
        let original = s(m, "name");
        let mut name = original.to_string();
        let max = (p.width - 24.).max(60.);
        let measure = |text: &str| visual::width(text, 10.);
        while name.chars().count() > 1
            && measure(&format!(
                "{}{}{}",
                name,
                if name.len() < original.len() {
                    "…"
                } else {
                    ""
                },
                suffix
            )) > max
        {
            name.pop();
        }
        let text = format!(
            "{}{}{}",
            name,
            if name.len() < original.len() {
                "…"
            } else {
                ""
            },
            suffix
        );
        let tw = measure(&text);
        let end = point.x > p.center.x;
        let x = if end {
            (point.x - 13.).min(p.width - 12.).max(tw + 12.)
        } else {
            (point.x + 13.).min(p.width - tw - 12.).max(12.)
        };
        let left = if end { x - tw } else { x };
        let min_y = p.top + 12.;
        let max_y = (p.height - p.bottom - 8.).max(min_y);
        let preferred = (point.y - 13.).clamp(min_y, max_y);
        let mut y = preferred;
        for attempt in 0..=((max_y - min_y) / 18.).ceil() as usize * 2 {
            let candidate = preferred
                + (attempt as f64 / 2.).ceil() * if attempt % 2 == 1 { -18. } else { 18. };
            if candidate < min_y || candidate > max_y {
                continue;
            }
            if bounds.iter().any(|b| {
                left < b.1 + 6.
                    && left + tw > b.0 - 6.
                    && candidate - 11. < b.3 + 4.
                    && candidate + 3. > b.2 - 4.
            }) {
                continue;
            }
            y = candidate;
            break;
        }
        bounds.push((left, left + tw, y - 11., y + 3.));
        let displaced = (y - (point.y - 13.)).abs() > 1.
            || (x - (point.x + if end { -13. } else { 13. })).abs() > 1.;
        output.push_str(&format!(
            "<g>{}<text x=\"{x}\" y=\"{y}\" text-anchor=\"{}\" class=\"point-label\">{}</text></g>",
            if displaced {
                format!(
                    "<path d=\"M{},{} L{},{}\" class=\"point-label-leader\" stroke=\"{}\"></path>",
                    point.x,
                    point.y,
                    x + if end { 4. } else { -4. },
                    y - 3.,
                    html(s(m, "color"))
                )
            } else {
                String::new()
            },
            if end { "end" } else { "start" },
            html(&text)
        ));
    }
    output
}
pub fn svg(a: &App, p: Projection) -> String {
    let axes = axes(a);
    let extents = p.extents();
    let ideal = p.point(extents).0;
    let bottom_left = p.point([0.; 3]).0;
    let mut grid = String::new();
    if p.three {
        for fixed in 0..3 {
            let mut basis = [0.; 3];
            basis[fixed] = 1.;
            let far = if a.camera.rotate(basis)[2] >= 0. {
                0.
            } else {
                extents[fixed]
            };
            let moving = (0..3).filter(|v| *v != fixed).collect::<Vec<_>>();
            for axis in &moving {
                let other = *moving.iter().find(|v| *v != axis).unwrap();
                for fraction in TICKS {
                    let mut start = [0.; 3];
                    start[fixed] = far;
                    start[other] = extents[other] * fraction;
                    let mut end = start;
                    end[*axis] = extents[*axis];
                    grid.push_str(&p.line(start, end, "plot-grid-line"));
                }
            }
        }
    } else {
        for fraction in TICKS {
            grid.push_str(&p.line(
                [100. * fraction, 0., 0.],
                [100. * fraction, 100., 0.],
                "plot-grid-line",
            ));
            grid.push_str(&p.line(
                [0., 100. * fraction, 0.],
                [100., 100. * fraction, 0.],
                "plot-grid-line",
            ));
        }
    }
    let mut axis_output = String::new();
    for (axis, metric) in axes.iter().enumerate() {
        let others = (0..3).filter(|v| *v != axis).collect::<Vec<_>>();
        let mut candidates = Vec::new();
        for corner in 0..if p.three { 4 } else { 1 } {
            let mut start = [0.; 3];
            for (i, other) in others.iter().enumerate() {
                start[*other] = ((corner >> i) & 1) as f64 * extents[*other];
            }
            let mut end = start;
            end[axis] = extents[axis];
            let ap = p.point(start).0;
            let bp = p.point(end).0;
            let mid = Point {
                x: (ap.x + bp.x) / 2.,
                y: (ap.y + bp.y) / 2.,
            };
            let length = (bp.x - ap.x).hypot(bp.y - ap.y);
            let divisor = if length == 0. { 1. } else { length };
            let mut normal = Point {
                x: -(bp.y - ap.y) / divisor,
                y: (bp.x - ap.x) / divisor,
            };
            let side = (mid.x - p.center.x) * normal.x + (mid.y - p.center.y) * normal.y;
            if side < 0. {
                normal.x = -normal.x;
                normal.y = -normal.y;
            }
            let preference = match axis {
                0 => normal.y,
                1 => -normal.x,
                _ => normal.x,
            };
            let mut angle = (bp.y - ap.y).atan2(bp.x - ap.x) * 180. / PI;
            if angle > 90. {
                angle -= 180.;
            }
            if angle < -90. {
                angle += 180.;
            }
            candidates.push((
                side.abs() + preference * 12.,
                start,
                end,
                mid,
                normal,
                angle,
                length,
            ));
        }
        candidates.sort_by(|a, b| b.0.total_cmp(&a.0));
        let (_, start, end, mid, normal, angle, length) = candidates[0];
        axis_output.push_str(&format!("<g><g class=\"plot-axes\"><path d=\"{}\" marker-end=\"url(#distance-arrow)\"></path></g>",p.path(&[start,end])));
        let ticks = if length < 80. {
            vec![0., 1.]
        } else {
            TICKS.to_vec()
        };
        for fraction in ticks {
            let mut q = start;
            q[axis] = extents[axis] * fraction;
            let point = p.point(q).0;
            let short = extents[axis] * (1. - fraction);
            let normalized = 100. - short;
            let value = if metric == "intelligence" {
                100. - short
            } else {
                short / 100. * n(a.scales(), metric).unwrap_or(1.)
            };
            axis_output.push_str(&format!("<text x=\"{}\" y=\"{}\" text-anchor=\"{}\" class=\"axis-label\"><title>{}</title>{}</text>",point.x+normal.x*16.,point.y+normal.y*16.+4.,if normal.x>0.3{"start"}else if normal.x< -0.3{"end"}else{"middle"},html(&precision(Some(value),metric)),format_number(normalized,2)));
        }
        axis_output.push_str(&format!("<text transform=\"translate({}, {}) rotate({angle})\" text-anchor=\"middle\" dominant-baseline=\"middle\" class=\"axis-title\">{}<!-- --> · <!-- -->{}</text></g>",mid.x+normal.x*43.,mid.y+normal.y*43.,["X","Y","Z"][axis],metric_label(metric)));
    }
    let models = a.ranked();
    let any = models
        .iter()
        .any(|(m, _)| is_pinned(a, s(m, "id")) || a.hovered == s(m, "id"));
    let radius = a.sweep * max_radius(a);
    let mut points = models
        .iter()
        .map(|(m, d)| (*m, *d, p.point(coordinates(m, a))))
        .collect::<Vec<_>>();
    points.sort_by(|a, b| a.2.1.total_cmp(&b.2.1));
    points.sort_by_key(|(m, _, _)| {
        if a.hovered == s(m, "id") {
            2
        } else if is_pinned(a, s(m, "id")) {
            1
        } else {
            0
        }
    });
    let mut point_output = String::new();
    for (m, d, (point, _)) in points {
        let id = s(m, "id");
        let highlighted = is_pinned(a, id) || a.hovered == id;
        let inside = d <= radius;
        point_output.push_str(&format!("<g class=\"model-point\" data-model-id=\"{}\" data-action=\"pin\" data-value=\"{}\" data-hover=\"{}\" data-point-distance=\"{d}\" tabindex=\"0\" role=\"button\" aria-pressed=\"{}\"><circle cx=\"{}\" cy=\"{}\" r=\"10\" fill=\"transparent\"></circle><circle data-point-halo cx=\"{}\" cy=\"{}\" r=\"{}\" fill=\"none\" stroke=\"{}\" stroke-opacity=\"{}\" {}></circle><circle data-point-fill cx=\"{}\" cy=\"{}\" r=\"{}\" fill=\"{}\" stroke=\"var(--chart-outline, {})\" stroke-width=\"1.2\" opacity=\"{}\"></circle></g>",html(id),html(id),html(id),is_pinned(a,id),point.x,point.y,point.x,point.y,if highlighted{10.}else{8.},html(s(m,"color")),if highlighted{1.}else{0.55},if highlighted||inside&&a.scan_active{""}else{"display=\"none\""},point.x,point.y,if highlighted{5.5}else{4.4},html(s(m,"color")),if bar::dark(s(m,"color")){"#ddd"}else{"#000"},if highlighted{1.}else if any{0.35}else if a.scan_active&&!inside{0.38}else{1.}));
    }
    format!(
        "<svg id=\"graph-svg\" class=\"orbit-svg {} {}\" viewBox=\"0 0 {} {}\" role=\"group\" {}>{}<defs><clipPath id=\"distance-clip\"><rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\"></rect></clipPath><marker id=\"distance-arrow\" viewBox=\"0 0 6 6\" refX=\"5\" refY=\"3\" markerWidth=\"6\" markerHeight=\"6\" orient=\"auto-start-reverse\"><path d=\"M0,0 L6,3 L0,6\" fill=\"none\" stroke=\"var(--chart-axis, #718265)\" stroke-width=\"1\"></path></marker></defs><g aria-hidden=\"true\">{grid}</g><g clip-path=\"url(#distance-clip)\" pointer-events=\"none\"><g id=\"sweep-rings\">{}</g><g id=\"distance-highlights\">{}</g></g>{axis_output}<g id=\"distance-points\">{point_output}</g><g id=\"distance-labels\" pointer-events=\"none\">{}</g><g class=\"perfect-origin\" pointer-events=\"none\"><path d=\"M{},{}h14 M{},{}v14\" stroke=\"var(--chart-highlight, #d3f6a6)\" stroke-width=\"1.5\"></path><circle cx=\"{}\" cy=\"{}\" r=\"3\" fill=\"var(--chart-highlight, #d3f6a6)\"></circle><text x=\"{}\" y=\"{}\" text-anchor=\"{}\">PERFECT MODEL</text></g></svg>",
        if p.three { "orbit-3d" } else { "" },
        if a.drag
            .as_ref()
            .is_some_and(|d| d.mode == "camera" && d.moved)
        {
            "dragging"
        } else {
            ""
        },
        p.width,
        p.height,
        if p.three {
            "tabindex=\"0\" aria-describedby=\"distance-help\""
        } else {
            ""
        },
        if p.three {
            "<desc id=\"distance-help\">Drag in any direction to rotate the graph. Arrow keys rotate the view. Home resets it. Click points or press Enter while focused on them to toggle their distance highlights. Multiple models can stay highlighted.</desc>"
        } else {
            ""
        },
        if p.three { 0. } else { bottom_left.x - 1. },
        if p.three { 0. } else { ideal.y - 1. },
        if p.three { p.width } else { 100. * p.unit + 2. },
        if p.three {
            p.height
        } else {
            100. * p.unit + 2.
        },
        sweep(a, p),
        highlights(a, p),
        labels(a, p),
        ideal.x - 7.,
        ideal.y,
        ideal.x,
        ideal.y - 7.,
        ideal.x,
        ideal.y,
        ideal.x,
        ideal.y - 17.,
        if ideal.x > p.center.x { "end" } else { "start" }
    )
}
pub fn prepare(a: &mut App) {
    let ranked = a.ranked();
    let key = json!([
        a.scales()["cost"],
        a.scales()["tokens"],
        a.distance_metrics(),
        ranked
            .iter()
            .map(|(m, d)| (s(m, "id"), d))
            .collect::<Vec<_>>()
    ])
    .to_string();
    let ids = ranked
        .iter()
        .map(|(m, _)| s(m, "id").to_string())
        .collect::<HashSet<_>>();
    if key != a.distance_state_key {
        a.sweep = 0.;
        a.scan_active = false;
        a.playing = false;
        a.last_frame = 0.;
        a.hovered.clear();
        a.pin = a
            .pin
            .split('|')
            .filter(|id| ids.contains(*id))
            .collect::<Vec<_>>()
            .join("|");
        a.distance_state_key = key;
    }
    if ids.is_empty() {
        a.drawing = Default::default();
    }
}
pub fn render(a: &mut App) -> String {
    let ranked = a.ranked();
    let metrics = a.distance_metrics();
    let p = Projection::new(a);
    let max = max_radius(a);
    let radius = a.sweep * max;
    let (grid_width, grid_height) = a.sizes.get("distance-grid").copied().unwrap_or_default();
    let (rw, rh) = a.sizes.get("ranking").copied().unwrap_or_default();
    let columns = 3_usize
        .min(((rw + 20.) / 340.).floor().max(1.) as usize)
        .min(ranked.len().max(1));
    let rows = 1_usize.max(
        ((rh - 34.) / 68.)
            .floor()
            .min((ranked.len() as f64 / columns as f64).ceil()) as usize,
    );
    let page = columns * rows;
    let plot_width = if grid_width != 0. && grid_height != 0. {
        format!(
            "--distance-plot-width:{}px;",
            0_f64.max(
                (grid_height - 34.)
                    .min(grid_width * 0.62)
                    .min(grid_width - 300.)
            )
        )
    } else {
        String::new()
    };
    let mut ranking = String::new();
    for column in 0..columns {
        ranking.push_str(&format!("<div class=\"ranking-columns\" style=\"grid-column:{};grid-row:1\" aria-hidden=\"true\"><span>MODEL</span><span>DISTANCE {}</span></div>",column+1,icon("ArrowDown",12,"")));
    }
    for (i, (m, d)) in ranked.iter().enumerate() {
        let id = s(m, "id");
        let reached = a.scan_active && *d <= radius;
        ranking.push_str(&format!("<div class=\"ranking-entry\" data-hover=\"{}\" data-tooltip=\"ranking\" style=\"grid-column:{};grid-row:{}\">{}{}</div>",html(id),(i%page)/rows+1,(i/page)*rows+i%rows+2,btn("pin",id,&format!("<span class=\"rank-number mono\">{}</span><span class=\"ranking-name\"><span class=\"ranking-model-title\">{}<span>{}</span></span><span class=\"ranking-detail\"><span style=\"background:{}\" class=\"swatch\"></span>{}</span></span><span class=\"rank-distance mono\" title=\"{d}\">{}</span>",if reached{icon("Check",13,"")}else{format!("{:02}",i+1)},render::logo(m,16),html(s(m,"name")),html(s(m,"color")),html(s(m,"provider")),format_number(*d,2)),&format!("ranking-row {} {}",if a.hovered==id{"highlighted"}else{""},if reached{"reached"}else{""}),&format!("aria-pressed=\"{}\" title=\"{} distance for {}\" data-rank-distance=\"{d}\" data-rank-number=\"{}\"",is_pinned(a,id),if is_pinned(a,id){"Hide"}else{"Show"},html(s(m,"name")),i+1)),btn("detail",id,&icon("ArrowUpRight",13,""),"ranking-inspect",&format!("data-screenshot-exclude aria-label=\"Open details for {}\" title=\"Open details for {}\"",html(s(m,"name")),html(s(m,"name"))))));
    }
    if ranked.is_empty() {
        ranking.push_str("<div class=\"empty-state\">No selected models have measurements within the normalization range.</div>");
    }
    let status = status(a, &ranked, radius);
    let controls=format!("<div class=\"perfection-intro\" data-screenshot-exclude><fieldset class=\"distance-metric-picker\"><legend>Add cost, tokens, or both</legend>{}</fieldset><div class=\"perfection-sweep\"><div class=\"sweep-controls\">{}<div class=\"sweep-slider\"><div><label for=\"radius\">Expand from perfection</label><output id=\"sweep-value\" class=\"mono\">{}</output></div><input id=\"radius\" type=\"range\" min=\"0\" max=\"{max}\" step=\"0.1\" value=\"{radius}\" data-field=\"sweep\" aria-label=\"Distance {} radius\"></div>{}</div><div class=\"sweep-status\" id=\"sweep-status\">{status}</div></div><div class=\"perfection-drawing-tools\">{}</div>{}</div>",["cost","tokens"].iter().map(|metric|{let checked=metrics.iter().any(|m|m==metric);format!("<label class=\"checkbox-label\"><input type=\"checkbox\" data-field=\"distance-metric\" value=\"{metric}\" {} {}>{}</label>",if checked{"checked"}else{""},if metrics.len()==2&&checked{"disabled"}else{""},metric_label(metric))}).collect::<String>(),btn("play","",&icon(if a.playing{"Pause"}else{"Play"},15,""),&format!("sweep-play {}",if a.playing{"playing"}else{""}),&format!("{} aria-label=\"{} radius sweep\"",if ranked.is_empty(){"disabled"}else{""},if a.playing{"Pause"}else{"Play"})),format_number(radius,1),if p.three{"sphere"}else{"circle"},btn("sweep-reset","",&icon("RotateCcw",16,""),"icon-button","aria-label=\"Reset radius sweep\""),if ranked.is_empty(){String::new()}else{drawing::toolbar(&a.drawing)},btn("method","",&(icon("SlidersHorizontal",15,"")+"Distance settings"),"text-button",""));
    let orbit = format!(
        "<div class=\"orbit-heading\"><span>{}{}<!-- --> normalized distance map</span><div class=\"plot-view-controls\" {}>{}</div></div><div class=\"orbit-viewport\" data-measure=\"orbit\">{}{}</div><div class=\"orbit-tooltip\" id=\"chart-tooltip\" data-screenshot-exclude hidden></div>",
        icon("Crosshair", 14, ""),
        if p.three { "3D" } else { "2D" },
        if p.three {
            "data-screenshot-exclude"
        } else {
            ""
        },
        if p.three {
            format!(
                "<span class=\"plot-drag-hint\">Drag to rotate</span>{}",
                btn(
                    "camera-reset",
                    "",
                    &icon("RotateCcw", 14, ""),
                    "icon-button",
                    "aria-label=\"Reset 3D view\" title=\"Reset 3D view\""
                )
            )
        } else {
            format!(
                "<span class=\"chart-direction\">Better{}</span>",
                icon("ArrowUpRight", 14, "")
            )
        },
        svg(a, p),
        if ranked.is_empty() {
            String::new()
        } else {
            drawing::overlay(&a.drawing, p.plot())
        }
    );
    let result = format!(
        "<section class=\"perfection-workspace\" style=\"{plot_width}--ranking-column-count:{columns};--ranking-row-height:68px;--ranking-header-height:34px;--ranking-column-gap:20px\">{controls}<div class=\"perfection-grid\" data-measure=\"distance-grid\"><div class=\"orbit-panel\" id=\"orbit-panel\">{orbit}</div><div class=\"distance-ranking\"><div class=\"ranking-heading\"><h3>Closest to perfection</h3><span class=\"muted\">{}<!-- --> models</span></div><div class=\"ranking-list\" data-measure=\"ranking\" data-scroll=\"ranking\" role=\"region\" aria-label=\"Models ranked by distance to perfection\" tabindex=\"0\">{ranking}</div></div></div>{}</section>",
        ranked.len(),
        if ranked.len() < a.visible().len() {
            format!(
                "<p class=\"data-note\">{} of {} selected models have published measurements within the normalization range.</p>",
                ranked.len(),
                a.visible().len()
            )
        } else {
            String::new()
        }
    );
    a.plot = p.plot();
    result
}
fn status(a: &App, models: &[(&Value, f64)], radius: f64) -> String {
    if !a.scan_active {
        return String::new();
    }
    let reached = models.iter().filter(|(_, d)| *d <= radius).count();
    let next = models.iter().find(|(_, d)| *d > radius);
    format!(
        "<span><span class=\"status-square active\"></span>{reached}<!-- --> of <!-- -->{}<!-- --> models reached</span>{}",
        models.len(),
        next.map(|(m, _)| format!(
            "<span title=\"{}\">Next · <!-- -->{}</span>",
            html(s(m, "name")),
            html(s(m, "name"))
        ))
        .unwrap_or_default()
    )
}
pub fn update_sweep() {
    APP.with(|c| {
        let a = c.borrow();
        let p = Projection::new(&a);
        let max = max_radius(&a);
        let radius = a.sweep * max;
        if let Some(g) = element("sweep-rings") {
            dom::render(&g, &sweep(&a, p));
        }
        if let Some(value) = element("sweep-value") {
            if let Some(text) = value.first_child() {
                text.set_node_value(Some(&format_number(radius, 1)));
            } else {
                value.set_text_content(Some(&format_number(radius, 1)));
            }
        }
        if let Some(input) = element("radius").and_then(|e| e.dyn_into::<HtmlInputElement>().ok()) {
            input.set_value(&radius.to_string());
        }
        let ranked = a.ranked();
        if let Some(status_element) = element("sweep-status") {
            dom::render(&status_element, &status(&a, &ranked, radius));
        }
        if let Ok(Some(play)) = document().query_selector("[data-action=play]") {
            dom::render(
                &play,
                &icon(if a.playing { "Pause" } else { "Play" }, 15, ""),
            );
            let _ = play.class_list().toggle_with_force("playing", a.playing);
        }
        update_points(&a, radius);
    });
}
fn update_points(a: &App, radius: f64) {
    distance_nodes::update(a, radius);
}
fn order_points(a: &App, p: Projection) {
    let Some(parent) = element("distance-points") else {
        return;
    };
    let ranked = a.ranked();
    let positions = ranked
        .iter()
        .enumerate()
        .map(|(i, (m, _))| (s(m, "id"), (i, p.point(coordinates(m, a)).1)))
        .collect::<HashMap<_, _>>();
    let nodes = parent.child_nodes();
    let mut ordered = (0..nodes.length())
        .filter_map(|i| nodes.item(i).and_then(|n| n.dyn_into::<Element>().ok()))
        .map(|e| {
            let id = attribute(&e, "data-model-id");
            let (rank, depth) = positions.get(id.as_str()).copied().unwrap_or_default();
            let priority = if id == a.hovered {
                2
            } else if is_pinned(a, &id) {
                1
            } else {
                0
            };
            (e, priority, depth, rank)
        })
        .collect::<Vec<_>>();
    ordered.sort_by(|a, b| a.1.cmp(&b.1).then(a.2.total_cmp(&b.2)).then(a.3.cmp(&b.3)));
    for (i, (e, _, _, _)) in ordered.iter().enumerate() {
        let current = parent.child_nodes().item(i as u32);
        if current.as_ref() != Some(e.as_ref()) {
            let _ = parent.insert_before(e, current.as_ref());
        }
    }
}
pub fn update_hover() {
    APP.with(|c| {
        let a = c.borrow();
        let p = Projection::new(&a);
        if let Some(g) = element("distance-highlights") {
            dom::render(&g, &highlights(&a, p));
        }
        if let Some(g) = element("distance-labels") {
            dom::render(&g, &labels(&a, p));
        }
        update_points(&a, a.sweep * max_radius(&a));
        order_points(&a, p);
    });
}
