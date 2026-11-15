use crate::charts::Plot;
use crate::*;
pub fn dark(color: &str) -> bool {
    if color == "black" {
        return true;
    }
    let hex = color.replace('#', "");
    if hex.len() != 6 || !hex.is_ascii() {
        return false;
    }
    let mut luminance = 0.;
    for (i, weight) in [0.2126, 0.7152, 0.0722].iter().enumerate() {
        let Ok(channel) = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16) else {
            return false;
        };
        luminance += channel as f64 * weight;
    }
    luminance < 55.
}
pub fn heat(score: f64) -> (String, &'static str) {
    let progress = score / 100.;
    let (start, end, p) = if progress <= 0.5 {
        ([13_f64, 175., 85.], [211., 139., 0.], progress * 2.)
    } else {
        ([211_f64, 139., 0.], [104., 12., 24.], (progress - 0.5) * 2.)
    };
    let rgb: [f64; 3] = std::array::from_fn(|i| (start[i] + (end[i] - start[i]) * p).round());
    let linear = rgb.map(|c| {
        let v = c / 255.;
        if v <= 0.04045 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    });
    (
        format!("rgb({} {} {})", rgb[0], rgb[1], rgb[2]),
        if linear[0] * 0.2126 + linear[1] * 0.7152 + linear[2] * 0.0722 > 0.24 {
            "#071008"
        } else {
            "#fffdf7"
        },
    )
}
pub fn ranges(models: &[&Value], metric: &str) -> (f64, f64) {
    let values = models
        .iter()
        .filter(|m| !parameter_metric(metric) || parameter_model(m))
        .filter_map(|m| n(m, metric))
        .collect::<Vec<_>>();
    (
        values.iter().copied().fold(f64::INFINITY, f64::min),
        values.iter().copied().fold(f64::NEG_INFINITY, f64::max),
    )
}
pub fn penalty(value: Option<f64>, metric: &str, range: (f64, f64)) -> Option<f64> {
    let value = value?;
    let (min, max) = range;
    if !min.is_finite() || !max.is_finite() {
        return None;
    }
    Some(if max == min {
        0.
    } else {
        (100.
            * if metric == "intelligence" {
                (max - value) / (max - min)
            } else {
                (value - min) / (max - min)
            })
        .clamp(0., 100.)
    })
}
pub fn chart_display(value: Option<f64>, metric: &str) -> String {
    display(value, metric, false)
}
pub fn shade(color: &str, index: usize, count: usize) -> (String, &'static str) {
    let hex = if color == "black" {
        "000000"
    } else {
        color.trim_start_matches('#')
    };
    let channels: Vec<f64> = if hex.len() == 6 && hex.chars().all(|c| c.is_ascii_hexdigit()) {
        [0, 2, 4]
            .iter()
            .map(|i| u8::from_str_radix(&hex[*i..*i + 2], 16).unwrap() as f64)
            .collect()
    } else {
        vec![100., 120., 90.]
    };
    let p = index as f64 / count.saturating_sub(1).max(1) as f64;
    let mix = if dark(color) {
        0.25 + p * 0.5
    } else {
        p * 0.55
    };
    let rgb: Vec<_> = channels
        .iter()
        .map(|c| (c + (255. - c) * mix).round())
        .collect();
    (
        format!("rgb({}, {}, {})", rgb[0], rgb[1], rgb[2]),
        if rgb[0] * 0.2126 + rgb[1] * 0.7152 + rgb[2] * 0.0722 > 145. {
            "#091005"
        } else {
            "#fff"
        },
    )
}
fn breaks(text: &str) -> Vec<(String, String, f64)> {
    let chars: Vec<_> = text.char_indices().collect();
    let mut result = Vec::new();
    for (i, (byte, ch)) in chars.iter().enumerate().take(chars.len().saturating_sub(1)) {
        let next = chars[i + 1].1;
        let split = match *ch {
            ' ' => Some((*byte, *byte + 1, 10.)),
            '-' | '–' => Some((*byte + ch.len_utf8(), *byte + ch.len_utf8(), 9.)),
            '/' | '_' => Some((*byte + 1, *byte + 1, 8.)),
            ')' => Some((*byte + 1, *byte + 1, 8.)),
            _ if next == '(' => Some((*byte + ch.len_utf8(), *byte + ch.len_utf8(), 9.)),
            _ if ch.is_ascii_lowercase() && next.is_ascii_uppercase() => {
                Some((*byte + ch.len_utf8(), *byte + ch.len_utf8(), 6.))
            }
            _ => None,
        };
        if let Some((left, right, q)) = split {
            let a = text[..left].trim();
            let b = text[right..].trim();
            if !a.is_empty() && !b.is_empty() {
                result.push((a.into(), b.into(), q));
            }
        }
    }
    result
}
fn len(text: &str) -> usize {
    text.encode_utf16().count()
}
pub fn wrap(text: &str, width: f64, slot: f64, size: f64) -> Vec<String> {
    let measure = |t: &str| visual::width(t, size);
    let total = measure(text);
    let tolerance = total / len(text).max(1) as f64 * 2.1;
    let max_width = (width + tolerance).min(slot - 2.);
    if total <= max_width {
        return vec![text.into()];
    }
    let mut best = None;
    let mut score = f64::INFINITY;
    for (a, b, q) in breaks(text) {
        let (x, y) = (measure(&a), measure(&b));
        let mut p = (x - y).abs() * 1.4;
        if x > width {
            p += (x - width) * 2.;
        }
        if y > width {
            p += (y - width) * 2.;
        }
        if x > max_width || y > max_width {
            p += 2000. + (x.max(y) - max_width) * 20.;
        }
        for text in [&a, &b] {
            p += match len(text) {
                0 | 1 => 500.,
                2 => 300.,
                3 => 50.,
                _ => 0.,
            };
        }
        p -= q * 3.;
        if p < score {
            score = p;
            best = Some((a, b));
        }
    }
    if score < 1500. {
        if let Some((a, b)) = best {
            return vec![a, b];
        }
    }
    let mut lines = Vec::new();
    let mut remaining = text.to_string();
    while !remaining.is_empty() {
        if measure(&remaining) <= max_width {
            lines.push(remaining);
            break;
        }
        let mut valid: Vec<_> = breaks(&remaining)
            .into_iter()
            .filter(|b| measure(&b.0) <= max_width)
            .collect();
        if !valid.is_empty() {
            valid.sort_by(|a, b| {
                let score = |v: &(String, String, f64)| {
                    let orphan = if len(&v.1) <= 2 {
                        300
                    } else if len(&v.1) <= 3 {
                        50
                    } else {
                        0
                    };
                    orphan - len(&v.0) as i32
                };
                score(a).cmp(&score(b))
            });
            lines.push(valid[0].0.clone());
            remaining = valid[0].1.clone();
        } else {
            let chars: Vec<_> = remaining
                .char_indices()
                .map(|(i, _)| i)
                .chain([remaining.len()])
                .collect();
            let mut end = 1;
            while end + 1 < chars.len() && measure(&remaining[..chars[end + 1]]) <= width {
                end += 1;
            }
            if chars.len() - 1 - end <= 2 && end > 2 {
                end = 1.max(chars.len() - 1 - 3);
            }
            lines.push(remaining[..chars[end]].trim().into());
            remaining = remaining[chars[end]..].trim_start().into();
        }
    }
    lines
}
#[derive(Clone)]
pub struct Bar<'a> {
    pub key: String,
    pub name: String,
    pub model: &'a Value,
    pub efforts: Vec<&'a Value>,
}
pub fn groups<'a>(a: &'a App) -> (Vec<&'a Value>, Vec<Bar<'a>>) {
    let metric = a.metric();
    let available = a
        .visible()
        .into_iter()
        .filter(|m| n(m, metric).is_some())
        .collect::<Vec<_>>();
    let mut bars = if b(&a.prefs, "stackReasoning") {
        families(available.iter().copied())
            .into_iter()
            .map(|(key, name, mut efforts)| {
                if metric == "intelligence" {
                    let mut best = f64::NEG_INFINITY;
                    efforts.retain(|m| {
                        let score = n(m, "intelligence").unwrap();
                        if score <= best {
                            false
                        } else {
                            best = score;
                            true
                        }
                    });
                }
                let model = if metric == "intelligence" {
                    *efforts.last().unwrap()
                } else {
                    *efforts
                        .iter()
                        .max_by(|a, b| n(a, metric).unwrap().total_cmp(&n(b, metric).unwrap()))
                        .unwrap()
                };
                Bar {
                    key,
                    name,
                    model,
                    efforts,
                }
            })
            .collect::<Vec<_>>()
    } else {
        available
            .iter()
            .map(|m| Bar {
                key: s(m, "id").into(),
                name: s(m, "name").into(),
                model: *m,
                efforts: vec![*m],
            })
            .collect()
    };
    bars.sort_by(|a, b| {
        let d = if metric == "intelligence" {
            n(a.model, metric).unwrap() - n(b.model, metric).unwrap()
        } else {
            n(b.model, metric).unwrap() - n(a.model, metric).unwrap()
        };
        d.total_cmp(&0.)
            .then_with(|| visual::compare(&a.name, &b.name))
    });
    (available, bars)
}
pub fn render(a: &mut App) -> String {
    let metric = a.metric().to_string();
    let (available, bars) = groups(a);
    let stacked = b(&a.prefs, "stackReasoning");
    let compare = !a.comparison.is_empty();
    let scores = score_labels(bars.iter().filter_map(|bar| n(bar.model, "intelligence")));
    let values = bars
        .iter()
        .flat_map(|bar| bar.efforts.iter().map(|m| n(m, &metric).unwrap()))
        .collect::<Vec<_>>();
    let max = values.iter().copied().fold(1., f64::max);
    let all_min = if values.is_empty() {
        0.
    } else {
        values.iter().copied().fold(f64::INFINITY, f64::min)
    };
    let min_score = bars
        .iter()
        .filter_map(|bar| n(bar.model, &metric))
        .fold(f64::INFINITY, f64::min);
    let min_score = if bars.is_empty() { all_min } else { min_score };
    let step = if parameter_metric(&metric) {
        10_f64.powf(max.log10().floor()) / 5.
    } else if metric == "tokens" {
        10_000_000.
    } else {
        10.
    };
    let focused_top = step.max((max / step).ceil() * step);
    let max_bottom = (min_score - 0.6 * focused_top) / 0.4;
    let focused_bottom = ((all_min / step).floor() * step)
        .min((max_bottom / step).floor() * step)
        .min(focused_top - 2. * step)
        .max(0.);
    let full = metric == "intelligence" && b(&a.prefs, "fullIntelligenceScale");
    let bottom = if full { 0. } else { focused_bottom };
    let top = if full { 100. } else { focused_top };
    let (vw, height) = visual::measured(a, "bars", 1150., 680.);
    let count = bars.len().max(1) as f64;
    let value_width = if metric == "cost"
        || metric == "tokens"
        || parameter_metric(&metric)
        || metric == "intelligence"
    {
        bars.iter()
            .map(|bar| {
                if metric == "intelligence" {
                    len(&scores[&n(bar.model, &metric).unwrap().to_bits()])
                } else {
                    len(&chart_display(n(bar.model, &metric), &metric))
                }
            })
            .max()
            .unwrap_or(0) as f64
            * 6.
    } else {
        0.
    };
    let tick_count = if metric == "intelligence" {
        ((top - bottom) / 10. + 1.).floor() as usize
    } else {
        6
    };
    let tick_values = (0..tick_count)
        .map(|i| {
            if metric == "intelligence" {
                bottom + i as f64 * 10.
            } else {
                bottom + (top - bottom) * i as f64 / 5.
            }
        })
        .collect::<Vec<_>>();
    let axis_width = (tick_values
        .iter()
        .map(|v| len(&chart_display(Some(*v), &metric)))
        .max()
        .unwrap_or(0) as f64
        * 6.6)
        .ceil()
        .max(12.)
        + 12.;
    let axis_width = axis_width.max(24.);
    let fill = (0.84 - (count - 1.) * 0.002).max(0.78);
    let steps = count - 1. + fill;
    let min_slot = (if stacked { 48_f64 } else { 44. }).max(value_width + 12.);
    let mut width = vw.max(steps * min_slot + axis_width + 2.);
    let preliminary = (width - axis_width - 2.) / steps;
    let font = (preliminary * 0.15).clamp(10., 16.);
    let logo_size = font * 1.6;
    let line_height = font * 1.25;
    let names = bars
        .iter()
        .map(|bar| {
            if len(&bar.name) > 39 {
                bar.name.chars().take(37).collect::<String>() + "…"
            } else {
                bar.name.clone()
            }
        })
        .collect::<Vec<_>>();
    let widths = names
        .iter()
        .map(|name| visual::width(name, font))
        .collect::<Vec<_>>();
    let rotated = preliminary < 64.;
    let angle = 56_f64.to_radians();
    let left = axis_width.max(if rotated {
        widths
            .iter()
            .enumerate()
            .map(|(i, w)| w * angle.cos() - (i as f64 + fill / 2.) * preliminary + 8.)
            .fold(0., f64::max)
    } else {
        0.
    });
    width = width.max(steps * min_slot + left + 2.);
    let slot = (width - left - 2.) / steps;
    let bar_width = slot * fill;
    let center = |i: usize| left + bar_width / 2. + i as f64 * slot;
    let labels = names
        .iter()
        .enumerate()
        .map(|(i, name)| {
            if rotated {
                vec![name.clone()]
            } else {
                let x = center(i);
                let label_slot = slot.min(2. * x.min(width - x) - 4.);
                wrap(name, label_slot - 12., label_slot, font)
            }
        })
        .collect::<Vec<_>>();
    let margin_bottom = 14.
        + logo_size
        + 14.
        + if rotated {
            widths.iter().copied().fold(0., f64::max) * angle.sin() + font
        } else {
            labels.iter().map(Vec::len).max().unwrap_or(1).max(1) as f64 * line_height
        }
        + 18.;
    let margin_top = font + 12.;
    let plot_height = (height - margin_top - margin_bottom).max(1.);
    let plot = Plot {
        x: left,
        y: margin_top,
        width: width - left - 2.,
        height: plot_height,
        canvas_width: width,
        canvas_height: height,
    };
    let height_for = |v: f64| (v - bottom) / (top - bottom) * plot_height;
    let ids = a.effective();
    let norm = a
        .models()
        .iter()
        .filter(|m| ids.contains(s(m, "id")))
        .collect::<Vec<_>>();
    let range = ranges(&norm, &metric);
    let mut defs = String::from("<linearGradient id=\"bar-heat-colors\" x1=\"0%\" x2=\"100%\">");
    for (i, bar) in bars.iter().enumerate() {
        let color = heat(penalty(n(bar.model, &metric), &metric, range).unwrap_or(50.)).0;
        let start = if i == 0 {
            0.
        } else {
            (center(i) - slot / 2. - left) / plot.width * 100.
        };
        let end = if i + 1 == bars.len() {
            100.
        } else {
            (center(i) + slot / 2. - left) / plot.width * 100.
        };
        let blend = 4. / bars.len() as f64;
        for offset in [
            if i == 0 { 0. } else { start + blend },
            if i + 1 == bars.len() {
                100.
            } else {
                end - blend
            },
        ] {
            defs.push_str(&format!(
                "<stop offset=\"{offset}%\" stop-color=\"{color}\"></stop>"
            ));
        }
    }
    defs.push_str("</linearGradient><marker id=\"bar-comparison-arrow\" markerWidth=\"12\" markerHeight=\"12\" refX=\"10\" refY=\"6\" orient=\"auto\" markerUnits=\"userSpaceOnUse\"><path d=\"M 0 0 L 12 6 L 0 12 Z\" fill=\"var(--chart-highlight, #d7ff9d)\"></path></marker>");
    let mut svg = format!(
        "<svg id=\"graph-svg\" class=\"bars-svg\" viewBox=\"0 0 {width} {height}\" style=\"width:{width}px;height:{height}px\" role=\"group\"><defs>{defs}</defs><rect class=\"bar-label-heatmap\" x=\"{left}\" y=\"{}\" width=\"{}\" height=\"20\" fill=\"url(#bar-heat-colors)\" aria-hidden=\"true\"></rect>",
        height - 20.,
        plot.width
    );
    for (i, value) in tick_values.iter().enumerate() {
        let y = margin_top + plot_height - height_for(*value);
        svg.push_str(&format!("<g><line x1=\"{left}\" x2=\"{}\" y1=\"{y}\" y2=\"{y}\" stroke=\"{}\"{}></line><text x=\"{}\" y=\"{}\" text-anchor=\"end\" class=\"axis-label\">{}</text></g>",width-2.,if i==0{"var(--chart-axis, #343434)"}else{"var(--chart-grid, #242424)"},if i==0{""}else{" stroke-dasharray=\"2 5\""},left-8.,y+4.,html(&chart_display(Some(*value),&metric))));
    }
    for (i, bar) in bars.iter().enumerate() {
        let m = bar.model;
        let x = center(i);
        let slot_left = if i == 0 { left } else { x - slot / 2. };
        let slot_right = if i + 1 == bars.len() {
            width - 2.
        } else {
            x + slot / 2.
        };
        let sw = slot_right - slot_left;
        let value = n(m, &metric).unwrap();
        let h = height_for(value).max(0.);
        let y = margin_top + plot_height - h;
        let radius = 2_f64.min(bar_width / 2.).min(h / 2.);
        let l = x - bar_width / 2.;
        let r = x + bar_width / 2.;
        let path = format!(
            "M {l} {} V {} Q {l} {y} {} {y} H {} Q {r} {y} {r} {} V {} Z",
            y + h,
            y + radius,
            l + radius,
            r - radius,
            y + radius,
            y + h
        );
        let label = if metric == "intelligence" {
            scores[&value.to_bits()].clone()
        } else if metric == "time" {
            format!("{}s", format_number(value, 0))
        } else {
            chart_display(Some(value), &metric)
        };
        let space = (sw - 8.).min(2. * x.min(width - x) - 8.);
        let value_font = font.min(space / (len(&label) as f64 * 0.6));
        let dark = dark(s(m, "color"));
        let hovered = bar.efforts.iter().any(|e| s(e, "id") == a.hovered);
        let any_hover = !a.hovered.is_empty();
        let role = if stacked { "group" } else { "button" };
        let click = if compare {
            bar.key.as_str()
        } else {
            s(m, "id")
        };
        svg.push_str(&format!("<g class=\"bar-item\" role=\"{role}\" {} data-action=\"bar\" data-value=\"{}\" data-hover=\"{}\" data-bar-key=\"{}\" data-guide-y=\"{y}\" opacity=\"{}\"><rect x=\"{slot_left}\" y=\"{}\" width=\"{sw}\" height=\"{}\" fill=\"transparent\"></rect>",if stacked{""}else{"tabindex=\"0\""},html(click),html(s(m,"id")),html(&bar.key),if any_hover&&!hovered{0.58}else{1.},margin_top-25.,plot_height+175.));
        if stacked {
            svg.push_str(&format!("<defs><clipPath id=\"bar-clip-{i}\"><path d=\"{path}\"></path></clipPath></defs><g clip-path=\"url(#bar-clip-{i})\">"));
            let mut endpoints = bar
                .efforts
                .iter()
                .enumerate()
                .map(|(i, m)| (i, *m, height_for(n(m, &metric).unwrap()).max(0.)))
                .collect::<Vec<_>>();
            endpoints.sort_by(|a, b| a.2.total_cmp(&b.2).then(a.0.cmp(&b.0)));
            let mut previous = 0.;
            for (ei, effort, endpoint) in endpoints {
                let sh = endpoint - previous;
                previous = endpoint;
                let sy = margin_top + plot_height - endpoint;
                let text = if endpoint > 0. && (metric == "intelligence" || sh >= 12.) {
                    compact_effort(effort)
                } else {
                    String::new()
                };
                let (fill, foreground) = shade(s(effort, "color"), ei, bar.efforts.len());
                let active = hovered && a.hovered == s(effort, "id");
                svg.push_str(&format!("<g class=\"stacked-segment\" role=\"button\" tabindex=\"0\" data-action=\"bar\" data-value=\"{}\" data-hover=\"{}\" data-guide-y=\"{sy}\"><rect class=\"effort-segment-fill\" x=\"{l}\" y=\"{sy}\" width=\"{bar_width}\" height=\"{sh}\" fill=\"{fill}\"></rect><line x1=\"{l}\" x2=\"{r}\" y1=\"{sy}\" y2=\"{sy}\" stroke=\"{}\" stroke-width=\"0.8\"></line>{}<rect class=\"effort-segment-hit\" x=\"{l}\" y=\"{}\" width=\"{bar_width}\" height=\"{}\" fill=\"transparent\" stroke=\"{}\" stroke-width=\"1.2\"></rect></g>",html(if compare{&bar.key}else{s(effort,"id")}),html(s(effort,"id")),if sh==0.{&fill}else{"var(--page)"},if text.is_empty(){String::new()}else{format!("<text class=\"effort-segment-label\" style=\"font-size:{}px\" x=\"{x}\" y=\"{}\" text-anchor=\"middle\" dominant-baseline=\"central\" fill=\"{foreground}\">{}</text>",font*0.9,sy+sh/2.,html(&text))},if sh<6.{sy-(6.-sh)/2.}else{sy},sh.max(6.),if active{"var(--chart-highlight, #fff)"}else{"none"}));
            }
            svg.push_str("</g>");
        }
        svg.push_str(&format!("<path data-bar-outline=\"{}\" d=\"{path}\" fill=\"{}\" stroke=\"{}\" stroke-width=\"{}\"{}></path><text x=\"{x}\" y=\"{}\" text-anchor=\"middle\" class=\"bar-value\" style=\"font-size:{value_font}px\">{}</text><image href=\"{}\" x=\"{}\" y=\"{}\" width=\"{logo_size}\" height=\"{logo_size}\" class=\"chart-logo\"></image>",html(s(m,"id")),if stacked{"none"}else{s(m,"color")},if hovered{"var(--chart-highlight, #fff)".into()}else{format!("var(--chart-outline, {})",if dark{"#858585"}else{s(m,"color")})},if hovered{1.6}else if stacked{0.8}else if dark{1.}else{0.6},if stacked{" pointer-events=\"none\""}else{""},y-10.,html(&label),html(s(m,"logo")),x-logo_size/2.,margin_top+plot_height+14.));
        if rotated {
            svg.push_str(&format!("<text transform=\"translate({}, {}) rotate(-56)\" text-anchor=\"end\" class=\"bar-name\" style=\"font-size:{font}px\">{}</text>",x+5.,margin_top+plot_height+19.+logo_size,html(&labels[i][0])));
        } else {
            svg.push_str(&format!("<text x=\"{x}\" y=\"{}\" text-anchor=\"middle\" class=\"bar-name\" style=\"font-size:{font}px\">{}</text>",margin_top+plot_height+19.+logo_size+font,labels[i].iter().enumerate().map(|(j,line)|format!("<tspan x=\"{x}\" dy=\"{}\">{}</tspan>",if j==0{0.}else{line_height},html(line))).collect::<String>()));
        }
        svg.push_str("</g>");
    }
    if a.comparison.len() > 1 {
        if let (Some(from), Some(to)) = (
            bars.iter().position(|bar| bar.key == a.comparison[0]),
            bars.iter().position(|bar| bar.key == a.comparison[1]),
        ) {
            let sx = center(from);
            let ex = center(to);
            let bar_top = |i: usize| {
                margin_top + plot_height - height_for(n(bars[i].model, &metric).unwrap())
            };
            let sy = (bar_top(from) - 34.).max(4.);
            let ey = (bar_top(to) - 34.).max(4.);
            let dx = ex - sx;
            let mut control = sy.min(ey) - (dx.abs() * 0.15).clamp(48., 130.);
            for i in from.min(to) + 1..from.max(to) {
                let cx = center(i);
                for x in [cx - bar_width / 2. - 5., cx, cx + bar_width / 2. + 5.] {
                    let progress = (x - sx) / dx;
                    let (mut low, mut high) = (0., 1.);
                    for _ in 0..28 {
                        let mid = (low + high) / 2.;
                        if 3. * mid * mid - 2. * mid * mid * mid < progress {
                            low = mid;
                        } else {
                            high = mid;
                        }
                    }
                    let t = (low + high) / 2.;
                    control = control.min(
                        (bar_top(i) - 34. - (1. - t).powi(3) * sy - t.powi(3) * ey)
                            / (3. * t * (1. - t)),
                    );
                }
            }
            let difference =
                n(bars[to].model, &metric).unwrap() - n(bars[from].model, &metric).unwrap();
            let label = format!(
                "{}{}",
                if difference > 0. {
                    "+"
                } else if difference < 0. {
                    "−"
                } else {
                    ""
                },
                chart_display(Some(difference.abs()), &metric)
            );
            svg.push_str(&format!("<g class=\"comparison-arc\"><title>{} to {} · {}</title><path d=\"M {sx} {sy} C {sx} {control} {ex} {control} {ex} {ey}\" marker-end=\"url(#bar-comparison-arrow)\"></path><text x=\"{}\" y=\"{}\" text-anchor=\"middle\">{}</text></g>",html(&bars[from].name),html(&bars[to].name),html(&label),(sx+ex)/2.,(sy/8.+control*3./4.+ey/8.-15.).max(18.),html(&label)));
        }
    }
    svg.push_str("<g id=\"bar-hover-guide\" class=\"bar-hover-guide\" aria-hidden=\"true\" pointer-events=\"none\" stroke-dasharray=\"0 7\" stroke-linecap=\"round\"></g></svg>");
    let caption = format!(
        "<div class=\"chart-caption\"><div class=\"chart-caption-start\"><div class=\"chart-benchmark\"><strong>{}</strong>{}</div><div class=\"chart-settings\" data-screenshot-exclude>{}{}{}</div></div><div class=\"chart-drawing-tools-slot\" data-screenshot-exclude>{}</div>{}</div>",
        metric_label(&metric),
        if parameter_metric(&metric) || metric == "tokens" || metric == "time" {
            format!(
                "<span>(<!-- -->{}<!-- -->)</span>",
                if parameter_metric(&metric) {
                    "billions"
                } else if metric == "tokens" {
                    "millions"
                } else {
                    "seconds"
                }
            )
        } else {
            String::new()
        },
        visual::checkbox(
            "preference",
            "stackReasoning",
            "Stack reasoning efforts",
            stacked,
            "stack-reasoning-setting",
            ""
        ),
        if metric == "intelligence" {
            visual::checkbox(
                "preference",
                "fullIntelligenceScale",
                "0 to 100 scale",
                b(&a.prefs, "fullIntelligenceScale"),
                "intelligence-scale-setting",
                "",
            )
        } else {
            String::new()
        },
        format!(
            "<label class=\"checkbox-label compare-models-setting\"><input type=\"checkbox\" data-action=\"compare-bars\" {}>Compare models</label>",
            if compare { "checked" } else { "" }
        ),
        if bars.is_empty() {
            String::new()
        } else {
            drawing::toolbar(&a.drawing)
        },
        visual::direction(metric == "intelligence")
    );
    let body = if bars.is_empty() {
        format!(
            "<div class=\"empty-state\"><span>Choose models to compare</span><p>{}</p></div>",
            if a.visible().is_empty() {
                "Adjust your filters or add models to the selection."
            } else {
                "The selected metric has published values for other models in the catalog."
            }
        )
    } else {
        format!(
            "<div class=\"chart-plot\"><div class=\"chart-scroll\" data-measure=\"bars\" data-scroll=\"bars\">{}{}</div></div>",
            svg,
            drawing::overlay(&a.drawing, plot)
        )
    };
    let missing = available.len() < a.visible().len();
    let result = format!(
        "<div class=\"bar-chart{}\" id=\"bar-container\">{}{}<div id=\"chart-tooltip\" class=\"model-tooltip\" data-screenshot-exclude role=\"tooltip\" hidden></div>{}</div>",
        if compare { " is-comparing" } else { "" },
        caption,
        body,
        if missing {
            format!(
                "<p class=\"data-note\">{}<!-- --> of <!-- -->{}<!-- --> selected models have a published value for this metric.</p>",
                available.len(),
                a.visible().len()
            )
        } else {
            String::new()
        }
    );
    a.plot = plot;
    result
}
