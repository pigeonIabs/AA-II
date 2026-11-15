use crate::*;
use crate::{charts::Plot, render::*, visual::btn};
use std::collections::HashSet;
fn curve_range(curve: &[Point], left: f64, right: f64) -> Vec<Point> {
    if curve.is_empty() {
        return vec![];
    }
    std::iter::once(Point {
        x: left,
        y: curve_at(curve, left),
    })
    .chain(curve.iter().filter(|p| p.x > left && p.x < right).copied())
    .chain(std::iter::once(Point {
        x: right,
        y: curve_at(curve, right),
    }))
    .collect()
}
fn tick(v: f64, metric: &str) -> String {
    if parameter_metric(metric) || metric == "tokens" {
        bar::chart_display(Some(v), metric)
    } else if metric == "cost" {
        if v.abs() >= 1000. {
            format!(
                "{}${}k",
                if v < 0. { "-" } else { "" },
                format_number(v.abs() / 1000., if v.abs() < 2000. { 2 } else { 1 })
            )
        } else {
            bar::chart_display(Some(v), metric)
        }
    } else {
        let (d, s) = if v.abs() >= 1e9 {
            (1e9, "B")
        } else if v.abs() >= 1e6 {
            (1e6, "M")
        } else if v.abs() >= 1e3 {
            (1e3, "k")
        } else {
            (1., "")
        };
        let value = v / d;
        format!(
            "{}{s}",
            format_number(
                value,
                (4. - value.abs().log10().floor()).clamp(0., 12.) as usize
            )
        )
    }
}
fn locations<'a>(
    mut models: Vec<&'a Value>,
    metric: &str,
    primary: &HashSet<&str>,
) -> Vec<Vec<&'a Value>> {
    models.sort_by(|a, b| {
        n(a, metric)
            .unwrap()
            .total_cmp(&n(b, metric).unwrap())
            .then(
                n(b, "intelligence")
                    .unwrap()
                    .total_cmp(&n(a, "intelligence").unwrap()),
            )
            .then(
                primary
                    .contains(s(b, "id"))
                    .cmp(&primary.contains(s(a, "id"))),
            )
            .then(visual::compare(s(a, "name"), s(b, "name")))
    });
    let mut groups: Vec<Vec<&Value>> = Vec::new();
    for m in models {
        if let Some(last) = groups.last_mut() {
            if n(last[0], metric) == n(m, metric)
                && n(last[0], "intelligence") == n(m, "intelligence")
            {
                last.push(m);
                continue;
            }
        }
        groups.push(vec![m]);
    }
    groups
}
pub fn render(a: &mut App) -> String {
    let metric = a.metric().to_string();
    let (dataset, frontier) = pareto(a.models(), &metric);
    let (selected, _) = pareto(a.visible(), &metric);
    let focused = dataset.iter().copied().find(|m| s(m, "id") == a.focus);
    let anchors = focused
        .map(|m| {
            if a.family_focus {
                families(dataset.iter().copied())
                    .into_iter()
                    .find(|f| f.0 == family_key(m))
                    .map(|f| f.2)
                    .unwrap_or_default()
            } else {
                vec![m]
            }
        })
        .unwrap_or_default();
    let dominators = s(&a.prefs, "paretoZone") == "dominators";
    let matches = dominance(&dataset, &anchors, &metric, dominators);
    let mut comparable = selected.clone();
    if focused.is_some() {
        comparable.extend(anchors.iter().chain(&matches).copied());
    }
    let mut seen = HashSet::new();
    comparable.retain(|m| seen.insert(s(m, "id")));
    let primary = seen.clone();
    let groups = locations(comparable.clone(), &metric, &primary);
    let (w, h) = visual::measured(a, "pareto", 1000., 520.);
    let left = if w < 500. { 48. } else { 62. };
    let frame = if focused.is_none() && groups.len() <= 5 {
        w.min(760_f64.max(h * if groups.len() <= 2 { 1.9 } else { 2.35 }))
    } else {
        w
    };
    let plot = Plot {
        x: left,
        y: 50.,
        width: (frame - left - 22.).max(1.),
        height: (h - 112.).max(1.),
        canvas_width: w,
        canvas_height: h,
    };
    let right = left + plot.width;
    let bottom = h - 62.;
    let xv = groups
        .iter()
        .map(|g| n(g[0], &metric).unwrap())
        .collect::<Vec<_>>();
    let yv = groups
        .iter()
        .map(|g| n(g[0], "intelligence").unwrap())
        .collect::<Vec<_>>();
    let reference = dataset
        .iter()
        .filter_map(|m| n(m, &metric))
        .collect::<Vec<_>>();
    let xa = Axis::new(
        &xv,
        &reference,
        b(&a.prefs, "paretoLogScale"),
        if w < 600. { 5 } else { 7 },
    );
    let ya = Axis::new(&yv, &yv, false, 6);
    let px = |v: f64| left + xa.pos(v) * plot.width;
    let py = |v: f64| plot.y + (1. - ya.pos(v)) * plot.height;
    let mut shown = comparable.clone();
    if !comparable.is_empty() {
        for m in &frontier {
            let x = xa.pos(n(m, &metric).unwrap());
            let y = ya.pos(n(m, "intelligence").unwrap());
            if (0. ..=1.).contains(&x) && (0. ..=1.).contains(&y) && seen.insert(s(m, "id")) {
                shown.push(m);
            }
        }
    }
    let shown_ids = shown
        .iter()
        .map(|m| s(m, "id").to_string())
        .collect::<Vec<_>>();
    let groups = locations(shown.clone(), &metric, &primary);
    let points = groups
        .iter()
        .map(|g| Point {
            x: px(n(g[0], &metric).unwrap()),
            y: py(n(g[0], "intelligence").unwrap()),
        })
        .collect::<Vec<_>>();
    let on_frontier = |m: &Value| frontier.iter().any(|f| s(f, "id") == s(m, "id"));
    let is_anchor = |g: &Vec<&Value>| {
        g.iter()
            .any(|m| anchors.iter().any(|f| s(f, "id") == s(m, "id")))
    };
    let mut content = String::new();
    let global = frontier
        .iter()
        .map(|m| Point {
            x: xa.pos(n(m, &metric).unwrap()),
            y: n(m, "intelligence").unwrap(),
        })
        .collect::<Vec<_>>();
    let line = |curve: &[Point]| {
        curve
            .iter()
            .enumerate()
            .map(|(i, p)| {
                format!(
                    "{}{},{}",
                    if i == 0 { "M" } else { "L" },
                    left + p.x * plot.width,
                    py(p.y)
                )
            })
            .collect::<Vec<_>>()
            .join(" ")
    };
    let boundary = if dominators {
        let mut out = anchors.clone();
        out.sort_by(|a, b| {
            n(b, &metric)
                .unwrap()
                .total_cmp(&n(a, &metric).unwrap())
                .then(
                    n(a, "intelligence")
                        .unwrap()
                        .total_cmp(&n(b, "intelligence").unwrap()),
                )
        });
        let mut best = f64::INFINITY;
        out.retain(|m| {
            let score = n(m, "intelligence").unwrap();
            if score < best {
                best = score;
                true
            } else {
                false
            }
        });
        out
    } else {
        pareto(anchors.iter().copied(), &metric).1
    };
    let zone = boundary
        .iter()
        .map(|m| Point {
            x: xa.pos(n(m, &metric).unwrap()),
            y: n(m, "intelligence").unwrap(),
        })
        .collect::<Vec<_>>();
    let capped = if a.family_focus && !dominators {
        cap_curve(&zone, &global, 1.)
    } else {
        vec![]
    };
    let ceiling = line(&curve_range(&global, 0., 1.));
    let cap = if ceiling.is_empty() {
        String::new()
    } else {
        format!("{ceiling} V{bottom} H{left} Z")
    };
    content.push_str(&format!("<defs><clipPath id=\"pareto-plot-clip\"><rect x=\"{left}\" y=\"50\" width=\"{}\" height=\"{}\"></rect></clipPath><clipPath id=\"pareto-frontier-cap\"><path d=\"{cap}\"></path></clipPath></defs><g aria-hidden=\"true\">",plot.width,plot.height));
    let mut zone_line = String::new();
    let mut family_lines = String::new();
    if let Some(m) = focused {
        let color = html(s(m, "color"));
        let dark = if bar::dark(s(m, "color")) {
            "pareto-dark-provider-fill"
        } else {
            ""
        };
        let stroke_dark = if bar::dark(s(m, "color")) {
            "pareto-dark-provider-stroke"
        } else {
            ""
        };
        if let (Some(first), Some(_last)) = (capped.first(), capped.last()) {
            let fill = format!(
                "{} V{bottom} H{} Z",
                line(&capped),
                left + first.x * plot.width
            );
            content.push_str(&format!("<g class=\"pareto-kill-zone\" clip-path=\"url(#pareto-plot-clip)\"><path class=\"{dark}\" clip-path=\"url(#pareto-frontier-cap)\" d=\"{fill}\" fill=\"{color}\" fill-opacity=\"0.13\"></path></g>"));
            let last = zone.last().unwrap().x;
            family_lines = format!(
                "<g clip-path=\"url(#pareto-frontier-cap)\"><path class=\"pareto-family-outline {stroke_dark}\" d=\"{}\" stroke=\"{color}\"></path><path class=\"pareto-kill-zone-boundary {stroke_dark}\" d=\"M{},{}V{} {}\" stroke=\"{color}\"></path></g>",
                line(
                    &capped
                        .iter()
                        .filter(|p| p.x <= last)
                        .copied()
                        .collect::<Vec<_>>()
                ),
                left + first.x * plot.width,
                bottom,
                py(first.y),
                line(
                    &capped
                        .iter()
                        .filter(|p| p.x >= last)
                        .copied()
                        .collect::<Vec<_>>()
                )
            );
        }
        if dominators && !zone.is_empty() {
            let first = zone[0];
            zone_line = format!(
                "M{},50V{}{}H{left}",
                left + first.x * plot.width,
                py(first.y),
                zone[1..]
                    .iter()
                    .map(|p| format!("H{}V{}", left + p.x * plot.width, py(p.y)))
                    .collect::<String>()
            );
        } else if !dominators && !a.family_focus {
            zone_line = format!(
                "M{},{}V{}H{right}",
                px(n(m, &metric).unwrap()),
                bottom,
                py(n(m, "intelligence").unwrap())
            );
        }
        if !zone_line.is_empty() {
            let fill = format!("{zone_line}V{}Z", if dominators { 50. } else { bottom });
            content.push_str(&format!("<g class=\"pareto-kill-zone\" clip-path=\"url(#pareto-plot-clip)\"><path class=\"{dark}\" d=\"{fill}\" fill=\"{color}\" fill-opacity=\"0.13\"></path></g>"));
            zone_line = format!(
                "<path class=\"pareto-kill-zone-boundary {stroke_dark}\" d=\"{zone_line}\" stroke=\"{color}\"></path>"
            );
        }
    }
    for v in &ya.ticks {
        let y = py(*v);
        content.push_str(&format!("<g><path d=\"M{left},{y}H{right}\" class=\"plot-grid-line\"></path><text class=\"axis-label\" x=\"{}\" y=\"{}\" text-anchor=\"end\">{}</text></g>",left-12.,y+3.,tick(*v,"")));
    }
    for v in &xa.ticks {
        let x = px(*v);
        content.push_str(&format!("<g><path d=\"M{x},50V{bottom}\" class=\"plot-grid-line\"></path><text class=\"axis-label\" x=\"{x}\" y=\"{}\" text-anchor=\"middle\">{}</text></g>",bottom+22.,tick(*v,&metric)));
    }
    let unit = if parameter_metric(&metric) {
        "billions"
    } else if metric == "cost" {
        "USD"
    } else if metric == "tokens" {
        "millions"
    } else {
        "seconds"
    };
    content.push_str(&format!("<path class=\"pareto-axes\" d=\"M{left},50V{bottom}H{right}\"></path><text class=\"axis-title\" transform=\"translate(-10, {}) rotate(-90)\" text-anchor=\"middle\">Intelligence Index</text><text class=\"axis-title\" x=\"{}\" y=\"{}\" text-anchor=\"middle\">{}<!-- --> · <!-- -->{unit}<!-- -->{}</text><g clip-path=\"url(#pareto-plot-clip)\">{}{family_lines}{zone_line}</g></g>",50.+plot.height/2.,left+plot.width/2.,h-2.,metric_label(&metric),if xa.threshold>0.{" · linear near zero"}else{""},if frontier.len()>1{format!("<path class=\"pareto-frontier-line\" d=\"{}\"></path>",line(&global))}else{String::new()}));
    let label_points = groups
        .iter()
        .zip(&points)
        .map(|(g, p)| {
            let m = g[0];
            let name = family_name(m);
            let suffix = if g.len() > 1 {
                format!(" +{}", g.len() - 1)
            } else {
                String::new()
            };
            let has = name != s(m, "name")
                || !m["effort"].is_null()
                || dataset
                    .iter()
                    .filter(|other| family_key(other) == family_key(m))
                    .count()
                    > 1;
            labels::LabelPoint {
                point: *p,
                family: family_key(m),
                name: name + &suffix,
                effort: if has {
                    effort(m).0.to_lowercase()
                } else {
                    String::new()
                },
                suffix,
                priority: if is_anchor(g) {
                    0
                } else if on_frontier(m) {
                    1
                } else {
                    2
                },
            }
        })
        .collect::<Vec<_>>();
    let layouts = labels::layout(
        &label_points,
        labels::Box2 {
            left,
            right: w - 8.,
            top: 32.,
            bottom: bottom - 6.,
        },
    );
    let mut order = (0..groups.len()).collect::<Vec<_>>();
    order.sort_by_key(|i| on_frontier(groups[*i][0]));
    for i in order {
        let g = &groups[i];
        let m = g[0];
        let point = points[i];
        let ids = g.iter().map(|m| s(m, "id")).collect::<Vec<_>>().join("|");
        let active = g
            .iter()
            .any(|m| a.hovered.split('|').any(|id| id == s(m, "id")));
        let anchor = is_anchor(g);
        content.push_str(&format!("<g class=\"model-point pareto-point {} {}\" data-location=\"{}\"><g class=\"pareto-dot-target\" data-hover=\"{}\" data-hover-x=\"{}\" data-hover-y=\"{}\" data-action=\"pareto-focus\" data-value=\"{}\" role=\"button\" tabindex=\"0\"><circle cx=\"{}\" cy=\"{}\" r=\"12\" fill=\"transparent\"></circle><circle class=\"pareto-marker\" cx=\"{}\" cy=\"{}\" r=\"{}\" fill=\"{}\" stroke=\"var(--chart-outline, {})\" stroke-width=\"1.5\"></circle></g>",if active{"active"}else{""},if anchor{"comparison-anchor"}else{""},html(&ids),html(&ids),point.x,point.y,html(s(m,"id")),point.x,point.y,point.x,point.y,if active{6.}else if anchor{5.5}else{4.5},html(s(m,"color")),if bar::dark(s(m,"color")){"#c1c9b8"}else{"#000"}));
        if let Some(layout) = layouts.get(&i) {
            for (l, kind, action) in [
                (layout.name.as_ref(), "family", "pareto-family"),
                (layout.effort.as_ref(), "effort", "pareto-focus"),
            ] {
                if let Some(l) = l {
                    content.push_str(&format!("<g class=\"pareto-label-target pareto-{kind}-target\" data-hover=\"{}\" data-hover-x=\"{}\" data-hover-y=\"{}\" data-action=\"{action}\" data-value=\"{}\" role=\"button\" tabindex=\"0\"><title>{} · Compare {}</title><rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"transparent\"></rect><text class=\"point-label pareto-{kind}-label\" x=\"{}\" y=\"{}\">{}</text></g>",html(&ids),point.x,point.y,html(s(m,"id")),html(if kind=="family"{&label_points[i].name}else{s(m,"name")}),if kind=="family"{"all measured reasoning efforts"}else{"this effort only"},l.bounds.left,l.bounds.top,l.bounds.right-l.bounds.left,l.bounds.bottom-l.bounds.top,l.x,l.y,html(&l.text)));
                }
            }
        }
        content.push_str("</g>");
    }
    let (count, leaders) = largest_kill(&dataset, &metric);
    let mut fs = families(dataset.iter().copied())
        .into_iter()
        .filter(|f| {
            f.2.iter()
                .map(|m| effort(m).0.to_lowercase())
                .collect::<HashSet<_>>()
                .len()
                > 1
        })
        .map(|(key, name, group)| {
            let count = dominance(&dataset, &group, &metric, false).len();
            (key, name, group, count)
        })
        .collect::<Vec<_>>();
    fs.sort_by(|a, b| {
        b.3.cmp(&a.3)
            .then(visual::compare(&a.1, &b.1))
            .then(visual::compare(&a.0, &b.0))
    });
    let stats = if dominators {
        String::new()
    } else {
        let leader = leaders.first();
        let single = leader
            .map(|m| {
                format!(
                    "{}<strong class=\"mono\">{count}<!-- --> <!-- -->{}</strong>{}",
                    btn(
                        "pareto-focus",
                        s(m, "id"),
                        &html(s(m, "name")),
                        "text-button",
                        "title=\"Show this model's kill zone across the full database\""
                    ),
                    if count == 1 { "model" } else { "models" },
                    if leaders.len() > 1 {
                        format!(
                            "<span title=\"{}\">· <!-- -->{}<!-- --> tied</span>",
                            html(
                                &leaders
                                    .iter()
                                    .map(|m| s(m, "name"))
                                    .collect::<Vec<_>>()
                                    .join("\n")
                            ),
                            leaders.len()
                        )
                    } else {
                        String::new()
                    }
                )
            })
            .unwrap_or_else(|| {
                format!(
                    "<span>{}</span>",
                    if dataset.is_empty() {
                        "No comparable models"
                    } else {
                        "No models dominated"
                    }
                )
            });
        let family = fs
            .first()
            .filter(|f| f.3 > 0)
            .map(|f| {
                let ties = fs.iter().filter(|e| e.3 == f.3).collect::<Vec<_>>();
                format!(
                    "{}<strong class=\"mono\">{}<!-- --> <!-- -->{}</strong>{}",
                    btn(
                        "pareto-family",
                        s(f.2[0], "id"),
                        &html(&f.1),
                        "text-button",
                        "title=\"Show the combined kill zone of all measured efforts\""
                    ),
                    f.3,
                    if f.3 == 1 { "model" } else { "models" },
                    if ties.len() > 1 {
                        format!(
                            "<span title=\"{}\">· <!-- -->{}<!-- --> tied</span>",
                            html(
                                &ties
                                    .iter()
                                    .map(|f| f.1.as_str())
                                    .collect::<Vec<_>>()
                                    .join("\n")
                            ),
                            ties.len()
                        )
                    } else {
                        String::new()
                    }
                )
            })
            .unwrap_or_else(|| {
                format!(
                    "<span>{}</span>",
                    if fs.is_empty() {
                        "No families with multiple measured efforts"
                    } else {
                        "No models dominated"
                    }
                )
            });
        format!(
            "<div class=\"pareto-kill-zone-stat\" aria-live=\"polite\"><span title=\"Largest single-model kill zone across the full eligible database for {}. Models must have a strictly lower score and higher resource use.\">Largest single-model kill zone</span>{single}</div><div class=\"pareto-kill-zone-stat\" aria-live=\"polite\"><span title=\"Combined kill zone across the full eligible database. Only families with at least two distinct measured efforts qualify. Each dominated model is counted once, excluding the family's own efforts.\">Largest family kill zone</span>{family}</div>",
            metric_label(&metric).to_lowercase()
        )
    };
    let mut caption = focused
        .map(|m| {
            format!(
                "{}{} · {} model{} {}",
                html(&if a.family_focus {
                    family_name(m)
                } else {
                    s(m, "name").into()
                }),
                if a.family_focus {
                    format!(
                        " · {} effort{}",
                        anchors.len(),
                        if anchors.len() == 1 { "" } else { "s" }
                    )
                } else {
                    String::new()
                },
                matches.len(),
                if matches.len() == 1 { "" } else { "s" },
                if dominators {
                    if matches.len() == 1 {
                        "dominates"
                    } else {
                        "dominate"
                    }
                } else {
                    "in kill zone"
                }
            )
        })
        .unwrap_or_else(|| {
            format!(
                "{} models shown · {} on full dataset frontier",
                shown.len(),
                frontier.len()
            )
        });
    if focused.is_some() && shown.len() > comparable.len() {
        caption += &format!(" · {} frontier models", shown.len() - comparable.len());
    }
    let graph = if !points.is_empty() {
        format!(
            "<svg id=\"graph-svg\" class=\"pareto-svg\" width=\"{w}\" height=\"{h}\" viewBox=\"0 0 {w} {h}\">{content}</svg>"
        )
    } else {
        format!(
            "<div class=\"empty-state\">Choose models with published <!-- -->{}<!-- --> measurements to draw the frontier.</div>",
            metric_label(&metric).to_lowercase()
        )
    };
    let draw_key = json!([
        metric,
        b(&a.prefs, "paretoLogScale"),
        focused.map(|m| s(m, "id")),
        focused.map(|_| if a.family_focus { "family" } else { "model" }),
        groups
            .iter()
            .map(|g| (n(g[0], &metric), n(g[0], "intelligence")))
            .collect::<Vec<_>>()
    ])
    .to_string();
    let reset_drawing = draw_key != a.pareto_draw_key;
    let fresh_drawing = drawing::Drawing::default();
    let drawing = if reset_drawing {
        &fresh_drawing
    } else {
        &a.drawing
    };
    let result = format!(
        "<section class=\"pareto-workspace\" aria-label=\"Pareto frontier\"><div class=\"metric-toolbar\" data-screenshot-exclude><div class=\"metric-tabs\" role=\"group\" aria-label=\"Horizontal axis\">{}</div><div class=\"pareto-options\"><div class=\"view-switch pareto-zone-switch\" role=\"group\" aria-label=\"Comparison region\">{}{}</div>{}{}</div></div>{stats}<div class=\"pareto-chart\"><div class=\"chart-caption\"><span aria-live=\"polite\">{caption}</span>{}<div class=\"pareto-drawing-tools-slot\" data-screenshot-exclude>{}</div></div><div class=\"pareto-viewport\" data-measure=\"pareto\">{graph}<div id=\"chart-tooltip\" class=\"pareto-tooltip\" data-screenshot-exclude hidden></div>{}</div>{}</div></section>",
        metric_tabs(a),
        btn(
            "zone",
            "kill",
            "Kill zone",
            if dominators { "" } else { "active" },
            &format!("aria-pressed=\"{}\"", !dominators)
        ),
        btn(
            "zone",
            "dominators",
            "Dominators",
            if dominators { "active" } else { "" },
            &format!("aria-pressed=\"{}\"", dominators)
        ),
        check(
            "preference",
            "paretoGlobalFrontier",
            "Full dataset frontier",
            b(&a.prefs, "paretoGlobalFrontier")
        ),
        check(
            "preference",
            "paretoLogScale",
            "Log scale",
            b(&a.prefs, "paretoLogScale")
        ),
        if focused.is_some() {
            btn(
                "clear-focus",
                "",
                "Clear comparison",
                "text-button",
                "data-screenshot-exclude",
            )
        } else {
            String::new()
        },
        if points.is_empty() {
            String::new()
        } else {
            drawing::toolbar(drawing)
        },
        if points.is_empty() {
            String::new()
        } else {
            drawing::overlay(drawing, plot)
        },
        if focused.is_none() && selected.len() < a.visible().len() {
            format!(
                "<p class=\"data-note\">{} of {} selected models have published {}.</p>",
                selected.len(),
                a.visible().len(),
                metric_label(&metric).to_lowercase()
            )
        } else {
            String::new()
        }
    );
    if reset_drawing {
        a.drawing = Default::default();
    }
    a.pareto_draw_key = draw_key;
    a.pareto_shown = shown_ids;
    a.plot = plot;
    result
}
struct Axis {
    min: f64,
    max: f64,
    threshold: f64,
    log: bool,
    ticks: Vec<f64>,
}
impl Axis {
    fn transform(&self, v: f64) -> f64 {
        if !self.log {
            v
        } else if self.threshold == 0. {
            v.log10()
        } else {
            let r = v.abs() / self.threshold;
            v.signum() * if r <= 1. { r } else { 1. + r.log10() }
        }
    }
    fn pos(&self, v: f64) -> f64 {
        (self.transform(v) - self.min) / (self.max - self.min)
    }
    fn inverse(&self, v: f64) -> f64 {
        if !self.log {
            v
        } else if self.threshold == 0. {
            10_f64.powf(v)
        } else {
            v.signum()
                * if v.abs() <= 1. {
                    v.abs() * self.threshold
                } else {
                    self.threshold * 10_f64.powf(v.abs() - 1.)
                }
        }
    }
    fn new(values: &[f64], reference: &[f64], log: bool, intervals: usize) -> Self {
        let smallest = reference
            .iter()
            .filter(|v| **v != 0.)
            .map(|v| v.abs())
            .reduce(f64::min)
            .unwrap_or(1.);
        let threshold = if log && reference.iter().any(|v| *v <= 0.) {
            10_f64.powf(smallest.log10().floor())
        } else {
            0.
        };
        let mut axis = Self {
            min: 0.,
            max: 1.,
            threshold,
            log,
            ticks: vec![],
        };
        if log && values.is_empty() {
            return axis;
        }
        let transformed = values
            .iter()
            .map(|v| axis.transform(*v))
            .collect::<Vec<_>>();
        let minimum = transformed.iter().copied().reduce(f64::min).unwrap_or(0.);
        let maximum = transformed.iter().copied().reduce(f64::max).unwrap_or(1.);
        let mut distinct = values.to_vec();
        distinct.sort_by(f64::total_cmp);
        distinct.dedup();
        let ratio = if distinct.len() <= 2 {
            0.12
        } else if values.len() <= 5 {
            0.07
        } else {
            0.03
        };
        let padding = if maximum != minimum {
            (maximum - minimum) * ratio
        } else if log {
            0.1
        } else {
            1_f64.max(maximum.abs() * 0.05)
        };
        axis.min = if minimum >= 0. && (!log || threshold > 0.) {
            0_f64.max(minimum - padding)
        } else {
            minimum - padding
        };
        axis.max = maximum + padding;
        let span = axis.max - axis.min;
        let local = nice_ticks(axis.inverse(axis.min), axis.inverse(axis.max), intervals);
        if !log {
            axis.ticks = local;
            return axis;
        }
        let multipliers: &[f64] = if span <= intervals as f64 / 3. {
            &[1., 2., 5.]
        } else if span <= intervals as f64 / 2. {
            &[1., 3.]
        } else {
            &[1.]
        };
        let stride = (span / intervals as f64).ceil().max(1.) as usize;
        let first = smallest.log10().floor() as i32;
        let last = values
            .iter()
            .filter(|v| **v != 0.)
            .map(|v| v.abs())
            .fold(1., f64::max)
            .log10()
            .ceil() as i32
            + 1;
        let mut decades = if threshold > 0. { vec![0.] } else { vec![] };
        for power in (first..=last).step_by(stride) {
            for m in multipliers {
                let tick = significant12(m * 10_f64.powi(power));
                if values.iter().any(|v| *v < 0.) {
                    decades.push(-tick);
                }
                decades.push(tick);
            }
        }
        let candidates = if span < 1.1 || decades.is_empty() {
            local
        } else {
            decades
        };
        axis.ticks = candidates
            .into_iter()
            .filter(|v| (threshold > 0. || *v > 0.) && axis.pos(*v) >= 0. && axis.pos(*v) <= 1.)
            .collect();
        axis
    }
}
fn significant12(v: f64) -> f64 {
    format!("{v:.11e}").parse().unwrap_or(v)
}
fn nice_ticks(min: f64, max: f64, intervals: usize) -> Vec<f64> {
    let rough = (max - min) / intervals.max(1) as f64;
    let step = if rough.is_finite() && rough > 0. {
        let power = 10_f64.powf(rough.log10().floor());
        [1., 2., 2.5, 5., 10.]
            .iter()
            .find(|v| **v * power >= rough)
            .unwrap_or(&10.)
            * power
    } else {
        1.
    };
    let first = ((min - step * 1e-10) / step).ceil();
    let last = ((max + step * 1e-10) / step).floor();
    let count = (last - first + 1.).max(0.) as usize;
    (0..count)
        .map(|i| significant12((first + i as f64) * step))
        .collect()
}
