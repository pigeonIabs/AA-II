use crate::visual::{btn, icon};
use crate::*;
fn guide(plot: charts::Plot, y: f64) {
    let Some(current) = element("bar-hover-guide") else {
        return;
    };
    let attributes = [
        ("class", "bar-hover-guide"),
        ("aria-hidden", "true"),
        ("pointer-events", "none"),
        ("stroke-dasharray", "0 7"),
        ("stroke-linecap", "round"),
        ("id", "bar-hover-guide"),
        ("data-native-guide", ""),
    ];
    let set = |e: &Element, key: &str, value: &str| {
        if e.get_attribute(key).as_deref() != Some(value) {
            let _ = e.set_attribute(key, value);
        }
    };
    let update = |line: &Element| {
        for (key, value) in [
            ("x1", plot.x),
            ("x2", plot.canvas_width - 2.),
            ("y1", y),
            ("y2", y),
        ] {
            set(line, key, &value.to_string());
        }
    };
    if current.has_attribute("data-native-guide") && current.child_nodes().length() == 2 {
        for i in 0..2 {
            if let Some(line) = current
                .child_nodes()
                .item(i)
                .and_then(|n| n.dyn_into::<Element>().ok())
            {
                update(&line);
            }
        }
        return;
    }
    let Ok(group) = document().create_element_ns(Some("http://www.w3.org/2000/svg"), "g") else {
        return;
    };
    for (key, value) in attributes {
        set(&group, key, value);
    }
    for (stroke, width, opacity) in [
        ("var(--page)", "5", Some("0.85")),
        ("var(--chart-highlight, #f4ffe8)", "2.5", None),
    ] {
        if let Ok(line) = document().create_element_ns(Some("http://www.w3.org/2000/svg"), "line") {
            update(&line);
            set(&line, "stroke", stroke);
            set(&line, "stroke-width", width);
            if let Some(opacity) = opacity {
                set(&line, "stroke-opacity", opacity);
            }
            set(&line, "vector-effect", "non-scaling-stroke");
            let _ = group.append_child(&line);
        }
    }
    if let Some(parent) = current.parent_node() {
        let _ = parent.replace_child(&group, &current);
    }
}
pub fn hide() {
    if let Some(t) = element("chart-tooltip") {
        let _ = t.set_attribute("hidden", "");
    }
    if let Some(g) = element("bar-hover-guide") {
        g.set_inner_html("");
    }
    update_bars("");
    if let Ok(nodes) = document().query_selector_all(".pareto-point") {
        for i in 0..nodes.length() {
            if let Some(e) = nodes.item(i).and_then(|n| n.dyn_into::<Element>().ok()) {
                let _ = e.class_list().remove_1("active");
                if let Ok(Some(m)) = e.query_selector(".pareto-marker") {
                    let _ = m.set_attribute(
                        "r",
                        if e.class_list().contains("comparison-anchor") {
                            "5.5"
                        } else {
                            "4.5"
                        },
                    );
                }
            }
        }
    }
}
fn update_bars(id: &str) {
    APP.with(|c| {
        let a = c.borrow();
        let (_, bars) = bar::groups(&a);
        let active = bars
            .iter()
            .find(|bar| bar.efforts.iter().any(|m| s(m, "id") == id));
        if let Ok(nodes) = document().query_selector_all(".bar-item") {
            for i in 0..nodes.length() {
                if let Some(e) = nodes.item(i).and_then(|n| n.dyn_into::<Element>().ok()) {
                    let current =
                        active.is_some_and(|bar| bar.key == attribute(&e, "data-bar-key"));
                    let _ = e.set_attribute(
                        "opacity",
                        if active.is_some() && !current {
                            "0.58"
                        } else {
                            "1"
                        },
                    );
                    if let Ok(Some(outline)) = e.query_selector("[data-bar-outline]") {
                        if let Some(bar) =
                            bars.iter().find(|b| b.key == attribute(&e, "data-bar-key"))
                        {
                            let dark = bar::dark(s(bar.model, "color"));
                            let stacked = b(&a.prefs, "stackReasoning");
                            let _ = outline.set_attribute(
                                "stroke",
                                &if current {
                                    "var(--chart-highlight, #fff)".into()
                                } else {
                                    format!(
                                        "var(--chart-outline, {})",
                                        if dark {
                                            "#858585"
                                        } else {
                                            s(bar.model, "color")
                                        }
                                    )
                                },
                            );
                            let _ = outline.set_attribute(
                                "stroke-width",
                                if current {
                                    "1.6"
                                } else if stacked {
                                    "0.8"
                                } else if dark {
                                    "1"
                                } else {
                                    "0.6"
                                },
                            );
                        }
                    }
                    if let Ok(parts) = e.query_selector_all(".stacked-segment") {
                        for j in 0..parts.length() {
                            if let Some(part) =
                                parts.item(j).and_then(|n| n.dyn_into::<Element>().ok())
                            {
                                if let Ok(Some(hit)) = part.query_selector(".effort-segment-hit") {
                                    let _ = hit.set_attribute(
                                        "stroke",
                                        if attribute(&part, "data-hover") == id {
                                            "var(--chart-highlight, #fff)"
                                        } else {
                                            "none"
                                        },
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
    });
}
fn position(a: &App, target: &Element, x: f64, y: f64) -> Option<String> {
    match s(&a.prefs, "view") {
        "pareto" => {
            let (w, h) = visual::measured(a, "pareto", 1000., 520.);
            let px = attribute(target, "data-hover-x")
                .parse::<f64>()
                .unwrap_or(0.);
            let py = attribute(target, "data-hover-y")
                .parse::<f64>()
                .unwrap_or(0.);
            Some(format!(
                "left:{}px;top:{}px;",
                8_f64.max((px + 18.).min(w - 306.)),
                8_f64.max((py + 18.).min(h - 218.))
            ))
        }
        "perfection" => {
            let r = element("orbit-panel")?.get_bounding_client_rect();
            Some(format!(
                "left:{}px;top:{}px;",
                8_f64.max((x - r.left() + 18.).min(r.width() - 326.)),
                15_f64.max((y - r.top() + 15.).min(r.height() - 255.))
            ))
        }
        _ => {
            let r = element("bar-container")?.get_bounding_client_rect();
            let sr = element("graph-svg")?.get_bounding_client_rect();
            let guide_y = attribute(target, "data-guide-y")
                .parse::<f64>()
                .unwrap_or(a.plot.y);
            let line = sr.top() + guide_y * sr.height() / a.plot.canvas_height;
            Some(format!(
                "width:220px;left:{}px;top:{}px;",
                0_f64.max(
                    (x - r.left() - 220_f64.min(r.width()) / 2.)
                        .min(r.width() - 220_f64.min(r.width()))
                ),
                y.min(line) - r.top() - 12.
            ))
        }
    }
}
pub fn show(target: &Element, id: &str, x: f64, y: f64) {
    let Some(t) = element("chart-tooltip") else {
        return;
    };
    if attribute(target, "data-tooltip") == "ranking" {
        let _ = t.set_attribute("hidden", "");
        return;
    }
    if !t.has_attribute("hidden") && t.get_attribute("data-tooltip-id").as_deref() == Some(id) {
        if let Some(style) = APP.with(|c| position(&c.borrow(), target, x, y)) {
            let _ = t.set_attribute("style", &style);
        }
        return;
    }
    if APP.with(|c| s(&c.borrow().prefs, "view") == "index") {
        update_bars(id);
    }
    APP.with(|c|{let a=c.borrow();let ids=id.split('|').collect::<Vec<_>>();let models=a.models().iter().filter(|m|ids.contains(&s(m,"id"))).collect::<Vec<_>>();let Some(m)=models.first().copied()else{return;};let view=s(&a.prefs,"view");
 let(style,content)=if view=="pareto"{
 let(w,h)=visual::measured(&a,"pareto",1000.,520.);let px=attribute(&target,"data-hover-x").parse::<f64>().unwrap_or(0.);let py=attribute(&target,"data-hover-y").parse::<f64>().unwrap_or(0.);let content=models.iter().map(|m|{let family=family_name(m);let has=family!=s(m,"name")||!m["effort"].is_null()||a.models().iter().filter(|other|family_key(other)==family_key(m)).count()>1;format!(r#"<div class="pareto-tooltip-model"><span class="swatch" style="background:{}"></span><div class="pareto-tooltip-name">{}{}</div>{}</div>"#,html(s(m,"color")),btn("pareto-family",s(m,"id"),&html(&family),"pareto-tooltip-family",r#"title="Compare all measured reasoning efforts""#),if has{btn("pareto-focus",s(m,"id"),&html(&effort(m).0.to_lowercase()),"pareto-tooltip-effort",r#"title="Compare this effort only""#)}else{String::new()},btn("detail",s(m,"id"),&icon("Info",13,""),"pareto-tooltip-details",&format!(r#"aria-label="Details for {}" title="Model details""#,html(s(m,"name")))))}).collect::<String>();if let Ok(nodes)=document().query_selector_all(".pareto-point"){for i in 0..nodes.length(){if let Some(e)=nodes.item(i).and_then(|n|n.dyn_into::<Element>().ok()){let active=attribute(&e,"data-location")==id;let _=e.class_list().toggle_with_force("active",active);if let Ok(Some(marker))=e.query_selector(".pareto-marker"){let _=marker.set_attribute("r",if active{"6"}else if e.class_list().contains("comparison-anchor"){"5.5"}else{"4.5"});}}}}(format!("left:{}px;top:{}px;",8_f64.max((px+18.).min(w-306.)),8_f64.max((py+18.).min(h-218.))),format!(r#"<div class="pareto-tooltip-models">{content}</div><dl><div><dt>Intelligence Index</dt><dd>{}</dd></div><div><dt>{}</dt><dd>{}</dd></div></dl>"#,precision(n(m,"intelligence"),"intelligence"),metric_label(a.metric()),precision(n(m,a.metric()),a.metric())))
 }else if view=="perfection"{
 if attribute(&target,"data-tooltip")=="ranking"{let _=t.set_attribute("hidden","");return;}let Some(panel)=element("orbit-panel")else{return;};let r=panel.get_bounding_client_rect();let d=distance(m,a.scales(),&a.distance_metrics()).unwrap_or(0.);let axes=distance::axes(&a);(format!("left:{}px;top:{}px;",8_f64.max((x as f64-r.left()+18.).min(r.width()-326.)),15_f64.max((y as f64-r.top()+15.).min(r.height()-255.))),format!(r#"<div class="tooltip-provider"><span class="swatch" style="background:{}"></span>{}</div><strong>{}</strong><div class="distance-readout"><span>Distance to perfection</span><b title="{}">{}</b></div><dl>{}</dl>"#,html(s(m,"color")),html(s(m,"provider")),html(s(m,"name")),precision(Some(d),""),format_number(d,4),axes.iter().map(|k|format!("<div><dt>{}</dt><dd>{}</dd></div>",metric_label(k),precision(n(m,k),k))).collect::<String>()))
 }else{
 let Some(container)=element("bar-container")else{return;};let Some(svg)=element("graph-svg")else{return;};let r=container.get_bounding_client_rect();let sr=svg.get_bounding_client_rect();let guide_y=attribute(&target,"data-guide-y").parse::<f64>().unwrap_or(a.plot.y);let line=sr.top()+guide_y*sr.height()/a.plot.canvas_height;let(_,bars)=bar::groups(&a);let Some(bar)=bars.iter().find(|b|b.efforts.iter().any(|m|s(m,"id")==id))else{return;};let content=if b(&a.prefs,"stackReasoning")&&bar.efforts.len()>1{format!(r#"<strong class="tooltip-title"><span class="swatch" style="background:{}"></span>{}</strong><div class="effort-tooltip-heading"><span>Effort</span><span>{}</span></div><dl class="effort-values">{}</dl>"#,html(s(bar.model,"color")),html(&bar.name),if a.metric()=="intelligence"{"Index score"}else{metric_label(a.metric())},bar.efforts.iter().enumerate().map(|(i,m)|format!(r#"<div class="{}"><dt><span class="swatch" style="background:{}"></span>{}</dt><dd>{}</dd></div>"#,if s(m,"id")==id{"emphasized"}else{""},bar::shade(s(m,"color"),i,bar.efforts.len()).0,html(&effort(m).0),precision(n(m,a.metric()),a.metric()))).collect::<String>())}else{format!(r#"<strong class="tooltip-title"><span class="swatch" style="background:{}"></span>{}</strong><dl>{}</dl>"#,html(s(m,"color")),html(s(m,"name")),METRICS.iter().filter(|k|!parameter_metric(k)||parameter_model(m)&&n(m,k).is_some()).map(|k|format!(r#"<div class="{}"><dt>{}</dt><dd title="{}">{}</dd></div>"#,if *k==a.metric(){"emphasized"}else{""},metric_label(k),html(&precision(n(m,k),k)),precision(n(m,k),k))).collect::<String>())};guide(a.plot,guide_y);(format!("width:220px;left:{}px;top:{}px;",0_f64.max((x as f64-r.left()-220_f64.min(r.width())/2.).min(r.width()-220_f64.min(r.width()))),(y as f64).min(line)-r.top()-12.),content)
 };let _=t.set_attribute("data-tooltip-id",id);t.set_inner_html(&content);let _=t.set_attribute("style",&style);let _=t.remove_attribute("hidden");
 });
}
