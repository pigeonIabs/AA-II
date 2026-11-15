use crate::*;
use crate::{
    render::*,
    visual::{btn, icon},
};
pub fn method(a: &App) -> String {
    let valid = a
        .method_cost
        .parse::<f64>()
        .is_ok_and(|v| v.is_finite() && v > 0.)
        && a.method_tokens
            .parse::<f64>()
            .is_ok_and(|v| v.is_finite() && v > 0.);
    let version = html(s(&a.snapshot, "indexVersion"));
    let description = html(s(&a.snapshot, "responseTimeDescription"));
    let metrics = a.distance_metrics();
    let terms = metrics
        .iter()
        .map(|m| match m.as_str() {
            "intelligence" => "s²",
            "cost" => "c²",
            _ => "t²",
        })
        .collect::<Vec<_>>()
        .join(" + ");
    let formula = metrics
        .iter()
        .map(|m| match m.as_str() {
            "intelligence" => "<div>s = 100 − Intelligence Index</div>",
            "cost" => "<div>c = 100 × (cost ÷ cost reference)</div>",
            _ => "<div>t = 100 × (tokens ÷ token reference)</div>",
        })
        .collect::<String>();
    modal(
        "Distance to perfection",
        &format!(
            r#"<div class="method-content"><p>Intelligence Index is always included. Add total cost or output tokens for a 2D graph, or both for a 3D graph. Each metric has its own axis. The ideal model scores 100, costs $0, and uses 0 output tokens.</p><div class="formula"><span>d = √({terms})</span><div>{formula}</div></div><p>Distance, rankings, and exports use exactly the selected metrics. The sweep expands a circle in 2D and a sphere in 3D, reaching models in distance order.</p><p>Every axis stays between 0 and 100, with 100 at the ideal end. The 2D graph is a square and the 3D graph is a cube, using equal scales on every axis. Cost and token scores equal 100 minus their normalized shortfall. Hover a point for original measurements. In 2D, improvement moves up and right. Drag the 3D graph to rotate it.</p><h3>Normalization</h3><p>Defaults use the largest published cost and token count among models with complete evaluation data. References stay fixed across metric selections and filters. Zero resource use maps to 100 and the reference maps to 0. Models whose normalized shortfall exceeds 100 on any selected metric are excluded from this graph, its ranking, and its selection exports. Exactly 100 remains included. A larger reference reduces that resource's penalty.</p><div class="scale-fields"><label>Cost reference · USD<input id="method-cost" data-field="method-cost" type="number" min="0.000001" step="any" value="{}"></label><label>Token reference · total tokens<input id="method-tokens" data-field="method-tokens" type="number" min="1" step="any" value="{}"></label></div>{}<div class="method-data"><h3>About the data</h3><p>Artificial Analysis Intelligence Index v{version}. Evaluation totals include reasoning and answer output tokens. {description}, in seconds.</p><p>Original numeric precision is preserved in calculations and exports. Hover a chart or table value, or open a model for the original values. The saved dataset contains actual Intelligence Index scores only. Distance rankings include models with published values for each selected metric.</p><p>The local snapshot is refreshed from data embedded in the public pages. Contributor entries use the same measured scores and token usage with a calculated cost at contributor rates. The raw export includes the published source fields and contributor calculations.</p>{}</div></div><div class="modal-footer">{}{}</div>"#,
            html(&a.method_cost),
            html(&a.method_tokens),
            btn(
                "method-default",
                "",
                &(icon("RefreshCw", 13, "") + "Use dataset maximums"),
                "text-button",
                ""
            ),
            link_size(
                s(&a.snapshot, "sourceUrl"),
                "View the public source",
                13,
                ""
            ),
            btn("method-restore", "", "Restore defaults", "text-button", ""),
            btn(
                "method-apply",
                "",
                &("Apply settings".to_string() + &icon("Check", 15, "")),
                "button accent-button",
                if valid { "" } else { "disabled" }
            )
        ),
        false,
    )
}
pub fn detail(a: &App) -> String {
    let Some(m) = a.models().iter().find(|m| s(m, "id") == a.detail) else {
        return String::new();
    };
    let mut rows = ["intelligence", "cost", "tokens", "time"]
        .iter()
        .map(|k| {
            format!(
                r#"<div><dt>{}</dt><dd class="mono">{}</dd></div>"#,
                metric_label(k),
                html(&precision(n(m, k), k))
            )
        })
        .collect::<String>();
    if let Some(d) = distance(m, a.scales(), &a.distance_metrics()) {
        rows += &format!(
            r#"<div class="detail-distance"><dt>Distance to perfection<small class="detail-distance-metrics">{}</small></dt><dd class="mono">{}</dd></div>"#,
            a.distance_metrics()
                .iter()
                .map(|m| metric_label(m))
                .collect::<Vec<_>>()
                .join(" · "),
            precision(Some(d), "")
        );
    }
    let mut props = String::new();
    if parameter_model(m) {
        for (key, label) in [
            ("parameters", "Total parameters"),
            ("activeParameters", "Active parameters per token"),
        ] {
            if key == "activeParameters" && n(m, key).is_none() {
                continue;
            }
            let val = precision(n(m, key), key);
            let source = &m["parameterSources"][key];
            let v = if source.is_object() && n(m, key).is_some() {
                link_size(
                    s(source, "url"),
                    &val,
                    12,
                    &format!(r#"title="{}""#, html(s(source, "label"))),
                )
            } else {
                html(&val)
            };
            props += &format!(r#"<div><dt>{label}</dt><dd class="mono">{v}</dd></div>"#);
        }
        if !s(m, "weightsUrl").is_empty() {
            props += &format!(
                "<div><dt>Model weights</dt><dd>{}</dd></div>",
                link(
                    s(m, "weightsUrl"),
                    if s(m, "weightsUrl").contains("//huggingface.co/") {
                        "Hugging Face"
                    } else {
                        "Download weights"
                    }
                )
            );
        }
    }
    let token_price =
        if n(m, "priceInputPerMillion").is_some() && n(m, "priceOutputPerMillion").is_some() {
            format!(
                "{}/{}/mT I/O",
                display(n(m, "priceInputPerMillion"), "cost", true),
                display(n(m, "priceOutputPerMillion"), "cost", true)
            )
        } else {
            "Unreported".into()
        };
    let license = if !s(m, "licenseUrl").is_empty() {
        link(
            s(m, "licenseUrl"),
            if s(m, "license").is_empty() {
                "License terms"
            } else {
                s(m, "license")
            },
        )
    } else {
        html(if !s(m, "license").is_empty() {
            s(m, "license")
        } else if b(m, "openWeights") {
            "See source"
        } else {
            "Proprietary"
        })
    };
    for (label, value, mono) in [
        ("Token price", html(&token_price), true),
        (
            "Weights",
            if b(m, "openWeights") {
                "Open weights"
            } else {
                "Proprietary"
            }
            .into(),
            false,
        ),
        ("License", license, false),
        (
            "Reasoning",
            if b(m, "reasoning") {
                "Reasoning model"
            } else {
                "Standard model"
            }
            .into(),
            false,
        ),
        (
            "Inputs",
            [
                "Text",
                if b(m, "image") { "Image" } else { "" },
                if b(m, "video") { "Video" } else { "" },
                if b(m, "speech") { "Audio" } else { "" },
            ]
            .into_iter()
            .filter(|v| !v.is_empty())
            .collect::<Vec<_>>()
            .join(" · "),
            false,
        ),
        (
            "Status",
            if b(m, "deprecated") {
                "Legacy"
            } else {
                "Current"
            }
            .into(),
            false,
        ),
        (
            "Context window",
            n(m, "context")
                .filter(|v| *v != 0.)
                .map(|v| format_number(v, 0))
                .unwrap_or("Unreported".into()),
            false,
        ),
    ] {
        props += &format!(
            r#"<div><dt>{label}</dt><dd{}>{value}</dd></div>"#,
            if mono { r#" class="mono""# } else { "" }
        );
    }
    for (key, label) in [
        ("reasoningTokens", "Reasoning output tokens"),
        ("answerTokens", "Answer output tokens"),
    ] {
        if n(m, key).is_some() {
            props += &format!(
                r#"<div><dt>{label}</dt><dd class="mono">{}</dd></div>"#,
                display(n(m, key), "tokens", true)
            );
        }
    }
    let p = &m["pricing"];
    let pricing = if p.is_object() {
        format!(r#"<div class="detail-pricing"><p>Score, output tokens and timing come from {}. {}</p><dl class="detail-properties">{}</dl>{}</div>"#,html(s(p,"sourceModelName")),if p["breakdown"].is_object(){"Cost uses the same run at contributor rates."}else{"Cost calculation awaits the source cache breakdown."},[("nonCacheInput","Uncached input"),("cacheRead","Cache reads"),("cacheWrite","Cache writes"),("output","Output including reasoning")].iter().map(|(k,label)|format!(r#"<div><dt>{label}<small>{} per million tokens</small></dt><dd class="mono">{}</dd></div>"#,precision(num(p,&["ratesPerMillion",k]),"cost"),if p["breakdown"].is_object(){precision(num(p,&["breakdown",k]),"cost")}else{"Pending".into()})).collect::<String>(),link(s(p,"sourceUrl"),"Contributor pricing"))
    } else {
        String::new()
    };
    modal(
        s(m, "name"),
        &format!(
            r#"<div class="detail-content"><div class="detail-provider">{}<span class="swatch" style="background:{}"></span>{}<span class="muted">{}</span></div><dl class="detail-metrics">{rows}</dl><dl class="detail-properties">{props}</dl>{pricing}</div><div class="modal-footer">{}</div>"#,
            logo(m, 25),
            html(s(m, "color")),
            html(s(m, "provider")),
            if s(m, "releaseDate").is_empty() {
                String::new()
            } else {
                visual::date(&format!("{}T12:00:00", s(m, "releaseDate")), false)
            },
            link_size(
                s(m, "sourceUrl"),
                "View on Artificial Analysis",
                14,
                r#"class="button""#
            )
        ),
        false,
    )
}
