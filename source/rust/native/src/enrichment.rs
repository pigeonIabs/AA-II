use crate::data::*;
use anyhow::{Result, anyhow};
use index_core::*;
use regex::Regex;
use scraper::{Html, Selector};
use std::{
    collections::{HashMap, HashSet},
    thread,
};

const FIELDS: [&str; 2] = ["parameters", "activeParameters"];
#[derive(Clone)]
struct Measurement {
    value: f64,
    resolution: f64,
}
struct Candidate {
    values: HashMap<String, Measurement>,
    method: &'static str,
    label: &'static str,
    url: String,
}
fn norm(value: &str) -> String {
    value
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}
fn repository(value: &str) -> Option<String> {
    let url = url::Url::parse(value).ok()?;
    let parts: Vec<_> = url.path_segments()?.filter(|v| !v.is_empty()).collect();
    (url.host_str() == Some("huggingface.co")
        && parts.len() >= 2
        && !["collections", "datasets", "spaces"].contains(&parts[0]))
    .then(|| format!("{}/{}", parts[0], parts[1]))
}
fn manual(model: &Value) -> bool {
    FIELDS
        .iter()
        .any(|key| text(model, &["parameterSources", key, "method"]) == "manual-size-fallback")
}
fn resolution(value: Option<f64>) -> f64 {
    let Some(value) = value.filter(|v| *v > 0. && v.is_finite()) else {
        return f64::INFINITY;
    };
    let source = value.to_string();
    let parts: Vec<_> = source.split(['e', 'E']).collect();
    let exponent = parts
        .get(1)
        .and_then(|v| v.parse::<i32>().ok())
        .unwrap_or(0);
    let decimal: Vec<_> = parts[0].split('.').collect();
    let whole = decimal[0].trim_start_matches('0');
    let fraction = decimal.get(1).copied().unwrap_or("");
    let exp = exponent
        + if !whole.is_empty() {
            whole.len() as i32 - 1
        } else {
            -(fraction.chars().take_while(|c| *c == '0').count() as i32) - 1
        };
    let length = format!("{whole}{fraction}")
        .trim_start_matches('0')
        .trim_end_matches('0')
        .len();
    10_f64.powi(exp - length as i32 + 1)
}
fn measurement(text: &str) -> Option<Measurement> {
    let stripped = Regex::new("[*`~≈,]").unwrap().replace_all(text, "");
    let regex = Regex::new(
        r"(?i)^(\d+(?:\.\d+)?)\s*(trillion|billion|million|[TBM])(?:\s+(?:parameters?|params?))?$",
    )
    .unwrap();
    let capture = regex.captures(stripped.trim())?;
    let factor = match capture[2].chars().next()?.to_ascii_lowercase() {
        't' => 1000.,
        'm' => 0.001,
        _ => 1.,
    };
    let value = capture[1].parse::<f64>().ok()? * factor;
    if value <= 0. || !value.is_finite() {
        return None;
    }
    let decimals = capture[1].split('.').nth(1).map(str::len).unwrap_or(0);
    Some(Measurement {
        value,
        resolution: factor / 10_f64.powi(decimals as i32),
    })
}
fn field(label: &str) -> Option<&'static str> {
    let clean = Regex::new(r"\s+")
        .unwrap()
        .replace_all(
            &Regex::new("[*`#:()]")
                .unwrap()
                .replace_all(&label.to_lowercase(), ""),
            " ",
        )
        .trim()
        .to_string();
    if Regex::new(r"^(?:(?:total|model) (?:number of )?)?(?:parameters?|params?)(?: total)?$")
        .unwrap()
        .is_match(&clean)
        || clean == "model size"
    {
        Some("parameters")
    } else if Regex::new(
        r"^(?:active|activated|activating) (?:parameters?|params?)(?: per token)?$",
    )
    .unwrap()
    .is_match(&clean)
        || Regex::new(r"^(?:parameters?|params?) (?:active|activated)(?: per token)?$")
            .unwrap()
            .is_match(&clean)
    {
        Some("activeParameters")
    } else {
        None
    }
}
fn extract(
    rows: &[Vec<String>],
    model: &Value,
    aliases: &HashSet<String>,
) -> HashMap<String, Measurement> {
    let mut found = HashMap::new();
    let mut headers: Vec<String> = Vec::new();
    for cells in rows {
        if cells.is_empty() {
            headers.clear();
            continue;
        }
        let Some(field) = field(&cells[0]) else {
            if cells.iter().any(|v| aliases.contains(&norm(v))) {
                headers = cells.clone();
            }
            continue;
        };
        let baseline = n(model, field);
        let values: Vec<_> = cells
            .iter()
            .enumerate()
            .filter_map(|(i, v)| measurement(v).map(|m| (i, m)))
            .collect();
        let near =
            |m: &Measurement| baseline.is_none_or(|v| v <= 0. || (m.value / v - 1.).abs() < 0.15);
        let offset = if headers.first().is_some_and(|v| {
            !v.is_empty()
                && !Regex::new(
                    r"(?i)^(?:model|architecture|property|specification|parameter|configuration)",
                )
                .unwrap()
                .is_match(v)
        }) {
            -1
        } else {
            0
        };
        let candidate = if values.len() == 1 {
            near(&values[0].1).then(|| values[0].1.clone())
        } else {
            let named: Vec<_> = values
                .iter()
                .filter(|(i, m)| {
                    let idx = *i as i32 + offset;
                    idx >= 0
                        && headers
                            .get(idx as usize)
                            .is_some_and(|v| aliases.contains(&norm(v)))
                        && near(m)
                })
                .collect();
            if named.len() == 1 {
                Some(named[0].1.clone())
            } else {
                let nearby: Vec<_> = values
                    .iter()
                    .filter(|(_, m)| baseline.is_some_and(|v| v > 0.) && near(m))
                    .collect();
                (nearby.len() == 1).then(|| nearby[0].1.clone())
            }
        };
        if let Some(value) = candidate {
            if found
                .get(field)
                .is_none_or(|old: &Measurement| value.resolution < old.resolution)
            {
                found.insert(field.to_string(), value);
            }
        }
    }
    found
}
fn card_rows(card: &str) -> Vec<Vec<String>> {
    let doc = Html::parse_document(card);
    let mut rows = Vec::new();
    for table in doc.select(&Selector::parse("table").unwrap()) {
        for row in table.select(&Selector::parse("tr").unwrap()) {
            rows.push(
                row.select(&Selector::parse("th,td").unwrap())
                    .map(|cell| cell.text().collect::<String>().trim().to_string())
                    .collect(),
            );
        }
        rows.push(vec![]);
    }
    for line in card.lines().filter(|line| line.contains('|')) {
        rows.push(
            line.trim()
                .trim_start_matches('|')
                .trim_end_matches('|')
                .split('|')
                .map(|cell| cell.trim().to_string())
                .collect(),
        );
    }
    rows
}
fn report_links(card: &str, root: &str) -> Vec<String> {
    let doc = Html::parse_document(card);
    let mut links: Vec<(String, String)> = doc
        .select(&Selector::parse("a[href]").unwrap())
        .map(|v| {
            (
                v.value().attr("href").unwrap_or("").to_string(),
                v.text().collect(),
            )
        })
        .collect();
    for capture in Regex::new(r"\[([^\]]+)\]\(([^\s)]+)\)")
        .unwrap()
        .captures_iter(card)
    {
        links.push((capture[2].to_string(), capture[1].to_string()));
    }
    let root = url::Url::parse(root).unwrap();
    let mut out = Vec::new();
    for (link, label) in links {
        if !Regex::new("(?i)report|paper").unwrap().is_match(&label) {
            continue;
        }
        let Ok(mut url) = root.join(&link) else {
            continue;
        };
        if url.scheme() != "https"
            || ![
                "github.com",
                "raw.githubusercontent.com",
                "huggingface.co",
                "arxiv.org",
            ]
            .contains(&url.host_str().unwrap_or(""))
        {
            continue;
        }
        if url.host_str() == Some("arxiv.org") {
            if let Some(c) = Regex::new(r"/(?:abs|pdf|html)/(\d{4}\.\d{4,5}(?:v\d+)?)")
                .unwrap()
                .captures(url.path())
            {
                out.push(format!("https://arxiv.org/pdf/{}", &c[1]));
            }
        } else if url.path().to_lowercase().ends_with(".pdf") {
            if url.host_str() == Some("github.com") {
                let _ = url.set_host(Some("raw.githubusercontent.com"));
                url.set_path(&url.path().replacen("/blob/", "/", 1));
            }
            out.push(url.to_string());
        }
    }
    out.truncate(1);
    out
}

fn pdf_rows(bytes: &[u8], remote: &Remote) -> Result<Vec<Vec<String>>> {
    use lopdf::{Document, Object};
    let doc = Document::load_mem(bytes)?;
    let mut rows = Vec::new();
    fn number(value: &Object) -> f64 {
        value
            .as_i64()
            .map(|v| v as f64)
            .or_else(|_| value.as_float().map(|v| v as f64))
            .unwrap_or(0.)
    }
    for (_, page_id) in doc.get_pages().into_iter().take(20) {
        remote.check()?;
        let fonts = doc.get_page_fonts(page_id)?;
        let encodings: HashMap<_, _> = fonts
            .iter()
            .filter_map(|(name, font)| {
                font.get_font_encoding(&doc)
                    .ok()
                    .map(|encoding| (name.clone(), encoding))
            })
            .collect();
        let content = doc.get_and_decode_page_content(page_id)?;
        let (mut font, mut size, mut x, mut y, mut line_x, mut line_y, mut leading, mut scale) =
            (Vec::new(), 12., 0., 0., 0., 0., 0., 1.);
        let mut pieces: Vec<(f64, f64, f64, String)> = Vec::new();
        let mut matrix = [1., 0., 0., 1., 0., 0.];
        let mut graphics = [1., 0., 0., 1., 0., 0.];
        let mut stack = Vec::new();
        fn product(a: [f64; 6], b: [f64; 6]) -> [f64; 6] {
            [
                a[0] * b[0] + a[2] * b[1],
                a[1] * b[0] + a[3] * b[1],
                a[0] * b[2] + a[2] * b[3],
                a[1] * b[2] + a[3] * b[3],
                a[0] * b[4] + a[2] * b[5] + a[4],
                a[1] * b[4] + a[3] * b[5] + a[5],
            ]
        }
        for operation in content.operations {
            let values = &operation.operands;
            match operation.operator.as_str() {
                "q" => stack.push(graphics),
                "Q" => {
                    if let Some(value) = stack.pop() {
                        graphics = value;
                    }
                }
                "cm" if values.len() == 6 => {
                    graphics = product(graphics, std::array::from_fn(|i| number(&values[i])));
                }
                "BT" => {
                    matrix = [1., 0., 0., 1., 0., 0.];
                    x = 0.;
                    y = 0.;
                    line_x = 0.;
                    line_y = 0.;
                }
                "Tf" => {
                    if let Some(value) = values.first().and_then(|v| v.as_name().ok()) {
                        font = value.to_vec();
                    }
                    if let Some(value) = values.get(1) {
                        size = number(value);
                    }
                }
                "TL" => {
                    leading = values.first().map(number).unwrap_or(0.);
                }
                "Tz" => {
                    scale = values.first().map(number).unwrap_or(100.) / 100.;
                }
                "Tm" if values.len() == 6 => {
                    matrix = std::array::from_fn(|i| number(&values[i]));
                    x = 0.;
                    y = 0.;
                    line_x = 0.;
                    line_y = 0.;
                }
                "Td" | "TD" if values.len() >= 2 => {
                    let (dx, dy) = (number(&values[0]), number(&values[1]));
                    line_x += dx;
                    line_y += dy;
                    x = line_x;
                    y = line_y;
                    if operation.operator == "TD" {
                        leading = -dy;
                    }
                }
                "T*" => {
                    line_y -= leading;
                    x = line_x;
                    y = line_y;
                }
                "Tj" | "TJ" | "'" | "\"" => {
                    if operation.operator == "'" || operation.operator == "\"" {
                        line_y -= leading;
                        x = line_x;
                        y = line_y;
                    }
                    let items: Vec<&Object> = if operation.operator == "TJ" {
                        values
                            .first()
                            .and_then(|v| v.as_array().ok())
                            .map(|v| v.iter().collect())
                            .unwrap_or_default()
                    } else if operation.operator == "\"" {
                        values.get(2).into_iter().collect()
                    } else {
                        values.iter().collect()
                    };
                    for value in items {
                        match value {
                            Object::String(raw, _) => {
                                let Some(encoding) = encodings.get(&font) else {
                                    continue;
                                };
                                let text = encoding.bytes_to_string(raw)?;
                                let mut width = 0.;
                                if let Some(dictionary) = fonts.get(&font) {
                                    let first = dictionary
                                        .get(b"FirstChar")
                                        .ok()
                                        .and_then(|v| v.as_i64().ok())
                                        .unwrap_or(0);
                                    let widths = dictionary
                                        .get(b"Widths")
                                        .ok()
                                        .and_then(|v| v.as_array().ok());
                                    for byte in raw {
                                        let glyph = widths
                                            .and_then(|v| {
                                                (*byte as i64 - first)
                                                    .try_into()
                                                    .ok()
                                                    .and_then(|i: usize| v.get(i))
                                            })
                                            .map(number)
                                            .unwrap_or(500.);
                                        width += glyph * size / 1000.;
                                    }
                                } else {
                                    width = text.chars().count() as f64 * size * 0.5;
                                }
                                let transform = product(graphics, matrix);
                                let px = transform[0] * x + transform[2] * y + transform[4];
                                let py = transform[1] * x + transform[3] * y + transform[5];
                                if !text.trim().is_empty() {
                                    pieces.push((px, py, px + width * scale * transform[0], text));
                                }
                                x += width * scale;
                            }
                            Object::Integer(_) | Object::Real(_) => {
                                x -= number(value) / 1000. * size * scale
                            }
                            _ => {}
                        }
                    }
                }
                _ => {}
            }
        }
        pieces.sort_by(|a, b| {
            (b.1 / 2.)
                .round()
                .total_cmp(&(a.1 / 2.).round())
                .then(a.0.total_cmp(&b.0))
        });
        let mut current_y = f64::NAN;
        let mut cells: Vec<String> = Vec::new();
        let mut edge = f64::NEG_INFINITY;
        for (px, py, end, text) in pieces {
            let py = (py / 2.).round() * 2.;
            if py != current_y {
                if !cells.is_empty() {
                    rows.push(std::mem::take(&mut cells));
                }
                current_y = py;
                edge = f64::NEG_INFINITY;
            }
            if px - edge > 12. || cells.is_empty() {
                cells.push(text);
            } else {
                let last = cells.last_mut().unwrap();
                if !last.ends_with(' ') && !text.starts_with(' ') {
                    last.push(' ');
                }
                last.push_str(&text);
            }
            edge = end;
        }
        if !cells.is_empty() {
            rows.push(cells);
        }
        rows.push(Vec::new());
    }
    Ok(rows)
}

fn reuse(model: &Value, cached: &Value) -> Value {
    let mut result = json!({"parameters":model["parameters"],"activeParameters":model["activeParameters"],"parameterSources":{},"parameterRevision":cached["parameterRevision"],"parameterEnrichmentVersion":cached["parameterEnrichmentVersion"]});
    for field in FIELDS {
        let source = &cached["parameterSources"][field];
        let precision = resolution(n(model, field));
        let refined = resolution(n(cached, field)) < precision
            && n(cached, field)
                .zip(n(model, field))
                .is_some_and(|(a, b)| (a / precision).round() == (b / precision).round());
        if [
            "publisher-model-card",
            "publisher-report",
            "hugging-face-tensors",
        ]
        .contains(&s(source, "method"))
            && n(cached, field).is_some_and(|v| v > 0.)
            && (n(model, field).is_none_or(|v| v <= 0.) || refined)
        {
            result[field] = cached[field].clone();
            result["parameterSources"][field] = source.clone();
        } else if n(model, field).is_some_and(|v| v > 0.) {
            result["parameterSources"][field] = json!({"method":"artificial-analysis","label":"Artificial Analysis","url":model["sourceUrl"],"checkedAt":now(),"valueBillions":model[field]});
        }
    }
    result
}
fn enrich_repository(
    remote: &Remote,
    models: &[Value],
    previous: &HashMap<String, Value>,
) -> Result<HashMap<String, Value>> {
    let repo = repository(s(&models[0], "weightsUrl"))
        .ok_or_else(|| anyhow!("The publisher repository is unavailable."))?;
    let info = remote.object(&format!("https://huggingface.co/api/models/{repo}"))?;
    let revision = s(&info, "sha");
    let mut output = HashMap::new();
    if !Regex::new("^[a-fA-F0-9]{40}$").unwrap().is_match(revision)
        || !s(&info, "id").eq_ignore_ascii_case(&repo)
    {
        return Ok(output);
    }
    let mut pending = Vec::new();
    for model in models {
        let cached = previous.get(s(model, "id"));
        let fresh = cached.is_some_and(|cached| {
            !manual(cached)
                && n(cached, "parameterEnrichmentVersion") == Some(1.)
                && s(cached, "parameterRevision") == revision
                && s(cached, "weightsUrl") == s(model, "weightsUrl")
                && chrono::DateTime::parse_from_rfc3339(text(
                    cached,
                    &["parameterSources", "parameters", "checkedAt"],
                ))
                .is_ok_and(|date| {
                    chrono::Utc::now().signed_duration_since(date).num_seconds() < 86400
                })
        });
        if fresh {
            output.insert(s(model, "id").to_string(), reuse(model, cached.unwrap()));
        } else {
            pending.push(model);
        }
    }
    if pending.is_empty() {
        return Ok(output);
    }
    let root = format!("https://huggingface.co/{repo}/resolve/{revision}/");
    let card = String::from_utf8(remote.publisher(&format!("{root}README.md"), 2_000_000)?)?;
    let rows = card_rows(&card);
    let mut reports = Vec::new();
    for url in report_links(&card, &root) {
        if let Ok(bytes) = remote.publisher(&url, 25_000_000) {
            if let Ok(rows) = pdf_rows(&bytes, remote) {
                reports.push((url, rows));
            }
        }
        remote.check()?;
    }
    let checked = now();
    for model in pending {
        let aliases: HashSet<_> = [
            repo.split('/').nth(1).unwrap_or(""),
            text(model, &["release", "name"]),
            &Regex::new(r"\s*\([^)]*\)\s*$")
                .unwrap()
                .replace(s(model, "name"), ""),
        ]
        .iter()
        .filter(|v| !v.is_empty())
        .map(|v| norm(v))
        .collect();
        let mut candidates = vec![
            Candidate {
                values: FIELDS
                    .iter()
                    .filter_map(|field| {
                        n(model, field).filter(|v| *v > 0.).map(|v| {
                            (
                                field.to_string(),
                                Measurement {
                                    value: v,
                                    resolution: resolution(Some(v)),
                                },
                            )
                        })
                    })
                    .collect(),
                method: "artificial-analysis",
                label: "Artificial Analysis",
                url: s(model, "sourceUrl").to_string(),
            },
            Candidate {
                values: extract(&rows, model, &aliases),
                method: "publisher-model-card",
                label: "Publisher model card",
                url: format!("{root}README.md"),
            },
        ];
        for (url, rows) in &reports {
            candidates.push(Candidate {
                values: extract(rows, model, &aliases),
                method: "publisher-report",
                label: "Publisher technical report",
                url: url.clone(),
            });
        }
        let published = candidates
            .iter()
            .filter_map(|candidate| candidate.values.get("parameters"))
            .min_by(|a, b| a.resolution.total_cmp(&b.resolution));
        let total = num(&info, &["safetensors", "total"]);
        let tensor_types: Vec<_> = info["safetensors"]["parameters"]
            .as_object()
            .map(|m| m.keys().map(String::as_str).collect())
            .unwrap_or_default();
        let quantized = nullish(&[
            &info["config"]["quantization_config"],
            &info["config"]["text_config"]["quantization_config"],
        ]);
        if let (Some(total), Some(published)) = (total.filter(|v| *v > 0.), published) {
            if !tensor_types.is_empty()
                && tensor_types
                    .iter()
                    .all(|v| ["BF16", "F16", "F32", "F64"].contains(v))
                && (quantized.is_null() || quantized.as_bool() == Some(false))
                && (total / 1e9 / published.resolution).round()
                    == (published.value / published.resolution).round()
            {
                candidates.push(Candidate {
                    values: HashMap::from([(
                        "parameters".into(),
                        Measurement {
                            value: total / 1e9,
                            resolution: 1e-9,
                        },
                    )]),
                    method: "hugging-face-tensors",
                    label: "Hugging Face tensor metadata",
                    url: format!("https://huggingface.co/api/models/{repo}/revision/{revision}"),
                });
            }
        }
        let mut result = json!({"parameters":model["parameters"],"activeParameters":model["activeParameters"],"parameterSources":{},"parameterRevision":revision,"parameterEnrichmentVersion":1});
        for field in FIELDS {
            if let Some(candidate) = candidates
                .iter()
                .filter(|candidate| candidate.values.contains_key(field))
                .min_by(|a, b| {
                    a.values[field]
                        .resolution
                        .total_cmp(&b.values[field].resolution)
                })
            {
                let value = candidate.values[field].value;
                result[field] = json!(value);
                result["parameterSources"][field] = json!({"method":candidate.method,"label":candidate.label,"url":candidate.url,"revision":revision,"checkedAt":checked,"valueBillions":value});
            } else {
                result["parameterSources"][field] = json!({"method":"artificial-analysis","label":"Artificial Analysis","url":model["sourceUrl"],"checkedAt":checked,"valueBillions":model[field]});
            }
        }
        output.insert(s(model, "id").to_string(), result);
    }
    Ok(output)
}
pub fn enrich(remote: &Remote, models: Vec<Value>, previous: &[Value]) -> Result<Vec<Value>> {
    let previous: HashMap<_, _> = previous
        .iter()
        .filter(|m| eligible(m))
        .map(|m| (s(m, "id").to_string(), m.clone()))
        .collect();
    let mut groups: HashMap<String, Vec<Value>> = HashMap::new();
    for model in models.iter().filter(|m| eligible(m) && b(m, "openWeights")) {
        if let Some(repo) = repository(s(model, "weightsUrl")) {
            groups.entry(repo).or_default().push(model.clone());
        }
    }
    let groups: Vec<_> = groups.into_values().collect();
    let mut updates = HashMap::new();
    for batch in groups.chunks(4) {
        remote.check()?;
        let results = thread::scope(|scope| {
            batch
                .iter()
                .map(|group| {
                    let previous = &previous;
                    scope.spawn(move || enrich_repository(remote, group, previous))
                })
                .collect::<Vec<_>>()
                .into_iter()
                .map(|h| h.join().ok().and_then(Result::ok))
                .collect::<Vec<_>>()
        });
        for result in results.into_iter().flatten() {
            updates.extend(result);
        }
    }
    let mut result: Vec<_> = models
        .into_iter()
        .filter(eligible)
        .map(|model| {
            let cached = previous.get(s(&model, "id"));
            let fallback = cached
                .filter(|cached| {
                    b(&model, "openWeights")
                        && repository(s(&model, "weightsUrl")).is_some()
                        && !manual(cached)
                        && s(cached, "weightsUrl") == s(&model, "weightsUrl")
                        && cached["parameterSources"].is_object()
                })
                .map(|cached| reuse(&model, cached));
            merge(
                &model,
                updates
                    .get(s(&model, "id"))
                    .or(fallback.as_ref())
                    .unwrap_or(&Value::Null),
            )
        })
        .collect();
    apply_fallbacks(&mut result, &previous);
    Ok(result)
}
fn apply_fallbacks(models: &mut [Value], previous: &HashMap<String, Value>) {
    let rules: Value = serde_json::from_str(SIZE_RULES).unwrap();
    let references: Vec<_> = models
        .iter()
        .cloned()
        .chain(previous.values().cloned())
        .collect();
    let checked = now();
    for model in models {
        let rule = array(&rules).iter().find(|rule| {
            s(model, "id") == s(rule, "id")
                || s(model, "providerSlug") == s(rule, "providerSlug")
                    && (s(model, "slug") == s(rule, "slug")
                        || text(model, &["release", "slug"]) == s(rule, "slug"))
        });
        let Some(rule) = rule else { continue };
        let reference = references
            .iter()
            .find(|reference| {
                s(reference, "providerSlug") == s(rule, "providerSlug")
                    && s(reference, "slug") == s(rule, "referenceSlug")
            })
            .unwrap_or(&Value::Null);
        if !model["parameterSources"].is_object() {
            model["parameterSources"] = json!({});
        }
        for field in FIELDS {
            let source = &model["parameterSources"][field];
            let value = n(reference, field)
                .filter(|v| *v > 0.)
                .or_else(|| n(rule, field));
            let published = if s(source, "method") == "manual-size-fallback" {
                None
            } else {
                n(model, field)
            };
            let precision = resolution(published);
            let refined = (source.is_null() || s(source, "method") == "artificial-analysis")
                && resolution(value) < precision
                && value
                    .zip(published)
                    .is_some_and(|(a, b)| (a / precision).round() == (b / precision).round());
            if published.is_none_or(|v| v <= 0.) || refined {
                let url =
                    nullish(&[&reference["parameterSources"][field]["url"], &rule["url"]]).clone();
                model[field] = json!(value);
                model["parameterSources"][field] = json!({"method":"manual-size-fallback","label":format!("Manual size fallback using {}",s(rule,"referenceName")),"url":url,"checkedAt":checked,"valueBillions":value,"referenceModelSlug":rule["referenceSlug"]});
            } else if source.is_null() || s(source, "method") == "manual-size-fallback" {
                model["parameterSources"][field] = json!({"method":"artificial-analysis","label":"Artificial Analysis","url":model["sourceUrl"],"checkedAt":checked,"valueBillions":published});
            }
        }
    }
}
