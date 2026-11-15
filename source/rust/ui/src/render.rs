use crate::visual::{btn, icon};
use crate::*;
pub fn check(field: &str, key: &str, label: &str, checked: bool) -> String {
    visual::checkbox(field, key, &html(label), checked, "", "")
}
pub fn select(
    field: &str,
    key: &str,
    current: &str,
    options: &[(&str, &str)],
    disabled: bool,
) -> String {
    let label = match key {
        "weights" => "Model weights",
        "reasoning" => "Reasoning type",
        "provider" => "Provider",
        _ => key,
    };
    format!(
        r#"<label class="select-control{}"><span class="sr-only">{}</span><select data-field="{}" data-key="{}" {}>{}</select>{}</label>"#,
        if key == "provider" {
            " provider-filter"
        } else {
            ""
        },
        html(label),
        html(field),
        html(key),
        if disabled { "disabled" } else { "" },
        options
            .iter()
            .map(|(v, l)| format!(
                r#"<option value="{}" {}>{}</option>"#,
                html(v),
                if *v == current { "selected" } else { "" },
                html(l)
            ))
            .collect::<String>(),
        icon("ChevronDown", 13, "")
    )
}
pub fn logo(m: &Value, size: u32) -> String {
    format!(
        r#"<img class="model-logo" src="{}" width="{size}" height="{size}" alt="">"#,
        html(s(m, "logo"))
    )
}
pub fn link_size(url: &str, label: &str, size: u32, attrs: &str) -> String {
    if !url.starts_with("https://") && !url.starts_with("http://") {
        return html(label);
    }
    format!(
        r#"<a href="{}" target="_blank" rel="noreferrer" {}>{}{}</a>"#,
        html(url),
        attrs,
        html(label),
        icon("ArrowUpRight", size, "")
    )
}
pub fn link(url: &str, label: &str) -> String {
    link_size(url, label, 12, "")
}
pub fn modal(title: &str, content: &str, wide: bool) -> String {
    format!(
        r#"<dialog id="app-dialog" class="modal {}" aria-labelledby="dialog-title"><div class="modal-shell"><div class="modal-heading"><h2 id="dialog-title">{}</h2>{}</div>{}</div></dialog>"#,
        if wide { "wide" } else { "" },
        html(title),
        btn(
            "close",
            "",
            &icon("X", 19, ""),
            "icon-button",
            r#"aria-label="Close dialog""#
        ),
        content
    )
}
pub fn metric_tabs(a: &App) -> String {
    let pareto = s(&a.prefs, "view") == "pareto";
    METRICS
        .iter()
        .filter(|m| !pareto || **m != "intelligence")
        .map(|m| {
            btn(
                "metric",
                m,
                metric_label(m),
                if a.metric() == *m { "active" } else { "" },
                &if pareto {
                    format!(r#"aria-pressed="{}""#, a.metric() == *m)
                } else {
                    format!(r#"role="tab" aria-selected="{}""#, a.metric() == *m)
                },
            )
        })
        .collect()
}
pub fn legend(a: &App) -> String {
    let mut models = if s(&a.prefs, "view") == "pareto" {
        a.models()
            .iter()
            .filter(|m| a.pareto_shown.iter().any(|id| id == s(m, "id")))
            .collect::<Vec<_>>()
    } else {
        a.visible()
    };
    models.sort_by(|a, b| {
        n(b, "intelligence")
            .unwrap_or(-1.)
            .total_cmp(&n(a, "intelligence").unwrap_or(-1.))
    });
    let mut providers: Vec<&Value> = Vec::new();
    for m in models {
        if let Some(index) = providers
            .iter()
            .position(|p| s(p, "providerSlug") == s(m, "providerSlug"))
        {
            providers[index] = m;
        } else {
            providers.push(m);
        }
    }
    let density = providers
        .iter()
        .map(|m| s(m, "provider").encode_utf16().count() as f64 * 0.62 + 1.5)
        .sum::<f64>();
    let width = a.sizes.get("legend").map(|s| s.0).unwrap_or(0.);
    let available = (width - providers.len().saturating_sub(1) as f64 * 22.).max(0.);
    let font = (available / density.max(1.)).clamp(10., 22.);
    format!(
        r#"<div class="provider-legend" data-measure="legend" style="font-size:{font}px" aria-label="Visible model providers">{}</div>"#,
        providers
            .iter()
            .map(|m| btn(
                "provider",
                s(m, "providerSlug"),
                &format!(
                    r#"<span class="swatch" style="background:{}{}"></span>{}"#,
                    html(s(m, "color")),
                    if ["#1f1f1f", "#000000", "black"].contains(&s(m, "color")) {
                        ";border:1px solid var(--chart-outline, #777)"
                    } else {
                        ""
                    },
                    html(s(m, "provider"))
                ),
                "",
                &format!(r#"title="Filter {}""#, html(s(m, "provider")))
            ))
            .collect::<String>()
    )
}
fn screenshot(a: &App) -> String {
    let busy = a.screenshot_busy;
    let copied = a.screenshot_state == "copied";
    let state = if busy {
        "Copying"
    } else if copied {
        "Copied"
    } else {
        "Screenshot"
    };
    let button = btn(
        "screenshot",
        "",
        &(icon(
            if busy {
                "LoaderCircle"
            } else if copied {
                "Check"
            } else {
                "Camera"
            },
            15,
            if busy { "spin" } else { "" },
        ) + &format!("<span>{state}</span>")),
        "button screenshot-button",
        &format!(
            r#"type="button" {} aria-label="Copy screenshot to clipboard" aria-busy="{busy}" title="Copy a PNG of this view with its heading and current filters, up to 32 megapixels""#,
            if busy { "disabled" } else { "" }
        ),
    );
    let recovery = if !a.screenshot_error.is_empty() || a.prepared_png.is_some() {
        format!(
            r#"<div class="screenshot-error {}" role="{}"><div class="screenshot-message"><span>{}</span>{}</div>{}</div>"#,
            if a.prepared_png.is_some() && a.screenshot_error.is_empty() {
                "screenshot-ready"
            } else {
                ""
            },
            if a.screenshot_error.is_empty() {
                "status"
            } else {
                "alert"
            },
            html(if a.screenshot_error.is_empty() {
                "Screenshot ready"
            } else {
                &a.screenshot_error
            }),
            btn(
                "screenshot-dismiss",
                "",
                &icon("X", 13, ""),
                "icon-button",
                &format!(
                    r#"type="button" aria-label="Dismiss screenshot message" {}"#,
                    if busy { "disabled" } else { "" }
                )
            ),
            if a.prepared_png.is_some() {
                format!(
                    r#"<div class="screenshot-recovery">{}{}</div>"#,
                    btn(
                        "copy-prepared",
                        "",
                        &(icon("Camera", 13, "") + "Copy again"),
                        "text-button",
                        &format!(r#"type="button" {}"#, if busy { "disabled" } else { "" })
                    ),
                    btn(
                        "save-png",
                        "",
                        &(icon("Download", 13, "") + "Save PNG"),
                        "text-button",
                        r#"type="button""#
                    )
                )
            } else {
                String::new()
            }
        )
    } else {
        String::new()
    };
    format!(
        r#"<div class="screenshot-action">{button}<span class="sr-only" role="status">{}</span>{recovery}</div>"#,
        if copied {
            "Screenshot copied to clipboard"
        } else if busy {
            "Preparing screenshot"
        } else {
            ""
        }
    )
}
fn shell(a: &mut App) -> String {
    if a.snapshot.is_null() {
        return format!(
            r#"<div class="loading-screen"><img class="brand-symbol" src="/logos/artificial-analysis.svg" width="30" height="30" alt="Artificial Analysis"><h1>Intelligence Index</h1>{}</div>"#,
            if a.error.is_empty() {
                format!(
                    r#"<p>{}Opening the local dataset</p>"#,
                    icon("LoaderCircle", 16, "spin")
                )
            } else {
                format!(
                    r#"<p>{}</p>{}"#,
                    html(&a.error),
                    btn("reload", "", "Reload snapshot", "button", "")
                )
            }
        );
    }
    let view = s(&a.prefs, "view").to_string();
    let index = view == "index";
    let filters = &a.prefs["filters"];
    let default = default_filters();
    let active = default
        .as_object()
        .unwrap()
        .keys()
        .filter(|key| filters[*key] != default[*key])
        .count();
    let mut providers = a
        .models()
        .iter()
        .map(|m| (s(m, "providerSlug"), s(m, "provider")))
        .collect::<Vec<_>>();
    providers.sort_by(|a, b| visual::compare(a.1, b.1));
    providers.dedup_by_key(|p| p.0);
    let options = std::iter::once(("all", "All providers"))
        .chain(providers)
        .collect::<Vec<_>>();
    let mut weights = vec![
        ("all", "All weights"),
        ("open", "Open weights"),
        ("proprietary", "Proprietary"),
    ];
    if a.size_only() {
        weights.insert(0, ("size", "Known sizes"));
    }
    let refresh = btn(
        "refresh",
        "",
        &(icon("RefreshCw", 15, "")
            + &format!(
                "<span>{}</span>",
                if a.refreshing {
                    "Refreshing"
                } else {
                    "Refresh data"
                }
            )),
        "button",
        &format!(
            r#"{} aria-busy="{}""#,
            if a.refreshing { "disabled" } else { "" },
            a.refreshing
        ),
    );
    let light = document()
        .document_element()
        .is_some_and(|e| attribute(&e, "data-theme") == "light");
    let theme_label = if light {
        "Switch to dark mode"
    } else {
        "Switch to light mode"
    };
    let header = format!(
        r#"<header class="site-header"><div class="page-brand" data-screenshot-heading><img class="brand-symbol" src="/logos/artificial-analysis.svg" width="48" height="48" alt=""><div><h1>Artificial Analysis</h1><p class="benchmark-subtitle">Intelligence Index v{}</p></div></div><nav aria-label="Main navigation">{}{}{}</nav><div class="heading-actions">{}{}<details class="export-menu"><summary class="button">{}Export{}</summary><div class="dropdown-menu">{}{}<a href="/api/raw">{}Full precision source data</a></div></details></div><div class="header-actions">{}{}</div></header>"#,
        html(s(&a.snapshot, "indexVersion")),
        btn(
            "view",
            "index",
            "Intelligence Index",
            if view == "index" { "active" } else { "" },
            ""
        ),
        btn(
            "view",
            "perfection",
            "Distance to perfection<span class=\"nav-star\">✳</span>",
            if view == "perfection" { "active" } else { "" },
            ""
        ),
        btn(
            "view",
            "pareto",
            "Pareto frontier",
            if view == "pareto" { "active" } else { "" },
            ""
        ),
        screenshot(a),
        visual::refresh_border(a, &refresh),
        icon("Download", 15, ""),
        icon("ChevronDown", 13, ""),
        btn("export", "csv", "Selected models · CSV", "", ""),
        btn("export", "json", "Selected models · JSON", "", ""),
        icon("ArrowDownToLine", 14, ""),
        link_size(
            s(&a.snapshot, "chartSourceUrl"),
            "Original site",
            14,
            r#"class="source-link""#
        ),
        btn(
            "theme",
            "",
            &icon(if light { "Moon" } else { "Sun" }, 17, ""),
            "icon-button theme-toggle",
            &format!(r#"type="button" aria-label="{theme_label}" title="{theme_label}""#)
        )
    );
    let controls = format!(
        r#"<div class="model-controls"><section class="selection-toolbar" aria-label="Model selection and filters">{}<label class="checkbox-label temporary-comparison" title="Keep comparison changes for this session. Turn off to restore your saved models and display settings."><input type="checkbox" data-action="temporary" {}>Temporary comparison</label><div class="toolbar-divider"></div>{}<div class="toolbar-end">{}{}</div></section><header class="workspace-header" data-screenshot-filters><div class="workspace-parameters" data-screenshot-parameters>{}{}{}</div></header></div>"#,
        btn(
            "picker",
            "",
            &(icon("Plus", 16, "")
                + &format!(
                    r#"<span>Models</span><span class="selection-count mono">{}<span> / {}</span></span>{}"#,
                    a.effective_count(),
                    a.models().len(),
                    icon("ChevronDown", 14, "")
                )),
            "model-select-button",
            ""
        ),
        if a.baseline.is_some() { "checked" } else { "" },
        btn(
            "more-filters",
            "",
            &(icon("Filter", 14, "")
                + "Filters"
                + &if active > 0 {
                    format!("<span>{active}</span>")
                } else {
                    String::new()
                }),
            &format!(
                "filter-button {}",
                if active > 0 || a.more_filters {
                    "active"
                } else {
                    ""
                }
            ),
            &format!(r#"aria-expanded="{}""#, a.more_filters)
        ),
        if view != "pareto" {
            format!(
                r#"<span class="muted">{} visible</span>"#,
                a.visible().len()
            )
        } else {
            String::new()
        },
        if active > 0 {
            btn(
                "reset-filters",
                "",
                &("Reset".to_string() + &icon("X", 12, "")),
                "text-button",
                "",
            )
        } else {
            String::new()
        },
        select(
            "filter",
            "weights",
            if a.size_only() {
                "size"
            } else {
                s(filters, "weights")
            },
            &weights,
            a.size_only()
        ),
        select(
            "filter",
            "reasoning",
            s(filters, "reasoning"),
            &[
                ("all", "All reasoning"),
                ("reasoning", "Reasoning"),
                ("standard", "Standard")
            ],
            false
        ),
        select(
            "filter",
            "provider",
            s(filters, "provider"),
            &options,
            false
        )
    );
    let expanded = if a.more_filters {
        format!(
            r#"<div class="expanded-filters"><label class="filter-field">INPUT<select data-field="filter" data-key="modality">{}</select></label><label class="filter-field score-filter">MINIMUM INDEX <output class="mono">{}</output><input id="min-score" type="range" min="0" max="100" data-field="filter" data-key="minScore" value="{}"></label></div>"#,
            [
                ("all", "All modalities"),
                ("multimodal", "Multimodal"),
                ("text", "Text only")
            ]
            .iter()
            .map(|(v, l)| format!(
                r#"<option value="{v}" {}>{l}</option>"#,
                if s(filters, "modality") == *v {
                    "selected"
                } else {
                    ""
                }
            ))
            .collect::<String>(),
            n(filters, "minScore").unwrap_or(0.),
            n(filters, "minScore").unwrap_or(0.)
        )
    } else {
        String::new()
    };
    let metric_toolbar = if index {
        format!(
            r#"<div class="metric-toolbar"><div class="metric-tabs" role="tablist" aria-label="Comparison metric">{}</div><div class="view-switch">{}{}</div></div>"#,
            metric_tabs(a),
            btn(
                "mode",
                "chart",
                &icon("BarChart3", 17, ""),
                if s(&a.prefs, "chartMode") == "chart" {
                    "active"
                } else {
                    ""
                },
                r#"aria-label="Bar chart view" title="Bar chart""#
            ),
            btn(
                "mode",
                "table",
                &icon("Table2", 17, ""),
                if s(&a.prefs, "chartMode") == "table" {
                    "active"
                } else {
                    ""
                },
                r#"aria-label="Table view" title="All metrics table""#
            )
        )
    } else {
        String::new()
    };
    let workspace = retained::workspace(a);
    let notice = if a.error.is_empty() && a.notice.is_empty() {
        String::new()
    } else {
        format!(
            r#"<div class="notice {}" role="status"><span>{}</span>{}</div>"#,
            if a.error.is_empty() {
                ""
            } else {
                "error-notice"
            },
            html(if a.error.is_empty() {
                &a.notice
            } else {
                &a.error
            }),
            btn(
                "dismiss",
                "",
                &icon("X", 14, ""),
                "icon-button",
                r#"aria-label="Dismiss message""#
            )
        )
    };
    let modal = if a.picker.is_some() {
        picker::render(a)
    } else if a.dialog == "method" {
        crate::dialogs::method(a)
    } else if a.dialog == "detail" {
        crate::dialogs::detail(a)
    } else {
        String::new()
    };
    format!(
        r#"<div class="app-shell{}">{header}<main class="{}-page">{notice}{controls}{expanded}{metric_toolbar}{workspace}<footer class="data-footer"><span><span class="status-square active"></span>Updated · <!-- -->{}<span class="footer-time"> at <!-- -->{}</span></span><div>{}{}</div></footer></main>{modal}</div>"#,
        if index && s(&a.prefs, "chartMode") == "chart" {
            " index-chart-shell"
        } else {
            ""
        },
        if view == "perfection" {
            "perfection"
        } else if view == "pareto" {
            "pareto"
        } else {
            "index"
        },
        visual::date(s(&a.snapshot, "updatedAt"), false),
        visual::date(s(&a.snapshot, "updatedAt"), true),
        link_size(
            s(&a.snapshot, "sourceUrl"),
            "Data by Artificial Analysis",
            12,
            ""
        ),
        btn(
            "method",
            "",
            &(icon("CircleHelp", 13, "") + "Methodology"),
            "text-button",
            ""
        )
    )
}
thread_local! {static OBSERVER:ResizeObserver={let f=Closure::wrap(Box::new(move ||{if measure_layout(){later(0,render);}})as Box<dyn FnMut()>);let observer=ResizeObserver::new(f.as_ref().unchecked_ref()).unwrap();f.forget();observer};}
fn measure_layout() -> bool {
    let mut changed = false;
    if let Ok(nodes) = document().query_selector_all("[data-measure]") {
        APP.with(|c| {
            let mut a = c.borrow_mut();
            for i in 0..nodes.length() {
                if let Some(e) = nodes.item(i).and_then(|n| n.dyn_into::<Element>().ok()) {
                    let key = attribute(&e, "data-measure");
                    let size = (e.client_width() as f64, e.client_height() as f64);
                    if a.sizes.get(&key) != Some(&size) {
                        a.sizes.insert(key, size);
                        changed = true;
                    }
                }
            }
        });
    }
    changed
}
fn matching_focus(signature: &[String; 6]) -> Vec<Element> {
    let mut matches = Vec::new();
    if let Ok(nodes) = document().query_selector_all("[id],[data-action],[data-field]") {
        for i in 0..nodes.length() {
            if let Some(e) = nodes.item(i).and_then(|n| n.dyn_into::<Element>().ok()) {
                if [
                    "id",
                    "data-action",
                    "data-value",
                    "data-field",
                    "data-key",
                    "value",
                ]
                .iter()
                .enumerate()
                .all(|(i, k)| attribute(&e, k) == signature[i])
                {
                    matches.push(e);
                }
            }
        }
    }
    matches
}
pub fn render() {
    PAINT_PENDING.with(|pending| pending.set(false));
    let Some(root) = element("root") else {
        return;
    };
    let focused = document()
        .active_element()
        .filter(|e| root.contains(Some(e)))
        .map(|e| {
            let signature = [
                "id",
                "data-action",
                "data-value",
                "data-field",
                "data-key",
                "value",
            ]
            .map(|k| attribute(&e, k));
            let ordinal = matching_focus(&signature)
                .iter()
                .position(|n| n == &e)
                .unwrap_or(0);
            let selection = e.dyn_ref::<HtmlInputElement>().and_then(|i| {
                Some((
                    i.selection_start().ok().flatten()?,
                    i.selection_end().ok().flatten()?,
                ))
            });
            (signature, ordinal, selection)
        });
    let mut scroll = HashMap::new();
    if let Ok(nodes) = root.query_selector_all("[data-scroll]") {
        for i in 0..nodes.length() {
            if let Some(e) = nodes.item(i).and_then(|n| n.dyn_into::<Element>().ok()) {
                scroll.insert(
                    attribute(&e, "data-scroll"),
                    (e.scroll_left(), e.scroll_top()),
                );
            }
        }
    }
    OBSERVER.with(|o| o.disconnect());
    for _ in 0..5 {
        let content = APP.with(|c| shell(&mut c.borrow_mut()));
        crate::dom::render(&root, &content);
        if !measure_layout() {
            break;
        }
    }
    if let Ok(nodes) = root.query_selector_all("[data-measure]") {
        OBSERVER.with(|observer| {
            for i in 0..nodes.length() {
                if let Some(e) = nodes.item(i).and_then(|n| n.dyn_into::<Element>().ok()) {
                    observer.observe(&e);
                }
            }
        });
    }
    if let Ok(nodes) = root.query_selector_all("[data-indeterminate]") {
        for i in 0..nodes.length() {
            if let Some(e) = nodes
                .item(i)
                .and_then(|n| n.dyn_into::<HtmlInputElement>().ok())
            {
                e.set_indeterminate(true);
            }
        }
    }
    if let Ok(nodes) = root.query_selector_all("[data-scroll]") {
        for i in 0..nodes.length() {
            if let Some(e) = nodes.item(i).and_then(|n| n.dyn_into::<Element>().ok()) {
                if let Some((x, y)) = scroll.get(&attribute(&e, "data-scroll")) {
                    e.set_scroll_left(*x);
                    e.set_scroll_top(*y);
                }
            }
        }
    }
    let dialog = element("app-dialog").and_then(|e| e.dyn_into::<HtmlDialogElement>().ok());
    if let Some(body) = document().body() {
        let _ = body
            .style()
            .set_property("overflow", if dialog.is_some() { "hidden" } else { "" });
    }
    if let Some(dialog) = dialog {
        if !dialog.open() {
            let _ = dialog.show_modal();
        }
    }
    if let Some(e) = element("brace-text") {
        visual::focus(&e);
    } else if let Some((signature, ordinal, selection)) = focused {
        if let Some(e) = matching_focus(&signature).get(ordinal) {
            visual::focus(e);
            if let (Some(input), Some((start, end))) = (e.dyn_ref::<HtmlInputElement>(), selection)
            {
                let _ = input.set_selection_range(start, end);
            }
        }
    }
    visual::align_direction();
    visual::sync_theme();
}
pub fn repaint_svg() {
    if let Some(g) = element("annotations") {
        dom::render(
            &g,
            &APP.with(|c| {
                let a = c.borrow();
                drawing::graphics(&a.drawing, a.plot)
            }),
        );
    }
    let camera = APP.with(|c| c.borrow().drag.as_ref().is_some_and(|d| d.mode == "camera"));
    if camera {
        let markup = APP.with(|c| {
            let a = c.borrow();
            distance::svg(&a, distance::Projection::new(&a))
        });
        if let Some(svg) = element("graph-svg") {
            dom::update_element(&svg, &markup);
        }
    }
}
thread_local! {static PAINT_PENDING:std::cell::Cell<bool>=const{std::cell::Cell::new(false)};}
pub fn request_repaint() {
    if PAINT_PENDING.with(|pending| pending.replace(true)) {
        return;
    }
    let callback = Closure::once_into_js(move |_: f64| {
        if PAINT_PENDING.with(|pending| pending.replace(false)) {
            repaint_svg();
        }
    });
    if window()
        .request_animation_frame(callback.unchecked_ref())
        .is_err()
    {
        PAINT_PENDING.with(|pending| pending.set(false));
        repaint_svg();
    }
}
