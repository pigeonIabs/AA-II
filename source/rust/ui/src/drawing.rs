use crate::charts::Plot;
use crate::*;
use std::fmt::Write as _;
#[derive(Clone, PartialEq)]
pub struct Annotation {
    pub kind: String,
    pub color: String,
    pub points: Vec<Point>,
    pub flipped: bool,
    pub text: String,
}
impl Annotation {
    fn end(&self) -> Point {
        *self.points.last().unwrap_or(&Point::default())
    }
    fn start(&self) -> Point {
        self.points.first().copied().unwrap_or_default()
    }
}
#[derive(Clone, PartialEq)]
pub struct Drawing {
    pub tool: String,
    pub color: String,
    pub annotations: Vec<Annotation>,
    pub selected: Option<usize>,
    pub pending: Option<Annotation>,
    pub editing: Option<usize>,
}
impl Default for Drawing {
    fn default() -> Self {
        Self {
            tool: String::new(),
            color: "#ff0000".into(),
            annotations: Vec::new(),
            selected: None,
            pending: None,
            editing: None,
        }
    }
}
pub fn toolbar(d: &Drawing) -> String {
    use crate::visual::{btn, icon};
    let mut content = String::new();
    for (tool, label, name, size, title) in [
        (
            "select",
            "Select",
            "MousePointer2",
            14,
            "Select, move, rotate, or resize placed annotations",
        ),
        ("pen", "Pen", "Pencil", 14, "Draw with the smoothed pen"),
        (
            "oval",
            "Oval",
            "",
            16,
            "Click the two opposite tips of the oval at any angle",
        ),
        (
            "arrow",
            "Arrow",
            "ArrowUpRight",
            15,
            "Click the arrow's start, then its tip",
        ),
        (
            "brace",
            "Brace",
            "Braces",
            14,
            "Click two points to place a curly brace",
        ),
    ] {
        let image = if tool == "oval" {
            r#"<svg width="16" height="14" viewBox="0 0 16 14" aria-hidden="true"><ellipse cx="8" cy="7" rx="6" ry="4" fill="none" stroke="currentColor" stroke-width="1.5"></ellipse></svg>"#.into()
        } else {
            icon(name, size, "")
        };
        content += &btn(
            "tool",
            tool,
            &format!("{image}<span>{label}</span>"),
            "chart-tool-button",
            &format!(
                "type=\"button\" aria-pressed=\"{}\" title=\"{title}\"",
                d.tool == tool
            ),
        );
    }
    if !d.tool.is_empty() && d.tool != "select" {
        content += &format!(
            r#"<input type="color" class="chart-pen-color" aria-label="Drawing color" title="Drawing color" data-field="drawing-color" value="{}">"#,
            html(&d.color)
        );
    }
    if d.selected.is_some() {
        content += &btn(
            "delete-drawing",
            "",
            &(icon("Trash2", 14, "") + "<span>Delete</span>"),
            "chart-tool-button",
            r#"type="button" aria-label="Delete selected annotation" title="Delete selected annotation""#,
        );
    }
    let disabled = if d.annotations.is_empty() && d.pending.is_none() {
        "disabled"
    } else {
        ""
    };
    content += &btn(
        "undo",
        "",
        &icon("Undo2", 15, ""),
        "",
        &format!(
            r#"type="button" aria-label="Undo last drawing" title="Undo last drawing" {disabled}"#
        ),
    );
    content += &btn(
        "clear-drawing",
        "",
        &(icon("Trash2", 14, "") + "<span>Clear</span>"),
        "",
        &format!(
            r#"type="button" aria-label="Clear drawing" title="Clear all drawings" {disabled}"#
        ),
    );
    format!(
        r#"<div class="chart-drawing-tools" role="group" aria-label="Chart drawing tools" data-screenshot-exclude>{content}</div>"#
    )
}
fn label_geometry(a: &Annotation, p: Plot) -> (Point, &'static str, &'static str, &'static str) {
    let (_, tip, nx, ny) = brace(a, p);
    let mut point = Point {
        x: tip.x + nx * 8.,
        y: tip.y + ny * 8.,
    };
    let (anchor, base, transform) = if nx < -0.3 {
        ("end", "central", "translate(-100%, -50%)")
    } else if nx > 0.3 {
        ("start", "central", "translate(0, -50%)")
    } else if ny < 0. {
        point.y -= 4.;
        ("middle", "auto", "translate(-50%, -100%)")
    } else {
        point.y += 4.;
        ("middle", "hanging", "translate(-50%, 0%)")
    };
    (point, anchor, base, transform)
}
pub fn overlay(d: &Drawing, p: Plot) -> String {
    let editor=d.editing.and_then(|i|d.annotations.get(i)).map(|a|{let(point,_,_,transform)=label_geometry(a,p);format!(r#"<div class="chart-brace-input-box" data-screenshot-exclude style="left:{}px;top:{}px;transform:{transform}"><input id="brace-text" type="text" class="chart-brace-input" data-field="brace-text" value="{}" placeholder="Label at tip..." aria-label="Brace label" autofocus>{}</div>"#,point.x,point.y,html(&a.text),visual::btn("flip-brace","",&visual::icon("RefreshCw",12,""),"chart-brace-flip-btn",r#"type="button" title="Flip brace direction" aria-label="Flip brace direction""#))}).unwrap_or_default();
    format!(
        r#"<div class="chart-drawing" style="width:{}px;height:{}px"><svg id="annotation-svg" class="chart-drawing-surface{}{}" width="{}" height="{}" viewBox="0 0 {} {}" role="img" aria-label="Chart annotations" tabindex="{}"><g id="annotations">{}</g></svg>{editor}</div>"#,
        p.canvas_width,
        p.canvas_height,
        if d.tool.is_empty() { "" } else { " active" },
        if d.tool == "select" {
            " select-mode"
        } else {
            ""
        },
        p.canvas_width,
        p.canvas_height,
        p.canvas_width,
        p.canvas_height,
        if d.tool.is_empty() { -1 } else { 0 },
        graphics(d, p)
    )
}
fn smooth(points: &[Point]) -> String {
    if points.is_empty() {
        return String::new();
    }
    let mut path = format!("M{},{}", points[0].x, points[0].y);
    for i in 1..points.len().saturating_sub(1) {
        let p = points[i];
        let next = points[i + 1];
        let _ = write!(
            path,
            "Q{},{} {},{}",
            p.x,
            p.y,
            (p.x + next.x) / 2.,
            (p.y + next.y) / 2.
        );
    }
    if let Some(last) = points.last() {
        let _ = write!(path, "L{},{}", last.x, last.y);
    }
    path
}
fn brace(a: &Annotation, plot: Plot) -> (String, Point, f64, f64) {
    let start = plot.pixel(a.start());
    let end = plot.pixel(a.end());
    let dx = end.x - start.x;
    let dy = end.y - start.y;
    let length = dx.hypot(dy).max(1.);
    let (ux, uy) = (dx / length, dy / length);
    let sign = if a.flipped { -1. } else { 1. };
    let (nx, ny) = (-uy * sign, ux * sign);
    let depth = (length * 0.4).min(8. + length * 0.08).clamp(10., 26.);
    let q = 18_f64.min(length / 4.);
    let (qk, wk) = (q * 0.55228, depth / 2. * 0.55228);
    let pt = |s: f64, t: f64| Point {
        x: start.x + ux * s + nx * t,
        y: start.y + uy * s + ny * t,
    };
    let f = |p: Point| format!("{},{}", p.x, p.y);
    let tip = pt(length / 2., depth);
    let path = format!(
        "M{} C{} {} {} L{} C{} {} {} C{} {} {} L{} C{} {} {}",
        f(start),
        f(pt(0., wk)),
        f(pt(q - qk, depth / 2.)),
        f(pt(q, depth / 2.)),
        f(pt(length / 2. - q, depth / 2.)),
        f(pt(length / 2. - q + qk, depth / 2.)),
        f(pt(length / 2., depth - wk)),
        f(tip),
        f(pt(length / 2., depth - wk)),
        f(pt(length / 2. + q - qk, depth / 2.)),
        f(pt(length / 2. + q, depth / 2.)),
        f(pt(length - q, depth / 2.)),
        f(pt(length - q + qk, depth / 2.)),
        f(pt(length, wk)),
        f(end)
    );
    (path, tip, nx, ny)
}
fn graphic(a: &Annotation, plot: Plot) -> String {
    let start = plot.pixel(a.start());
    let end = plot.pixel(a.end());
    let color = html(&a.color);
    let length = (end.x - start.x).hypot(end.y - start.y);
    match a.kind.as_str() {
        "pen" => {
            if a.points.len() == 1 {
                format!(
                    r#"<circle cx="{}" cy="{}" r="3" fill="{color}" pointer-events="none"></circle>"#,
                    start.x, start.y
                )
            } else {
                format!(
                    r#"<path d="{}" fill="none" stroke="{color}" stroke-width="6" stroke-linecap="round" stroke-linejoin="round" pointer-events="none"></path>"#,
                    smooth(&a.points.iter().map(|p| plot.pixel(*p)).collect::<Vec<_>>())
                )
            }
        }
        "brace" => {
            if length < 1. {
                return String::new();
            }
            let (path, _, _, _) = brace(a, plot);
            let (point, anchor, base, _) = label_geometry(a, plot);
            format!(
                r#"<path d="{path}" fill="none" stroke="{color}" stroke-width="3" stroke-linecap="round" stroke-linejoin="round" pointer-events="none"></path>{}"#,
                if a.text.is_empty() {
                    String::new()
                } else {
                    format!(
                        r#"<text class="chart-brace-label" x="{}" y="{}" text-anchor="{anchor}" dominant-baseline="{base}" fill="{color}" pointer-events="auto">{}</text>"#,
                        point.x,
                        point.y,
                        html(&a.text)
                    )
                }
            )
        }
        "oval" => {
            if length < 1. {
                return String::new();
            }
            let angle = (end.y - start.y).atan2(end.x - start.x).to_degrees();
            format!(
                r#"<path d="M{},{} A{},{} {angle} 0 1 {},{} A{},{} {angle} 0 1 {},{} Z" fill="none" stroke="{color}" stroke-width="6" stroke-linejoin="round" pointer-events="none"></path>"#,
                start.x,
                start.y,
                length / 2.,
                length / 6.,
                end.x,
                end.y,
                length / 2.,
                length / 6.,
                start.x,
                start.y
            )
        }
        _ => {
            if length < 1. {
                return String::new();
            }
            let dx = (end.x - start.x) / length;
            let dy = (end.y - start.y) / length;
            let head = 24_f64.min(length * 0.45);
            let half = head * 0.55;
            let bx = end.x - dx * head;
            let by = end.y - dy * head;
            format!(
                r#"<path d="M{},{} L{},{}" fill="none" stroke="{color}" stroke-width="{}" stroke-linecap="round" pointer-events="none"></path><path d="M{},{} L{},{} L{},{} Z" fill="{color}" pointer-events="none"></path>"#,
                start.x,
                start.y,
                end.x - dx * head * 0.8,
                end.y - dy * head * 0.8,
                6_f64.min(length / 4.),
                end.x,
                end.y,
                bx - dy * half,
                by + dx * half,
                bx + dy * half,
                by - dx * half
            )
        }
    }
}
fn bounds(a: &Annotation, plot: Plot) -> (f64, f64, f64, f64) {
    let mut points: Vec<_> = a.points.iter().map(|p| plot.pixel(*p)).collect();
    if a.kind == "brace" {
        points.push(brace(a, plot).1);
    }
    if a.kind == "oval" {
        let start = plot.pixel(a.start());
        let end = plot.pixel(a.end());
        let len = (end.x - start.x).hypot(end.y - start.y).max(1.);
        let nx = -(end.y - start.y) / len;
        let ny = (end.x - start.x) / len;
        let center = Point {
            x: (start.x + end.x) / 2.,
            y: (start.y + end.y) / 2.,
        };
        for sign in [-1., 1.] {
            points.push(Point {
                x: center.x + nx * len / 6. * sign,
                y: center.y + ny * len / 6. * sign,
            });
        }
    }
    let minx = points.iter().map(|p| p.x).fold(f64::INFINITY, f64::min);
    let miny = points.iter().map(|p| p.y).fold(f64::INFINITY, f64::min);
    let maxx = points.iter().map(|p| p.x).fold(f64::NEG_INFINITY, f64::max);
    let maxy = points.iter().map(|p| p.y).fold(f64::NEG_INFINITY, f64::max);
    (minx, miny, maxx, maxy)
}
pub fn graphics(d: &Drawing, p: Plot) -> String {
    let mut result = String::new();
    for (i, a) in d.annotations.iter().enumerate() {
        let mut copy = a.clone();
        if d.editing == Some(i) {
            copy.text.clear();
        }
        let mut markup = graphic(&copy, p);
        if a.kind == "brace" && d.tool != "select" {
            markup = markup.replace(
                "<text class=",
                &format!(r#"<text data-action="edit-brace" data-value="{i}" class="#),
            );
        }
        result += &format!(r#"<g data-annotation="{i}">{markup}"#);
        if d.tool == "select" {
            let hit = graphic(a, p)
                .replace("pointer-events=\"none\"", "pointer-events=\"stroke\"")
                .replace("stroke-width=\"6\"", "stroke-width=\"22\"")
                .replace("stroke-width=\"3\"", "stroke-width=\"22\"")
                .replace(
                    &format!("stroke=\"{}\"", html(&a.color)),
                    "stroke=\"transparent\"",
                )
                .replace(
                    &format!("fill=\"{}\"", html(&a.color)),
                    "fill=\"transparent\"",
                );
            let hit = if a.kind == "pen" && a.points.len() == 1 {
                let start = p.pixel(a.start());
                format!(
                    r#"<path d="M{},{} h0.01" fill="none" stroke="transparent" stroke-width="22" stroke-linecap="round" stroke-linejoin="round" pointer-events="stroke" cursor="move"></path>"#,
                    start.x, start.y
                )
            } else if a.kind == "arrow" {
                let start = p.pixel(a.start());
                let end = p.pixel(a.end());
                format!(
                    r#"<line x1="{}" y1="{}" x2="{}" y2="{}" stroke="transparent" stroke-width="22" stroke-linecap="round" pointer-events="stroke" cursor="move"></line>"#,
                    start.x, start.y, end.x, end.y
                )
            } else {
                hit
            };
            let hit = if a.kind == "oval" {
                hit.replace("pointer-events=\"stroke\"", "pointer-events=\"all\"")
            } else {
                hit
            };
            result += &format!(r#"<g cursor="move" opacity="0">{hit}</g>"#);
        } else if a.kind == "brace" && d.editing != Some(i) {
            let (_, tip, _, _) = brace(a, p);
            result += &format!(
                r#"<circle cx="{}" cy="{}" r="10" fill="transparent" pointer-events="auto" cursor="pointer" data-action="edit-brace" data-value="{i}"><title>{}</title></circle>"#,
                tip.x,
                tip.y,
                if a.text.is_empty() {
                    "Add label at tip"
                } else {
                    "Edit label"
                }
            );
        }
        result += "</g>";
    }
    if d.tool == "select" {
        if let Some((i, a)) = d
            .selected
            .and_then(|i| d.annotations.get(i).map(|a| (i, a)))
        {
            let (x1, y1, x2, y2) = bounds(a, p);
            let (x, y, w, h) = (x1 - 8., y1 - 8., x2 - x1 + 16., y2 - y1 + 16.);
            let cx = (x1 + x2) / 2.;
            result += &format!(
                r#"<g class="chart-selection-group" data-screenshot-exclude><rect class="chart-selection-box" x="{x}" y="{y}" width="{w}" height="{h}"></rect><rect data-annotation="{i}" x="{x}" y="{y}" width="{w}" height="{h}" fill="none" stroke="transparent" stroke-width="14" pointer-events="stroke" cursor="move"></rect><line class="chart-rotate-stem" x1="{cx}" y1="{y}" x2="{cx}" y2="{}"></line><circle class="chart-rotate-handle" data-handle="rotate" data-annotation="{i}" cx="{cx}" cy="{}" r="6"><title>Drag to rotate</title></circle>"#,
                y - 20.,
                y - 20.
            );
            for (mode, hx, hy, cursor, title) in [
                ("nw", x, y, "nwse-resize", "Resize top left"),
                ("ne", x + w, y, "nesw-resize", "Resize top right"),
                ("se", x + w, y + h, "nwse-resize", "Resize bottom right"),
                ("sw", x, y + h, "nesw-resize", "Resize bottom left"),
            ] {
                result += &format!(
                    r#"<rect class="chart-resize-handle" data-handle="{mode}" data-annotation="{i}" x="{}" y="{}" width="8" height="8" style="cursor:{cursor}"><title>{title}</title></rect>"#,
                    hx - 4.,
                    hy - 4.
                );
            }
            if a.kind != "pen" {
                for (mode, pt, title) in [
                    ("start", p.pixel(a.start()), "Drag start point"),
                    ("end", p.pixel(a.end()), "Drag end point"),
                ] {
                    result += &format!(
                        r#"<circle class="chart-endpoint-handle" data-handle="{mode}" data-annotation="{i}" cx="{}" cy="{}" r="5"><title>{title}</title></circle>"#,
                        pt.x, pt.y
                    );
                }
            }
            result += "</g>";
        }
    }
    if let Some(a) = &d.pending {
        if a.kind == "pen" {
            result += &graphic(a, p);
        } else {
            let point = p.pixel(a.start());
            result += &format!(
                r#"<g pointer-events="none" opacity="0.55" data-screenshot-exclude>{}<circle cx="{}" cy="{}" r="3" fill="{}"></circle>{}</g>"#,
                graphic(a, p),
                point.x,
                point.y,
                html(&a.color),
                if a.kind == "brace" {
                    let (_, tip, _, _) = brace(a, p);
                    format!(
                        r#"<circle cx="{}" cy="{}" r="3" fill="{}"></circle>"#,
                        tip.x,
                        tip.y,
                        html(&a.color)
                    )
                } else {
                    String::new()
                }
            );
        }
    }
    result
}
pub fn pointer(kind: &str, event: PointerEvent) {
    let Some(svg) = element(if APP.with(|c| !c.borrow().drawing.tool.is_empty()) {
        "annotation-svg"
    } else {
        "graph-svg"
    }) else {
        return;
    };
    let client = visual::event_point(event.as_ref());
    let target = event.target().and_then(|t| t.dyn_into::<Element>().ok());
    let inside = target.as_ref().is_some_and(|t| svg.contains(Some(t)));
    let bounds_svg = svg.get_bounding_client_rect();
    if bounds_svg.width() == 0. {
        return;
    }
    if kind == "pointerdown" && inside && APP.with(|c| !c.borrow().drawing.tool.is_empty()) {
        visual::focus(&svg);
    }
    let (mut repaint, mut full) = (false, false);
    APP.with(|c| {
        let mut a = c.borrow_mut();
        let plot = a.plot;
        let point = Point {
            x: ((client.x - bounds_svg.x()) * plot.canvas_width / bounds_svg.width() - plot.x)
                / plot.width,
            y: ((client.y - bounds_svg.y()) * plot.canvas_height / bounds_svg.height() - plot.y)
                / plot.height,
        };
        if kind == "pointerdown" {
            a.drag = None;
            if !inside || event.button() != 0 || !event.is_primary() {
                return;
            }
            a.prepared_png = None;
            a.hovered.clear();
            a.drawing.editing = None;
            let annotation = target
                .as_ref()
                .and_then(|t| t.closest("[data-annotation]").ok().flatten())
                .and_then(|t| attribute(&t, "data-annotation").parse::<usize>().ok());
            let handle = target
                .as_ref()
                .map(|t| attribute(t, "data-handle"))
                .unwrap_or_default();
            let tool = a.drawing.tool.clone();
            if tool == "select" || tool.is_empty() {
                if let Some(i) = annotation {
                    a.drawing.selected = Some(i);
                    a.drag = Some(Drag {
                        x: point.x,
                        y: point.y,
                        moved: false,
                        mode: if handle.is_empty() {
                            "move".into()
                        } else {
                            handle
                        },
                        original: a.drawing.annotations.get(i).cloned(),
                    });
                    event.prevent_default();
                    full = true;
                } else if tool.is_empty()
                    && s(&a.prefs, "view") == "perfection"
                    && a.distance_metrics().len() == 3
                {
                    a.drag = Some(Drag {
                        x: client.x,
                        y: client.y,
                        moved: false,
                        mode: "camera".into(),
                        original: None,
                    });
                    a.drawing.selected = None;
                } else if tool == "select" {
                    a.drawing.selected = None;
                    full = true;
                }
            } else if tool == "pen" {
                a.drawing.pending = Some(Annotation {
                    kind: tool,
                    color: a.drawing.color.clone(),
                    points: vec![point],
                    flipped: false,
                    text: String::new(),
                });
                a.drag = Some(Drag {
                    x: point.x,
                    y: point.y,
                    moved: false,
                    mode: "pen".into(),
                    original: None,
                });
                event.prevent_default();
                full = true;
            } else if let Some(mut pending) = a.drawing.pending.take() {
                let start = pending.start();
                if ((point.x - start.x) * plot.width).hypot((point.y - start.y) * plot.height) < 1.
                {
                    a.drawing.pending = Some(pending);
                    return;
                }
                pending.points[1] = point;
                a.drawing.annotations.push(pending);
                if a.drawing.tool == "brace" {
                    a.drawing.editing = Some(a.drawing.annotations.len() - 1);
                }
                event.prevent_default();
                full = true;
            } else {
                a.drawing.pending = Some(Annotation {
                    kind: tool,
                    color: a.drawing.color.clone(),
                    points: vec![point, point],
                    flipped: false,
                    text: String::new(),
                });
                event.prevent_default();
                full = true;
            }
        } else if kind == "pointermove" {
            if let Some(mut drag) = a.drag.take() {
                let mut keep = true;
                if drag.mode == "camera" {
                    let dx = client.x - drag.x;
                    let dy = client.y - drag.y;
                    if drag.moved || dx.hypot(dy) >= 4. {
                        a.camera = camera_turn(a.camera, dx * 0.008, dy * 0.008);
                        drag.x = client.x;
                        drag.y = client.y;
                        drag.moved = true;
                        repaint = true;
                    }
                } else if drag.mode == "pen" {
                    if let Some(pending) = &mut a.drawing.pending {
                        let last = pending.end();
                        let dx = (point.x - last.x) * plot.width;
                        let dy = (point.y - last.y) * plot.height;
                        let len = dx.hypot(dy);
                        if len >= 4. {
                            let progress = (len - 3.) / len;
                            pending.points.push(Point {
                                x: last.x + (point.x - last.x) * progress,
                                y: last.y + (point.y - last.y) * progress,
                            });
                            drag.moved = true;
                            repaint = true;
                        }
                    }
                } else if let (Some(i), Some(original)) =
                    (a.drawing.selected, drag.original.as_ref())
                {
                    let dx = (point.x - drag.x) * plot.width;
                    let dy = (point.y - drag.y) * plot.height;
                    let (x1, y1, x2, y2) = bounds(original, plot);
                    let (cx, cy) = ((x1 + x2) / 2., (y1 + y2) / 2.);
                    let mut next = original.clone();
                    for (i, p) in next.points.iter_mut().enumerate() {
                        let old = original.points[i];
                        let pixel = plot.pixel(old);
                        let transformed = match drag.mode.as_str() {
                            "move" => Point {
                                x: pixel.x + dx,
                                y: pixel.y + dy,
                            },
                            "start" if i == 0 => plot.pixel(point),
                            "end" if i == original.points.len() - 1 => plot.pixel(point),
                            "rotate" => {
                                let start = plot.pixel(Point {
                                    x: drag.x,
                                    y: drag.y,
                                });
                                let current = plot.pixel(point);
                                let angle = (current.y - cy).atan2(current.x - cx)
                                    - (start.y - cy).atan2(start.x - cx);
                                Point {
                                    x: cx + (pixel.x - cx) * angle.cos()
                                        - (pixel.y - cy) * angle.sin(),
                                    y: cy
                                        + (pixel.x - cx) * angle.sin()
                                        + (pixel.y - cy) * angle.cos(),
                                }
                            }
                            "nw" | "ne" | "sw" | "se" => {
                                let west = drag.mode.ends_with('w');
                                let north = drag.mode.starts_with('n');
                                let ax = if west { x2 } else { x1 };
                                let ay = if north { y2 } else { y1 };
                                let sx = ((x2 - x1 + if west { -dx } else { dx }).max(4.))
                                    / (x2 - x1).max(10.);
                                let sy = ((y2 - y1 + if north { -dy } else { dy }).max(4.))
                                    / (y2 - y1).max(10.);
                                let (sx, sy) = if event.shift_key() {
                                    let scale = sx.max(sy);
                                    (scale, scale)
                                } else {
                                    (sx, sy)
                                };
                                Point {
                                    x: ax + (pixel.x - ax) * sx,
                                    y: ay + (pixel.y - ay) * sy,
                                }
                            }
                            _ => pixel,
                        };
                        *p = Point {
                            x: (transformed.x - plot.x) / plot.width,
                            y: (transformed.y - plot.y) / plot.height,
                        };
                    }
                    a.drawing.annotations[i] = next;
                    drag.moved |= dx.hypot(dy) > 2.;
                    repaint = true;
                } else {
                    keep = false;
                }
                if keep {
                    a.drag = Some(drag);
                }
            } else if inside {
                if let Some(pending) = &mut a.drawing.pending {
                    if pending.kind != "pen" {
                        pending.points[1] = point;
                        repaint = true;
                    }
                }
            }
        } else if kind == "pointerup" || kind == "pointercancel" {
            if let Some(drag) = a.drag.take() {
                if drag.mode == "camera" {
                    let _ = svg.class_list().remove_1("dragging");
                }
                if drag.mode == "pen" {
                    if kind == "pointerup" {
                        if let Some(mut pending) = a.drawing.pending.take() {
                            let last = pending.end();
                            if pending.points.len() > 1
                                && ((last.x - point.x) * plot.width)
                                    .hypot((last.y - point.y) * plot.height)
                                    >= 0.5
                            {
                                pending.points.push(point);
                            }
                            if !pending.points.is_empty() {
                                a.drawing.annotations.push(pending);
                            }
                        }
                    } else if let Some(pending) = a.drawing.pending.take() {
                        a.drawing.annotations.push(pending);
                    }
                    full = true;
                }
                if drag.moved {
                    event.prevent_default();
                    let _ = svg.set_attribute("data-suppress-click", "true");
                    later(0, || {
                        if let Some(svg) =
                            element(if APP.with(|c| !c.borrow().drawing.tool.is_empty()) {
                                "annotation-svg"
                            } else {
                                "graph-svg"
                            })
                        {
                            let _ = svg.remove_attribute("data-suppress-click");
                        }
                    });
                }
            }
        }
    });
    if full {
        render::render();
    } else if repaint {
        render::request_repaint();
    }
}
