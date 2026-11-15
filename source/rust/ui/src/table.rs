use crate::visual::{btn, icon};
use crate::*;
pub fn render(a: &App) -> String {
    let keys = if a.size_only() {
        vec!["intelligence", "parameters", "activeParameters"]
    } else {
        METRICS[..4].to_vec()
    };
    let effective = a.effective();
    let selected = a
        .models()
        .iter()
        .filter(|m| effective.contains(s(m, "id")))
        .collect::<Vec<_>>();
    let ranges = METRICS
        .iter()
        .map(|key| (*key, bar::ranges(&selected, key)))
        .collect::<HashMap<_, _>>();
    let penalty = |m: &Value, key: &str| bar::penalty(n(m, key), key, ranges[key]);
    let average = |m: &Value| {
        let values = METRICS[..4]
            .iter()
            .filter_map(|key| penalty(m, key))
            .collect::<Vec<_>>();
        if values.len() == 4 {
            Some(values.iter().sum::<f64>() / 4.)
        } else {
            None
        }
    };
    let mut models = a.visible();
    models.sort_by(|a_model, b_model| {
        if a.sort == "name" {
            let cmp = visual::compare(s(a_model, "name"), s(b_model, "name"));
            if a.descending { cmp.reverse() } else { cmp }
        } else {
            let va = if a.sort == "average" {
                average(a_model)
            } else {
                n(a_model, &a.sort)
            };
            let vb = if a.sort == "average" {
                average(b_model)
            } else {
                n(b_model, &a.sort)
            };
            match (va, vb) {
                (None, _) => std::cmp::Ordering::Greater,
                (_, None) => std::cmp::Ordering::Less,
                (Some(x), Some(y)) => {
                    let cmp = (x - y).total_cmp(&0.);
                    if a.descending { cmp.reverse() } else { cmp }
                }
            }
        }
    });
    let labels = score_labels(models.iter().filter_map(|m| n(m, "intelligence")));
    let heading = |key: &str, text: &str, title: &str| {
        let sort_icon = if a.sort == key {
            icon("ChevronDown", 12, if a.descending { "" } else { "flipped" })
        } else {
            String::new()
        };
        btn("sort", key, &(text.to_string() + &sort_icon), "", title)
    };
    let mut rows = String::new();
    for (i, m) in models.iter().enumerate() {
        let mut cells = String::new();
        for key in &keys {
            let score = penalty(m, key);
            let value = n(m, key);
            let style = score
                .map(|v| {
                    let (bg, fg) = bar::heat(v);
                    format!("background-color:{bg};color:{fg}")
                })
                .unwrap_or_default();
            let title = if let Some(score) = score {
                format!(
                    "{} · selected-set penalty {} of 100",
                    precision(value, key),
                    format_number(score, 1)
                )
            } else {
                format!("{} unreported", metric_label(key))
            };
            let label = if *key == "intelligence" {
                labels[&n(m, key).unwrap().to_bits()].clone()
            } else {
                display(value, key, true)
            };
            cells.push_str(&format!("<td class=\"numeric mono heat-cell\"><span class=\"heat-box {}\" style=\"{}\" title=\"{}\">{}</span></td>",if score.is_none(){"is-missing"}else{""},html(&style),html(&title),html(&label)));
        }
        if !a.size_only() {
            let avg = average(m);
            let cell = if let Some(value) = avg {
                let (bg, fg) = bar::heat(value);
                format!(
                    "<span class=\"heat-box\" style=\"background-color:{bg};color:{fg}\" title=\"Average selected-set penalty {} of 100\">{}</span>",
                    format_number(value, 1),
                    format_number(value, 0)
                )
            } else {
                "<span class=\"heat-box is-missing\" title=\"A four-metric average requires all four published values\">—</span>".into()
            };
            cells.push_str(&format!("<td class=\"numeric mono heat-cell average-cell\">{cell}</td><td class=\"muted weights-cell\">{}</td>",if b(m,"openWeights"){"Open"}else{"Proprietary"}));
        }
        rows.push_str(&format!("<tr><td class=\"rank-col mono muted\">{:02}</td><td>{}</td>{cells}<td>{}</td></tr>",i+1,btn("detail",s(m,"id"),&format!("<span class=\"provider-line\" style=\"background:{}\"></span>{}<span>{}<small>{}</small></span>",html(s(m,"color")),render::logo(m,18),html(s(m,"name")),html(s(m,"provider"))),"table-model",""),btn("detail",s(m,"id"),&icon("ArrowUpRight",15,""),"icon-button",&format!("aria-label=\"Inspect {}\"",html(s(m,"name"))))));
    }
    format!(
        "<div class=\"heatmap-table\"><div class=\"heatmap-key\" aria-label=\"Heat map scale from zero best to one hundred worst across the selected models\"><span>Selected models</span><span class=\"mono\">0 best</span><i aria-hidden=\"true\"></i><span class=\"mono\">100 worst</span></div><div class=\"table-scroll\" data-scroll=\"table\"><table class=\"data-table\"><thead><tr><th class=\"rank-col\">#</th><th>{}</th>{}{}<th class=\"table-action\"></th></tr></thead><tbody>{rows}</tbody></table>{}</div></div>",
        heading("name", "Model", ""),
        keys.iter()
            .map(|key| format!(
                "<th class=\"numeric\">{}</th>",
                heading(key, metric_label(key), "")
            ))
            .collect::<String>(),
        if a.size_only() {
            String::new()
        } else {
            format!(
                "<th class=\"numeric average-heading\">{}</th><th>Weights</th>",
                heading(
                    "average",
                    "4-metric avg",
                    "title=\"Average normalized penalty across all four metrics\""
                )
            )
        },
        if models.is_empty() {
            "<div class=\"empty-state\">Choose models or adjust your filters.</div>"
        } else {
            ""
        }
    )
}
