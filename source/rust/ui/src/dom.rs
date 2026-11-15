use crate::*;

// Retain the browser's focus, image instances, scroll positions and pointer targets.
// Parsing the new markup does not replace unchanged live elements.
pub fn render(root: &Element, markup: &str) {
    let Ok(staging) = (if root.namespace_uri().as_deref() == Some("http://www.w3.org/2000/svg") {
        document().create_element_ns(Some("http://www.w3.org/2000/svg"), "svg")
    } else {
        document().create_element("div")
    }) else {
        return;
    };
    staging.set_inner_html(markup);
    children(root.as_ref(), staging.as_ref());
}
pub fn update_element(current: &Element, markup: &str) {
    if let Ok(staging) = document().create_element("div") {
        staging.set_inner_html(markup);
        if let Some(desired) = staging.first_child() {
            patch(current.as_ref(), &desired);
        }
    }
}
fn clean(node: &Node) -> Node {
    let nodes = node.child_nodes();
    let children = (0..nodes.length())
        .filter_map(|i| nodes.item(i))
        .collect::<Vec<_>>();
    if let Some(e) = node
        .dyn_ref::<Element>()
        .filter(|e| e.namespace_uri().as_deref() == Some("http://www.w3.org/2000/svg"))
    {
        // Build SVG through the same namespace and property APIs as the original UI.
        // Children precede initial properties, matching its host element construction.
        if let Ok(native) =
            document().create_element_ns(Some("http://www.w3.org/2000/svg"), &e.local_name())
        {
            for child in children.iter().filter(|n| n.node_type() != 8) {
                let _ = native.append_child(&clean(child));
            }
            for name in e
                .get_attribute_names()
                .iter()
                .filter_map(|n| n.as_string())
                .filter(|n| n != "style")
            {
                let _ = native.set_attribute(&name, &e.get_attribute(&name).unwrap_or_default());
            }
            if let Some(style) = e.get_attribute("style") {
                inline_style(&native, &style);
            }
            return native.unchecked_into();
        }
    }
    for child in children {
        if child.node_type() == 8 {
            let _ = node.remove_child(&child);
        } else {
            let materialized = clean(&child);
            if materialized != child {
                let _ = node.replace_child(&materialized, &child);
            }
        }
    }
    if let Some(e) = node.dyn_ref::<Element>() {
        if let Some(style) = e.get_attribute("style") {
            inline_style(e, &style);
        }
    }
    node.clone()
}
fn inline_style(e: &Element, value: &str) {
    // Match property assignment in the reference renderer, including fractional SVG sizes.
    let style = js_sys::Reflect::get(e, &JsValue::from_str("style"))
        .ok()
        .and_then(|v| v.dyn_into::<CssStyleDeclaration>().ok());
    let Some(style) = style else {
        return;
    };
    let properties = value
        .split(';')
        .filter_map(|declaration| declaration.split_once(':'))
        .map(|(key, value)| (key.trim(), value.trim()))
        .filter(|(key, _)| !key.is_empty())
        .collect::<Vec<_>>();
    let previous = (0..style.length())
        .map(|i| style.item(i))
        .collect::<Vec<_>>();
    for key in previous {
        if !properties.iter().any(|(name, _)| *name == key) {
            let _ = style.remove_property(&key);
        }
    }
    for (key, value) in properties {
        let _ = style.set_property(key, value);
    }
}
fn key(node: &Node) -> String {
    let Some(e) = node.dyn_ref::<Element>() else {
        return String::new();
    };
    for name in ["id", "data-bar-key", "data-model-id", "data-location"] {
        if let Some(value) = e.get_attribute(name) {
            return format!("{name}:{value}");
        }
    }
    String::new()
}
fn compatible(a: &Node, b: &Node) -> bool {
    if a.node_type() != b.node_type() {
        return false;
    }
    if let (Some(a), Some(b)) = (a.dyn_ref::<Element>(), b.dyn_ref::<Element>()) {
        a.tag_name() == b.tag_name() && a.namespace_uri() == b.namespace_uri()
    } else {
        true
    }
}
fn children(current: &Node, desired: &Node) {
    let wanted = desired.child_nodes();
    let wanted = (0..wanted.length())
        .filter_map(|i| wanted.item(i))
        .filter(|n| n.node_type() != 8)
        .collect::<Vec<_>>();
    for (index, next) in wanted.iter().enumerate() {
        let mut live = current.child_nodes().item(index as u32);
        let desired_key = key(next);
        if !desired_key.is_empty() && live.as_ref().is_none_or(|n| key(n) != desired_key) {
            let candidates = current.child_nodes();
            if let Some(found) = (index as u32 + 1..candidates.length())
                .filter_map(|i| candidates.item(i))
                .find(|n| key(n) == desired_key && compatible(n, next))
            {
                let _ = current.insert_before(&found, live.as_ref());
                live = Some(found);
            }
        }
        match live {
            Some(ref node)
                if compatible(node, next)
                    && (desired_key.is_empty() || key(node) == desired_key) =>
            {
                patch(node, next);
            }
            Some(ref node) => {
                let next = clean(next);
                let _ = current.insert_before(&next, Some(node));
            }
            None => {
                let next = clean(next);
                let _ = current.append_child(&next);
            }
        }
    }
    while current.child_nodes().length() > wanted.len() as u32 {
        if let Some(last) = current.last_child() {
            let _ = current.remove_child(&last);
        } else {
            break;
        }
    }
}
fn patch(current: &Node, desired: &Node) {
    if let (Some(current), Some(desired)) =
        (current.dyn_ref::<Element>(), desired.dyn_ref::<Element>())
    {
        let retain_children = desired
            .get_attribute("data-rust-retain")
            .is_some_and(|revision| {
                current.get_attribute("data-rust-retain").as_deref() == Some(&revision)
            });
        let select_value = desired.dyn_ref::<HtmlSelectElement>().map(|e| e.value());
        let names = current.get_attribute_names();
        for name in names.iter().filter_map(|v| v.as_string()) {
            if !desired.has_attribute(&name) {
                if name == "data-current-hover" {
                    continue;
                }
                if name == "open" && matches!(current.tag_name().as_str(), "DIALOG" | "DETAILS") {
                    continue;
                }
                let _ = current.remove_attribute(&name);
            }
        }
        for name in desired
            .get_attribute_names()
            .iter()
            .filter_map(|v| v.as_string())
        {
            let value = desired.get_attribute(&name).unwrap_or_default();
            if current.get_attribute(&name).as_deref() != Some(&value) {
                if name == "style" {
                    inline_style(current, &value);
                } else {
                    let _ = current.set_attribute(&name, &value);
                }
            }
        }
        if !retain_children {
            children(current.as_ref(), desired.as_ref());
        }
        if let (Some(current), Some(desired)) = (
            current.dyn_ref::<HtmlInputElement>(),
            desired.dyn_ref::<HtmlInputElement>(),
        ) {
            if current.value() != desired.value() {
                current.set_value(&desired.value());
            }
            if current.checked() != desired.checked() {
                current.set_checked(desired.checked());
            }
            current.set_indeterminate(desired.has_attribute("data-indeterminate"));
        }
        if let (Some(current), Some(value)) = (current.dyn_ref::<HtmlSelectElement>(), select_value)
        {
            if current.value() != value {
                current.set_value(&value);
            }
        }
    } else if current.node_value() != desired.node_value() {
        current.set_node_value(desired.node_value().as_deref());
    }
}
