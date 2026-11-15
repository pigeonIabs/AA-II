pub use serde_json::{Value, json};
pub const POLICY: &str = "published-scores-excluding-zero-cost";

use std::collections::{BTreeMap, HashMap, HashSet};

pub const METRICS: [&str; 6] = [
    "intelligence",
    "cost",
    "tokens",
    "time",
    "parameters",
    "activeParameters",
];
pub const DISTANCE_METRICS: [&str; 3] = ["intelligence", "cost", "tokens"];
pub const SIZE_RULES: &str = include_str!("../../../scripts/model-size-rules.json");
pub fn s<'a>(value: &'a Value, key: &str) -> &'a str {
    value.get(key).and_then(Value::as_str).unwrap_or("")
}
pub fn n(value: &Value, key: &str) -> Option<f64> {
    value
        .get(key)
        .and_then(Value::as_f64)
        .filter(|number| number.is_finite())
}
pub fn b(value: &Value, key: &str) -> bool {
    value.get(key).and_then(Value::as_bool) == Some(true)
}
pub fn at<'a>(value: &'a Value, path: &[&str]) -> &'a Value {
    let mut current = value;
    for key in path {
        current = &current[*key];
    }
    current
}
pub fn text<'a>(value: &'a Value, path: &[&str]) -> &'a str {
    at(value, path).as_str().unwrap_or("")
}
pub fn num(value: &Value, path: &[&str]) -> Option<f64> {
    at(value, path).as_f64().filter(|number| number.is_finite())
}
pub fn array(value: &Value) -> &[Value] {
    value.as_array().map(Vec::as_slice).unwrap_or(&[])
}
pub fn metric_label(metric: &str) -> &'static str {
    match metric {
        "cost" => "Total cost",
        "tokens" => "Output tokens",
        "time" => "Response time",
        "parameters" => "Total parameters",
        "activeParameters" => "Active parameters",
        _ => "Intelligence Index",
    }
}
pub fn parameter_metric(metric: &str) -> bool {
    matches!(metric, "parameters" | "activeParameters")
}
pub fn zero_cost(model: &Value) -> bool {
    n(model, "cost") == Some(0.)
        || n(model, "intelligenceIndexCostTotal") == Some(0.)
        || num(model, &["intelligenceIndexCost", "total"]) == Some(0.)
}
pub fn eligible_source(model: &Value) -> bool {
    n(model, "intelligenceIndex").is_some()
        && !b(model, "intelligenceIndexIsEstimated")
        && !zero_cost(model)
}
pub fn eligible(model: &Value) -> bool {
    n(model, "intelligence").is_some() && !b(model, "estimated") && !zero_cost(model)
}
pub fn parameter_model(model: &Value) -> bool {
    b(model, "openWeights")
        || s(model, "id") == "665c67ca-09eb-48f0-a7a3-0096e1d5de51"
        || s(model, "providerSlug") == "kimi"
            && (s(model, "slug") == "kimi-k3-1-pro"
                || text(model, &["release", "slug"]) == "kimi-k3-1-pro")
}
pub fn pareto_eligible(model: &Value) -> bool {
    eligible(model)
        && ![
            s(model, "slug"),
            s(model, "name"),
            s(model, "fullName"),
            text(model, &["pricing", "modelId"]),
        ]
        .iter()
        .any(|value| {
            let lower = value.to_lowercase().replace('-', " ");
            lower.contains("muse spark") && lower.contains("contributor")
        })
}
pub fn default_filters() -> Value {
    json!({"weights":"all","reasoning":"all","modality":"all","provider":"all","minScore":0})
}
pub fn default_preferences() -> Value {
    json!({"selected":[],"filters":default_filters(),"view":"index","metric":"intelligence","paretoMetric":"cost","paretoLogScale":true,
        "paretoGlobalFrontier":false,"paretoZone":"kill","chartMode":"chart","stackReasoning":false,"fullIntelligenceScale":false,
        "scales":null,"distanceMetrics":DISTANCE_METRICS})
}
pub fn preferences(value: Value) -> Value {
    let mut result = default_preferences();
    if let Some(map) = value.as_object() {
        for (key, item) in map {
            if result.get(key).is_some() {
                result[key] = item.clone();
            }
        }
    }
    for metric in ["metric", "paretoMetric"] {
        if !METRICS.contains(&s(&result, metric))
            || metric == "paretoMetric" && s(&result, metric) == "intelligence"
        {
            result[metric] = json!(if metric == "metric" {
                "intelligence"
            } else {
                "cost"
            });
        }
    }
    if !["index", "perfection", "pareto"].contains(&s(&result, "view")) {
        result["view"] = json!("index");
    }
    let mut filters = default_filters();
    for key in ["weights", "reasoning", "modality", "provider", "minScore"] {
        if !result["filters"][key].is_null() {
            filters[key] = result["filters"][key].clone();
        }
    }
    result["filters"] = filters;
    result["distanceMetrics"] = json!(read_distance_metrics(&result["distanceMetrics"]));
    if num(&result, &["scales", "cost"]).is_none_or(|v| v <= 0.)
        || num(&result, &["scales", "tokens"]).is_none_or(|v| v <= 0.)
    {
        result["scales"] = Value::Null;
    }
    result
}
pub fn read_distance_metrics(value: &Value) -> Vec<String> {
    let selected = array(value);
    let metrics: Vec<_> = DISTANCE_METRICS
        .iter()
        .filter(|metric| {
            **metric == "intelligence"
                || selected.iter().any(|item| item.as_str() == Some(**metric))
        })
        .map(|m| m.to_string())
        .collect();
    if metrics.len() < 2 {
        DISTANCE_METRICS.iter().map(|v| v.to_string()).collect()
    } else {
        metrics
    }
}
pub fn matches_filters(model: &Value, filters: &Value, size_only: bool) -> bool {
    eligible(model)
        && (size_only
            || s(filters, "weights") == "all"
            || b(model, "openWeights") == (s(filters, "weights") == "open"))
        && (s(filters, "reasoning") == "all"
            || b(model, "reasoning") == (s(filters, "reasoning") == "reasoning"))
        && (s(filters, "modality") == "all"
            || (b(model, "image") || b(model, "video") || b(model, "speech"))
                == (s(filters, "modality") == "multimodal"))
        && (s(filters, "provider") == "all" || s(model, "providerSlug") == s(filters, "provider"))
        && n(model, "intelligence").unwrap_or(0.) >= n(filters, "minScore").unwrap_or(0.)
        && (!size_only || parameter_model(model))
}
pub fn shortfall(value: f64, metric: &str, scales: &Value) -> f64 {
    if metric == "intelligence" {
        100. - value
    } else {
        100. * (value / n(scales, metric).unwrap_or(1.))
    }
}
pub fn distance_eligible(model: &Value, scales: &Value, metrics: &[String]) -> bool {
    eligible(model)
        && metrics.iter().all(|metric| {
            let reference = if metric == "intelligence" {
                Some(100.)
            } else {
                n(scales, metric)
            };
            match (n(model, metric), reference) {
                (Some(value), Some(reference)) => {
                    reference > 0. && value >= 0. && value <= reference
                }
                _ => false,
            }
        })
}
pub fn distance(model: &Value, scales: &Value, metrics: &[String]) -> Option<f64> {
    distance_eligible(model, scales, metrics).then(|| {
        metrics
            .iter()
            .map(|metric| shortfall(n(model, metric).unwrap(), metric, scales).powi(2))
            .sum::<f64>()
            .sqrt()
    })
}
pub fn ranked<'a>(
    models: &[&'a Value],
    scales: &Value,
    metrics: &[String],
) -> Vec<(&'a Value, f64)> {
    let mut result: Vec<_> = models
        .iter()
        .filter_map(|model| distance(model, scales, metrics).map(|d| (*model, d)))
        .collect();
    result.sort_by(|(a, x), (b, y)| {
        x.total_cmp(y)
            .then(
                n(b, "intelligence")
                    .unwrap_or(0.)
                    .total_cmp(&n(a, "intelligence").unwrap_or(0.)),
            )
            .then(s(a, "name").cmp(s(b, "name")))
    });
    result
}
pub fn distance_formula(metrics: &[String]) -> String {
    format!(
        "sqrt({})",
        metrics
            .iter()
            .map(|metric| match metric.as_str() {
                "cost" => "(100*(cost/costReference))^2",
                "tokens" => "(100*(tokens/tokenReference))^2",
                _ => "(100-index)^2",
            })
            .collect::<Vec<_>>()
            .join(" + ")
    )
}

fn decimal_parts(value: f64) -> (String, String) {
    let text = value.abs().to_string();
    let parts: Vec<_> = text.split(['e', 'E']).collect();
    let exponent = parts
        .get(1)
        .and_then(|v| v.parse::<i32>().ok())
        .unwrap_or(0);
    let coefficient = parts[0];
    let point = coefficient.find('.').unwrap_or(coefficient.len()) as i32 + exponent;
    let digits = coefficient.replace('.', "");
    if point <= 0 {
        (
            String::from("0"),
            format!("{}{}", "0".repeat((-point) as usize), digits),
        )
    } else if point as usize >= digits.len() {
        (
            format!("{}{}", digits, "0".repeat(point as usize - digits.len())),
            String::new(),
        )
    } else {
        (
            digits[..point as usize].to_string(),
            digits[point as usize..].to_string(),
        )
    }
}
fn grouped(integer: &str) -> String {
    let mut out = String::new();
    for (i, c) in integer.chars().enumerate() {
        if i > 0 && (integer.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}
pub fn format_number(value: f64, places: usize) -> String {
    if !value.is_finite() {
        return "Unreported".into();
    }
    let (whole, mut fraction) = decimal_parts(value);
    fraction.push_str(&"0".repeat((places + 1).saturating_sub(fraction.len())));
    let round = fraction.as_bytes().get(places).copied().unwrap_or(b'0') >= b'5';
    let mut digits = format!("{}{}", whole, &fraction[..places]).into_bytes();
    if round {
        let mut carry = true;
        for digit in digits.iter_mut().rev() {
            if !carry {
                break;
            }
            if *digit == b'9' {
                *digit = b'0';
            } else {
                *digit += 1;
                carry = false;
            }
        }
        if carry {
            digits.insert(0, b'1');
        }
    }
    let point = digits.len() - places;
    let integer = String::from_utf8_lossy(&digits[..point]);
    let decimals = String::from_utf8_lossy(&digits[point..]);
    let decimals = decimals.trim_end_matches('0');
    let nonzero = digits.iter().any(|d| *d != b'0');
    format!(
        "{}{}{}",
        if value < 0. && nonzero { "-" } else { "" },
        grouped(&integer),
        if decimals.is_empty() {
            String::new()
        } else {
            format!(".{decimals}")
        }
    )
}
pub fn score_labels(scores: impl IntoIterator<Item = f64>) -> HashMap<u64, String> {
    let mut values: Vec<_> = scores.into_iter().filter(|v| v.is_finite()).collect();
    values.sort_by(f64::total_cmp);
    values.dedup_by(|a, b| *a == *b);
    let mut digits: HashMap<u64, usize> = values.iter().map(|v| (v.to_bits(), 1)).collect();
    let mut labels = HashMap::new();
    loop {
        let mut groups: HashMap<String, Vec<u64>> = HashMap::new();
        for value in &values {
            let (whole, fraction) = decimal_parts(*value);
            let count = digits[&value.to_bits()];
            let tail = fraction.chars().take(count).collect::<String>();
            let tail = tail.trim_end_matches('0');
            let prefix = if tail.is_empty() {
                whole
            } else {
                format!("{whole}.{tail}")
            };
            let label = format!(
                "{}{prefix}",
                if *value < 0. && prefix != "0" {
                    "-"
                } else {
                    ""
                }
            );
            labels.insert(value.to_bits(), label.clone());
            groups.entry(label).or_default().push(value.to_bits());
        }
        let collisions: Vec<_> = groups
            .values()
            .filter(|group| group.len() > 1)
            .cloned()
            .collect();
        if collisions.is_empty() {
            break;
        }
        for group in collisions {
            for key in group {
                *digits.get_mut(&key).unwrap() += 1;
            }
        }
    }
    labels
}
pub fn display(value: Option<f64>, metric: &str, detail: bool) -> String {
    let Some(value) = value else {
        return "Unreported".into();
    };
    match metric {
        "cost" => format!("${}", format_number(value, 2)),
        "tokens" => format!(
            "{}{}",
            format_number(value / 1e6, 1),
            if detail { "M" } else { "" }
        ),
        "time" => format!("{} s", format_number(value, 1)),
        "parameters" | "activeParameters" if detail => {
            if value >= 1000. {
                format!("{}T", format_number(value / 1000., 6))
            } else {
                format!("{}B", format_number(value, 3))
            }
        }
        "parameters" | "activeParameters" => format_number(value, 3),
        _ => format_number(value, 1),
    }
}
pub fn precision(value: Option<f64>, metric: &str) -> String {
    match metric {
        "cost" | "tokens" | "parameters" | "activeParameters" => display(value, metric, true),
        _ => value
            .map(|v| format!("{}{}", v, if metric == "time" { " s" } else { "" }))
            .unwrap_or_else(|| "Unreported".into()),
    }
}

const EFFORT_NAMES: [&str; 8] = [
    "none", "minimal", "low", "medium", "high", "xhigh", "max", "ultra",
];

// These names have a small, fixed suffix grammar. Parsing that grammar directly
// keeps a general-purpose regular-expression engine out of the browser module.
fn after_space(text: &str) -> Option<&str> {
    let remaining = text.trim_start();
    (remaining.len() != text.len()).then_some(remaining)
}
fn fallback_suffix(text: &str) -> bool {
    let text = text.strip_prefix(',').unwrap_or(text).trim_start();
    for prefix in ["with", "default"] {
        if text.strip_prefix(prefix).and_then(after_space) == Some("fallback") {
            return true;
        }
    }
    let Some(version) = text.strip_prefix("opus").and_then(after_space) else {
        return false;
    };
    let end = version
        .find(|c: char| c != '.' && !c.is_ascii_digit())
        .unwrap_or(version.len());
    end > 0 && after_space(&version[end..]) == Some("fallback")
}
fn effort_suffix(text: &str) -> bool {
    if text.is_empty() || fallback_suffix(text) {
        return true;
    }
    EFFORT_NAMES.iter().any(|name| {
        let Some(rest) = text.strip_prefix(name) else {
            return false;
        };
        rest.is_empty()
            || fallback_suffix(rest)
            || after_space(rest)
                .and_then(|r| r.strip_prefix("effort"))
                .is_some_and(|r| r.is_empty() || fallback_suffix(r))
    })
}
fn reasoning_suffix(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    effort_suffix(&lower)
        || [
            "non-reasoning",
            "non reasoning",
            "adaptive reasoning",
            "reasoning",
            "thinking",
        ]
        .iter()
        .any(|prefix| {
            lower.strip_prefix(prefix).is_some_and(|rest| {
                effort_suffix(rest)
                    || rest
                        .strip_prefix(',')
                        .is_some_and(|r| effort_suffix(r.trim_start()))
            })
        })
}
pub fn family_name(model: &Value) -> String {
    let name = s(model, "name").trim();
    if let Some(body) = name.strip_suffix(')') {
        if let Some(start) = body.rfind('(') {
            if reasoning_suffix(&body[start + 1..]) {
                return body[..start].trim_end().to_owned();
            }
        }
    }
    name.to_owned()
}
fn named_effort(name: &str) -> Option<&'static str> {
    let body = name.trim_end().strip_suffix(')')?;
    let tail = body.rsplit(')').next()?;
    let group = &tail[tail.find('(')? + 1..];
    let lower = group.to_ascii_lowercase();
    let word = |c: char| c.is_alphanumeric() || c == '_';
    let mut found = None;
    for name in EFFORT_NAMES {
        for (start, _) in lower.match_indices(name) {
            let end = start + name.len();
            if !lower[..start].chars().next_back().is_some_and(word)
                && !lower[end..].chars().next().is_some_and(word)
                && found.is_none_or(|(previous, _)| start > previous)
            {
                found = Some((start, name));
            }
        }
    }
    found.map(|(_, name)| name)
}
pub fn family_key(model: &Value) -> String {
    json!([
        s(model, "providerSlug"),
        model["release"]["slug"].as_str().unwrap_or(s(model, "id")),
        family_name(model).to_lowercase()
    ])
    .to_string()
}
pub fn effort(model: &Value) -> (String, f64) {
    if !b(model, "reasoning") {
        return ("None".into(), 0.);
    }
    let named = named_effort(s(model, "name")).map(str::to_owned);
    let label = named.unwrap_or_else(|| {
        model["effort"]["label"]
            .as_str()
            .unwrap_or("Reasoning")
            .to_string()
    });
    let lower = label.to_lowercase();
    let level = match lower.as_str() {
        "none" => 0.,
        "minimal" => 10.,
        "low" => 20.,
        "medium" => 30.,
        "high" => 40.,
        "xhigh" => 50.,
        "max" => 60.,
        "ultra" => 70.,
        _ => num(model, &["effort", "level"]).unwrap_or(35.),
    };
    let mut chars = label.chars();
    let label = chars
        .next()
        .map(|c| c.to_uppercase().collect::<String>() + chars.as_str())
        .unwrap_or_default();
    (label, level)
}
pub fn compact_effort(model: &Value) -> String {
    let (label, level) = effort(model);
    if level == 0. || label == "Reasoning" {
        String::new()
    } else {
        match label.to_lowercase().as_str() {
            "minimal" => "min".into(),
            "medium" => "med".into(),
            "ultra" => "ult".into(),
            _ => label.to_lowercase(),
        }
    }
}
pub fn families<'a>(
    models: impl IntoIterator<Item = &'a Value>,
) -> Vec<(String, String, Vec<&'a Value>)> {
    let mut groups: BTreeMap<String, Vec<&Value>> = BTreeMap::new();
    for model in models {
        groups.entry(family_key(model)).or_default().push(model);
    }
    groups
        .into_iter()
        .map(|(key, mut values)| {
            values.sort_by(|a, b| {
                effort(a)
                    .1
                    .total_cmp(&effort(b).1)
                    .then(s(a, "name").cmp(s(b, "name")))
                    .then(s(a, "id").cmp(s(b, "id")))
            });
            (key, family_name(values[0]), values)
        })
        .collect()
}
pub fn pareto<'a>(
    models: impl IntoIterator<Item = &'a Value>,
    metric: &str,
) -> (Vec<&'a Value>, Vec<&'a Value>) {
    let mut comparable: Vec<_> = models
        .into_iter()
        .filter(|model| {
            pareto_eligible(model)
                && n(model, metric).is_some()
                && (!parameter_metric(metric)
                    || parameter_model(model) && n(model, metric).is_some_and(|v| v > 0.))
        })
        .collect();
    comparable.sort_by(|a, b| {
        n(a, metric)
            .unwrap()
            .total_cmp(&n(b, metric).unwrap())
            .then(
                n(b, "intelligence")
                    .unwrap()
                    .total_cmp(&n(a, "intelligence").unwrap()),
            )
            .then(s(a, "name").cmp(s(b, "name")))
    });
    let (mut best, mut resource) = (f64::NEG_INFINITY, f64::NEG_INFINITY);
    let mut frontier = Vec::new();
    for model in &comparable {
        let score = n(model, "intelligence").unwrap();
        let value = n(model, metric).unwrap();
        if score > best {
            frontier.push(*model);
            best = score;
            resource = value;
        } else if score == best && value == resource {
            frontier.push(*model);
        }
    }
    (comparable, frontier)
}
pub fn dominates(anchor: &Value, model: &Value, metric: &str, strict: bool) -> bool {
    match (
        n(anchor, metric),
        n(model, metric),
        n(anchor, "intelligence"),
        n(model, "intelligence"),
    ) {
        (Some(a), Some(m), Some(x), Some(y)) => {
            if strict {
                a < m && x > y
            } else {
                a <= m && x >= y && (a < m || x > y)
            }
        }
        _ => false,
    }
}
pub fn dominance<'a>(
    models: &[&'a Value],
    anchors: &[&Value],
    metric: &str,
    dominators: bool,
) -> Vec<&'a Value> {
    let ids: HashSet<_> = anchors.iter().map(|m| s(m, "id")).collect();
    models
        .iter()
        .filter(|model| {
            !ids.contains(s(model, "id"))
                && anchors.iter().any(|anchor| {
                    if dominators {
                        dominates(model, anchor, metric, true)
                    } else {
                        dominates(anchor, model, metric, true)
                    }
                })
        })
        .copied()
        .collect()
}
pub fn largest_kill<'a>(models: &[&'a Value], metric: &str) -> (usize, Vec<&'a Value>) {
    let mut count = 0;
    let mut leaders = Vec::new();
    for anchor in models {
        let total = models
            .iter()
            .filter(|model| {
                s(anchor, "id") != s(model, "id") && dominates(anchor, model, metric, true)
            })
            .count();
        if total > count {
            count = total;
            leaders = vec![*anchor];
        } else if total == count && total > 0 {
            leaders.push(*anchor);
        }
    }
    leaders.sort_by(|a, b| {
        n(b, "intelligence")
            .unwrap_or(0.)
            .total_cmp(&n(a, "intelligence").unwrap_or(0.))
            .then(
                n(a, metric)
                    .unwrap_or(0.)
                    .total_cmp(&n(b, metric).unwrap_or(0.)),
            )
            .then(s(a, "name").cmp(s(b, "name")))
    });
    (count, leaders)
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}
pub fn curve_at(curve: &[Point], x: f64) -> f64 {
    if curve.is_empty() {
        return 0.;
    }
    let next = curve.iter().position(|p| p.x >= x);
    match next {
        None => curve.last().unwrap().y,
        Some(0) => curve[0].y,
        Some(i) if curve[i].x == x => curve[i].y,
        Some(i) => {
            let (a, b) = (curve[i - 1], curve[i]);
            a.y + (b.y - a.y) * (x - a.x) / (b.x - a.x)
        }
    }
}
pub fn ordered_curve(points: &[Point]) -> Vec<Point> {
    let mut values: Vec<_> = points
        .iter()
        .filter(|p| p.x.is_finite() && p.y.is_finite())
        .copied()
        .collect();
    values.sort_by(|a, b| a.x.total_cmp(&b.x).then(b.y.total_cmp(&a.y)));
    values.dedup_by(|a, b| a.x == b.x);
    values
}
pub fn cap_curve(family: &[Point], frontier: &[Point], right: f64) -> Vec<Point> {
    let family = ordered_curve(family);
    let frontier = ordered_curve(frontier);
    if family.is_empty() || frontier.is_empty() {
        return vec![];
    }
    let start = family[0].x;
    let end = right.max(family.last().unwrap().x);
    let mut knots: Vec<_> = family
        .iter()
        .chain(&frontier)
        .map(|p| p.x)
        .chain([start, end])
        .filter(|x| *x >= start && *x <= end)
        .collect();
    knots.sort_by(f64::total_cmp);
    knots.dedup();
    let mut result = Vec::new();
    for (i, x) in knots.iter().enumerate() {
        let a = curve_at(&family, *x);
        let b = curve_at(&frontier, *x);
        result.push(Point { x: *x, y: a.min(b) });
        if let Some(next) = knots.get(i + 1) {
            let difference = a - b;
            let other = curve_at(&family, *next) - curve_at(&frontier, *next);
            if difference * other < 0. {
                let cross = x + (next - x) * difference / (difference - other);
                result.push(Point {
                    x: cross,
                    y: curve_at(&family, cross).min(curve_at(&frontier, cross)),
                });
            }
        }
    }
    result
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Quaternion {
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub w: f64,
}
impl Default for Quaternion {
    fn default() -> Self {
        Self {
            x: 0.,
            y: 0.,
            z: 0.,
            w: 1.,
        }
    }
}
impl Quaternion {
    pub fn axis(x: f64, y: f64, z: f64, angle: f64) -> Self {
        let l = (x * x + y * y + z * z).sqrt();
        if l == 0. {
            return Self::default();
        }
        let s = (angle / 2.).sin() / l;
        Self {
            x: x * s,
            y: y * s,
            z: z * s,
            w: (angle / 2.).cos(),
        }
    }
    pub fn mul(self, b: Self) -> Self {
        let a = self;
        let q = Self {
            x: a.w * b.x + a.x * b.w + a.y * b.z - a.z * b.y,
            y: a.w * b.y - a.x * b.z + a.y * b.w + a.z * b.x,
            z: a.w * b.z + a.x * b.y - a.y * b.x + a.z * b.w,
            w: a.w * b.w - a.x * b.x - a.y * b.y - a.z * b.z,
        };
        let l = (q.x * q.x + q.y * q.y + q.z * q.z + q.w * q.w).sqrt();
        Self {
            x: q.x / l,
            y: q.y / l,
            z: q.z / l,
            w: q.w / l,
        }
    }
    pub fn rotate(self, p: [f64; 3]) -> [f64; 3] {
        let q = self;
        let tx = 2. * (q.y * p[2] - q.z * p[1]);
        let ty = 2. * (q.z * p[0] - q.x * p[2]);
        let tz = 2. * (q.x * p[1] - q.y * p[0]);
        [
            p[0] + q.w * tx + q.y * tz - q.z * ty,
            p[1] + q.w * ty + q.z * tx - q.x * tz,
            p[2] + q.w * tz + q.x * ty - q.y * tx,
        ]
    }
}
pub fn html(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}
pub fn csv_cell(value: &Value) -> String {
    let text = if let Some(v) = value.as_str() {
        if v.starts_with(['=', '+', '@', '-']) {
            format!("'{v}")
        } else {
            v.to_string()
        }
    } else if value.is_null() {
        String::new()
    } else {
        value.to_string()
    };
    format!("\"{}\"", text.replace('"', "\"\""))
}

pub fn retain(snapshot: &mut Value, raw: &mut Value) {
    let models: Vec<_> = array(&snapshot["models"])
        .iter()
        .filter(|m| eligible(m))
        .cloned()
        .collect();
    let ids: HashSet<String> = models.iter().map(|m| s(m, "id").to_string()).collect();
    snapshot["models"] = json!(models);
    snapshot["dataPolicy"] = json!(POLICY);
    snapshot["defaultModelIds"] = json!(
        array(&snapshot["defaultModelIds"])
            .iter()
            .filter(|id| id.as_str().is_some_and(|id| ids.contains(id)))
            .cloned()
            .collect::<Vec<_>>()
    );
    let sources: Vec<_> = array(&raw["models"])
        .iter()
        .filter(|m| eligible_source(m) && ids.contains(s(m, "id")))
        .cloned()
        .collect();
    let source_ids: HashSet<String> = sources.iter().map(|m| s(m, "id").to_string()).collect();
    raw["models"] = json!(sources);
    for key in [
        "defaults",
        "catalog",
        "parameterMetadata",
        "pricingVariants",
    ] {
        let values: Vec<_> = array(&raw[key])
            .iter()
            .filter(|model| match key {
                "defaults" => source_ids.contains(s(model, "id")) && eligible_source(model),
                "catalog" => source_ids.contains(s(model, "id")),
                "pricingVariants" => {
                    ids.contains(s(model, "id"))
                        && eligible(model)
                        && source_ids.contains(text(model, &["pricing", "sourceModelId"]))
                }
                _ => ids.contains(s(model, "id")),
            })
            .cloned()
            .collect();
        raw[key] = json!(values);
    }
    raw["dataPolicy"] = json!(POLICY);
}
