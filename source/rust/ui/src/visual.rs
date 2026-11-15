use crate::*;
use std::cmp::Ordering;
pub fn event_point(event: &JsValue) -> Point {
    let get = |key: &str| {
        js_sys::Reflect::get(event, &JsValue::from_str(key))
            .ok()
            .and_then(|v| v.as_f64())
            .unwrap_or(0.)
    };
    Point {
        x: get("clientX"),
        y: get("clientY"),
    }
}
pub fn sync_theme() {
    if let (Some(root), Ok(Some(meta))) = (
        document().document_element(),
        document().query_selector("meta[name=theme-color]"),
    ) {
        if let Ok(Some(style)) = window().get_computed_style(&root) {
            let _ = meta.set_attribute(
                "content",
                style
                    .get_property_value("--page")
                    .unwrap_or_default()
                    .trim(),
            );
        }
    }
}
pub fn focus(e: &Element) {
    let options = js_sys::Object::new();
    let _ = js_sys::Reflect::set(
        &options,
        &JsValue::from_str("preventScroll"),
        &JsValue::TRUE,
    );
    if let Ok(f) = js_sys::Reflect::get(e, &JsValue::from_str("focus")) {
        if let Some(f) = f.dyn_ref::<js_sys::Function>() {
            let _ = f.call1(e, &options);
        }
    }
}
pub fn icon(name: &str, size: u32, class: &str) -> String {
    macro_rules! icons {($($name:literal),*)=>{match name{$($name=>include_str!(concat!("../../../assets/icons/",$name,".svg")),)*_=>""}}}
    let svg = icons!(
        "ArrowDownToLine",
        "ArrowUpRight",
        "BarChart3",
        "Check",
        "ChevronDown",
        "CircleHelp",
        "Download",
        "Filter",
        "LoaderCircle",
        "Plus",
        "RefreshCw",
        "Search",
        "Table2",
        "X",
        "Moon",
        "Sun",
        "Camera",
        "ArrowDown",
        "Pause",
        "Play",
        "RotateCcw",
        "SlidersHorizontal",
        "Crosshair",
        "Info",
        "Braces",
        "MousePointer2",
        "Pencil",
        "Trash2",
        "Undo2"
    );
    let svg = svg
        .replacen("width=\"24\"", &format!("width=\"{size}\""), 1)
        .replacen("height=\"24\"", &format!("height=\"{size}\""), 1);
    if class.is_empty() {
        svg
    } else {
        svg.replacen(
            "class=\"lucide ",
            &format!("class=\"{} lucide ", html(class)),
            1,
        )
    }
}
pub fn btn(action: &str, value: &str, content: &str, class: &str, attrs: &str) -> String {
    format!(
        "<button{} data-action=\"{}\" data-value=\"{}\" {}>{}</button>",
        if class.is_empty() {
            String::new()
        } else {
            format!(" class=\"{}\"", html(class))
        },
        html(action),
        html(value),
        attrs,
        content
    )
}
pub fn checkbox(
    field: &str,
    key: &str,
    label: &str,
    checked: bool,
    class: &str,
    attrs: &str,
) -> String {
    format!(
        "<label class=\"checkbox-label{}\" {}><input type=\"checkbox\" data-field=\"{}\" data-key=\"{}\" value=\"{}\" {}>{}</label>",
        if class.is_empty() {
            String::new()
        } else {
            format!(" {}", html(class))
        },
        attrs,
        html(field),
        html(key),
        html(key),
        if checked { "checked" } else { "" },
        label
    )
}
pub fn measured(a: &App, key: &str, width: f64, height: f64) -> (f64, f64) {
    let result = a.sizes.get(key).copied().unwrap_or_default();
    (
        if result.0 == 0. { width } else { result.0 },
        if result.1 == 0. { height } else { result.1 },
    )
}
pub fn width(text: &str, size: f64) -> f64 {
    labels::measure(text, size).width
}
pub fn compare(a: &str, b: &str) -> Ordering {
    use js_sys::{Array, Intl, Object};
    thread_local! {static COLLATOR:Intl::Collator=Intl::Collator::new(&Array::new(),&Object::new());}
    COLLATOR.with(|c| {
        c.compare()
            .call2(
                &JsValue::UNDEFINED,
                &JsValue::from_str(a),
                &JsValue::from_str(b),
            )
            .ok()
            .and_then(|v| v.as_f64())
            .unwrap_or(0.)
            .total_cmp(&0.)
    })
}
pub fn date(value: &str, time: bool) -> String {
    let options = js_sys::Object::new();
    for (key, val) in if time {
        vec![("hour", "numeric"), ("minute", "2-digit")]
    } else {
        vec![("month", "short"), ("day", "numeric"), ("year", "numeric")]
    } {
        let _ = js_sys::Reflect::set(&options, &JsValue::from_str(key), &JsValue::from_str(val));
    }
    let locales = js_sys::Array::of1(&JsValue::from_str("en-US"));
    let format = js_sys::Intl::DateTimeFormat::new(&locales, &options);
    let date = js_sys::Date::new(&JsValue::from_str(value));
    format
        .format()
        .call1(&JsValue::UNDEFINED, &date)
        .ok()
        .and_then(|v| v.as_string())
        .unwrap_or_default()
}
pub fn direction(higher: bool) -> String {
    format!(
        "<span class=\"chart-direction\"><span data-direction-label>Better<span class=\"chart-direction-baseline\" aria-hidden=\"true\"></span></span><svg data-direction-icon width=\"32\" height=\"32\" viewBox=\"0 0 32 32\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.6\" stroke-linecap=\"round\" stroke-linejoin=\"round\" aria-hidden=\"true\"><g data-direction-placement><path data-direction-path d=\"{}\"></path></g></svg></span>",
        if higher {
            "M5 27 27 5M19 5h8v8"
        } else {
            "M5 5l22 22M19 27h8v-8"
        }
    )
}
pub fn align_direction() {
    let find = |q: &str| document().query_selector(q).ok().flatten();
    let (Some(text), Some(baseline), Some(icon), Some(placement), Some(path)) = (
        find("[data-direction-label]"),
        find(".chart-direction-baseline"),
        find("[data-direction-icon]"),
        find("[data-direction-placement]"),
        find("[data-direction-path]"),
    ) else {
        return;
    };
    let Some(style) = window().get_computed_style(&text).ok().flatten() else {
        return;
    };
    let font = format!(
        "{} {} {} {}",
        style.get_property_value("font-style").unwrap_or_default(),
        style.get_property_value("font-weight").unwrap_or_default(),
        style.get_property_value("font-size").unwrap_or_default(),
        style.get_property_value("font-family").unwrap_or_default()
    );
    let ink = labels::ink("Better", &font);
    let Some(ink) = ink else {
        return;
    };
    let center = baseline.get_bounding_client_rect().top()
        + (ink.actual_bounding_box_descent() - ink.actual_bounding_box_ascent()) / 2.;
    let bounds = icon.get_bounding_client_rect();
    let scale = bounds.height() / 32.;
    if scale <= 0. {
        return;
    }
    let bbox = js_sys::Reflect::get(path.as_ref(), &JsValue::from_str("getBBox"))
        .ok()
        .and_then(|f| f.dyn_into::<js_sys::Function>().ok())
        .and_then(|f| f.call0(path.as_ref()).ok());
    if let Some(bbox) = bbox {
        let get = |key: &str| {
            js_sys::Reflect::get(&bbox, &JsValue::from_str(key))
                .ok()
                .and_then(|v| v.as_f64())
                .unwrap_or(0.)
        };
        let arrow_center = bounds.top() + (get("y") + get("height") / 2.) * scale;
        let _ = placement.set_attribute(
            "transform",
            &format!("translate(0 {})", (center - arrow_center) / scale),
        );
    }
}
pub fn refresh_border(a: &App, content: &str) -> String {
    let light = document()
        .document_element()
        .is_some_and(|e| attribute(&e, "data-theme") == "light");
    let template = if light {
        include_str!("../../../assets/refresh-light.html")
    } else {
        include_str!("../../../assets/refresh-dark.html")
    };
    let markup = template.replace("{{CONTENT}}", content);
    if a.refreshing && js_sys::Date::now() - a.refresh_started >= 3000. {
        markup.replacen(
            "data-beam=\"ii-refresh\"",
            "data-beam=\"ii-refresh\" data-active=\"\"",
            1,
        )
    } else {
        markup
    }
}
