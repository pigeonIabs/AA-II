use crate::render::*;
use crate::visual::{btn, icon};
use crate::*;
use std::collections::BTreeMap;
pub fn matches(a: &App) -> Vec<&Value> {
    let Some(p) = &a.picker else {
        return vec![];
    };
    let query = p.query.trim().to_lowercase();
    let metrics = a.distance_metrics();
    a.models()
        .iter()
        .filter(|m| {
            (!a.size_only() || parameter_model(m))
                && (p.provider == "all" || s(m, "providerSlug") == p.provider)
                && (!p.selected_only || p.draft.contains(s(m, "id")))
                && (!p.complete_only || metrics.iter().all(|k| n(m, k).is_some()))
                && format!(
                    "{} {} {} {}",
                    s(m, "name"),
                    s(m, "fullName"),
                    s(m, "provider"),
                    effort(m).0
                )
                .to_lowercase()
                .contains(&query)
        })
        .collect()
}
pub fn render(a: &App) -> String {
    let p = a.picker.as_ref().unwrap();
    let found = matches(a);
    let ids: HashSet<_> = found.iter().map(|m| s(m, "id")).collect();
    let mut groups: BTreeMap<&str, Vec<(String, String, Vec<&Value>)>> = BTreeMap::new();
    for f in families(a.models()) {
        groups.entry(s(f.2[0], "providerSlug")).or_default().push(f);
    }
    let mut groups: Vec<_> = groups.into_iter().collect();
    let priority = |slug: &str| match slug {
        "openai" => 0,
        "anthropic" => 1,
        _ => 2,
    };
    groups.sort_by(|(a, x), (b, y)| {
        priority(a)
            .cmp(&priority(b))
            .then(
                y.iter()
                    .map(|f| f.2.len())
                    .sum::<usize>()
                    .cmp(&x.iter().map(|f| f.2.len()).sum()),
            )
            .then(visual::compare(
                s(x[0].2[0], "provider"),
                s(y[0].2[0], "provider"),
            ))
    });
    let options: Vec<_> = std::iter::once(("all", "All providers"))
        .chain(
            groups
                .iter()
                .map(|(slug, f)| (*slug, s(f[0].2[0], "provider"))),
        )
        .collect();
    let visible_scores = groups
        .iter()
        .filter(|(slug, _)| p.providers.contains(*slug))
        .flat_map(|(_, groups)| groups.iter())
        .filter(|f| f.2.len() == 1 || p.expanded.contains(&f.0))
        .flat_map(|f| f.2.iter())
        .filter(|m| ids.contains(s(m, "id")))
        .filter_map(|m| n(m, "intelligence"));
    let labels = score_labels(visible_scores);
    let mut rows = String::new();
    for (slug, mut group) in groups {
        let count = group
            .iter()
            .flat_map(|f| &f.2)
            .filter(|m| ids.contains(s(m, "id")))
            .count();
        if count == 0 {
            continue;
        }
        let model = group[0].2[0];
        let open = p.providers.contains(slug);
        rows.push_str(&format!(r#"<div class="picker-provider">{}<div class="picker-provider-models" id="provider-{}" role="group" aria-labelledby="provider-{}-heading" {}>"#,btn("picker-provider",slug,&format!(r#"{}<span class="picker-provider-name">{}</span><span class="picker-provider-count"><span class="mono">{count}</span> <!-- -->{}</span>{}"#,logo(model,22),html(s(model,"provider")),if count==1{"model"}else{"models"},icon("ChevronDown",17,"")),"picker-provider-toggle",&format!(r#"type="button" id="provider-{}-heading" aria-expanded="{open}" aria-controls="provider-{}" {}"#,html(slug),html(slug),if p.saving{"disabled"}else{""})),html(slug),html(slug),if open{""}else{"hidden"}));
        group.sort_by(|x, y| {
            let max = |f: &(String, String, Vec<&Value>)| {
                f.2.iter()
                    .filter(|m| ids.contains(s(m, "id")))
                    .filter_map(|m| n(m, "intelligence"))
                    .fold(f64::NEG_INFINITY, f64::max)
            };
            max(y).total_cmp(&max(x)).then(visual::compare(&x.1, &y.1))
        });
        for (key, name, models) in group {
            let matches: Vec<_> = models.iter().filter(|m| ids.contains(s(m, "id"))).collect();
            if matches.is_empty() {
                continue;
            }
            let selected = models
                .iter()
                .filter(|m| p.draft.contains(s(m, "id")))
                .count();
            let all = selected == models.len();
            let grouped = models.len() > 1;
            let value = if grouped {
                key.clone()
            } else {
                s(models[0], "id").into()
            };
            let selected_attr = if all { "checked" } else { "" };
            let partial = if selected > 0 && !all {
                "data-indeterminate"
            } else {
                ""
            };
            let score = if grouped {
                String::new()
            } else {
                labels
                    .get(&n(models[0], "intelligence").unwrap().to_bits())
                    .cloned()
                    .unwrap_or_default()
            };
            let name = if grouped { &name } else { s(models[0], "name") };
            let checkbox = format!(
                r#"<label class="picker-row {} {}"><input type="checkbox" data-field="{}" value="{}" {selected_attr} {partial} {} aria-label="{}"><span class="picker-model"><span>{}</span><small>{}{}</small></span>{}</label>"#,
                if grouped { "picker-family-select" } else { "" },
                if all { "selected" } else { "" },
                if grouped {
                    "picker-group"
                } else {
                    "picker-model"
                },
                html(&value),
                if p.saving { "disabled" } else { "" },
                html(&if grouped {
                    format!("All reasoning efforts for {name}")
                } else {
                    name.to_string()
                }),
                html(name),
                if b(models[0], "openWeights") {
                    "Open weights"
                } else {
                    "Proprietary"
                },
                if b(models[0], "deprecated") {
                    " · Legacy"
                } else {
                    ""
                },
                if grouped {
                    String::new()
                } else {
                    format!(
                        r#"<span class="picker-score mono" title="{}">{score}</span>"#,
                        precision(n(models[0], "intelligence"), "intelligence")
                    )
                }
            );
            if grouped {
                let expanded = p.expanded.contains(&key);
                rows.push_str(&format!(r#"<div class="picker-family"><div class="picker-family-header {}">{}{}</div><div class="picker-efforts" id="efforts-{}" role="group" aria-label="{} reasoning efforts" {}>{}</div></div>"#,if selected>0{"selected"}else{""},checkbox,btn("picker-family",&key,&icon("ChevronDown",17,""),"picker-expand",&format!(r#"type="button" {} aria-label="{} reasoning efforts for {}" aria-expanded="{expanded}" aria-controls="efforts-{}""#,if p.saving{"disabled"}else{""},if expanded{"Collapse"}else{"Expand"},html(name),html(&key))),html(&key),html(name),if expanded{""}else{"hidden"},matches.iter().map(|m|format!(r#"<label class="picker-row picker-effort-row {}"><input type="checkbox" data-field="picker-model" value="{}" {} {}><span class="picker-model"><span>{}</span></span><span class="picker-score mono" title="{}">{}</span></label>"#,if p.draft.contains(s(m,"id")){"selected"}else{""},html(s(m,"id")),if p.draft.contains(s(m,"id")){"checked"}else{""},if p.saving{"disabled"}else{""},html(&effort(m).0),precision(n(m,"intelligence"),"intelligence"),labels.get(&n(m,"intelligence").unwrap().to_bits()).cloned().unwrap_or_default())).collect::<String>()));
            } else {
                rows.push_str(&checkbox);
            }
        }
        rows.push_str("</div></div>");
    }
    let disabled = if p.saving { "disabled" } else { "" };
    let temp = a.baseline.is_some();
    let search = format!(
        r#"<div class="picker-search">{}<input id="picker-query" data-field="picker-query" autofocus {disabled} placeholder="Search models or providers" aria-label="Search models or providers" value="{}">{}<select data-field="picker-provider" {disabled} aria-label="Filter model picker by provider">{}</select></div>"#,
        icon("Search", 17, ""),
        html(&p.query),
        if p.query.is_empty() {
            String::new()
        } else {
            btn(
                "picker-search-clear",
                "",
                &icon("X", 14, ""),
                "icon-button",
                &format!(r#"{disabled} aria-label="Clear search""#),
            )
        },
        options
            .iter()
            .map(|(v, l)| format!(
                r#"<option value="{}" {}>{}</option>"#,
                html(v),
                if *v == p.provider { "selected" } else { "" },
                html(l)
            ))
            .collect::<String>()
    );
    let content = format!(
        r#"{search}<div class="picker-actions"><div>{}{}{}</div><span class="muted">{}<!-- --> models</span></div><div class="picker-checks">{}{}</div><div class="picker-list" data-scroll="picker">{}</div><div class="modal-footer"><span><strong>{}</strong> models selected</span>{}</div>"#,
        btn("picker-all", "", "Select shown", "text-button", disabled),
        btn(
            "picker-clear",
            "",
            if temp { "Clear all" } else { "Clear shown" },
            "text-button",
            disabled
        ),
        btn(
            "picker-defaults",
            "",
            "AA defaults",
            "text-button",
            disabled
        ),
        found.len(),
        visual::checkbox(
            "picker-selected",
            "",
            "Selected only",
            p.selected_only,
            "",
            ""
        ),
        visual::checkbox(
            "picker-complete",
            "",
            "Complete distance data",
            p.complete_only,
            "",
            ""
        ),
        if rows.is_empty() {
            "<div class=\"empty-state\">Try another model name or provider.</div>"
        } else {
            &rows
        },
        p.draft.len(),
        btn(
            "apply-selection",
            "",
            &format!(
                "{}{}",
                if p.saving {
                    if temp { "Applying" } else { "Saving" }
                } else if temp {
                    "Compare models"
                } else {
                    "Apply selection"
                },
                icon(
                    if p.saving { "LoaderCircle" } else { "Check" },
                    15,
                    if p.saving { "spin" } else { "" }
                )
            ),
            "button accent-button",
            disabled
        )
    );
    let content = if p.save_error.is_empty() {
        content
    } else {
        content.replacen("<div class=\"modal-footer\">",&format!("<p class=\"notice error-notice\" role=\"alert\">{}</p><div class=\"modal-footer\">",html(&p.save_error)),1)
    };
    let content = if p.saving {
        content.replace(
            "<input type=\"checkbox\"",
            "<input disabled type=\"checkbox\"",
        )
    } else {
        content
    };
    modal(
        if temp {
            "Temporary comparison"
        } else {
            "Choose your models"
        },
        &content,
        true,
    )
}
