use crate::*;
use js_sys::{Array, Function, Object, Promise, Reflect};
fn blob(text: &str, mime: &str) -> Result<Blob, JsValue> {
    let parts = Array::new();
    parts.push(&JsValue::from_str(text));
    let opts = BlobPropertyBag::new();
    opts.set_type(mime);
    Blob::new_with_str_sequence_and_options(&parts, &opts)
}
fn download(image: &Blob, name: &str) -> Result<(), JsValue> {
    let url = Url::create_object_url_with_blob(image)?;
    let a = document()
        .create_element("a")?
        .dyn_into::<HtmlAnchorElement>()?;
    a.set_href(&url);
    a.set_download(name);
    document().body().unwrap().append_child(&a)?;
    a.click();
    a.remove();
    later(60000, move || {
        let _ = Url::revoke_object_url(&url);
    });
    Ok(())
}
pub fn save_png() {
    let png = APP.with(|c| c.borrow().prepared_png.clone());
    if let Some(png) = png {
        if let Err(e) = download(&png, "intelligence-index-screenshot.png") {
            fail(js_error(e));
        } else {
            update(|a| {
                a.prepared_png = None;
                a.screenshot_error.clear();
                a.screenshot_state = "idle".into();
            });
        }
    }
}
pub fn selection(format: &str) {
    let output=APP.with(|c|{let a=c.borrow();let metrics=a.distance_metrics();let rows:Vec<_>=a.visible().iter().map(|m|json!({"model":m["name"],"provider":m["provider"],"intelligence_index":m["intelligence"],"total_cost_usd":m["cost"],"total_output_tokens":m["tokens"],"end_to_end_seconds":m["time"],"input_price_per_million_tokens":m["priceInputPerMillion"],"output_price_per_million_tokens":m["priceOutputPerMillion"],"total_parameters_billions":m["parameters"],"active_parameters_billions":m["activeParameters"],"model_weights_url":m["weightsUrl"],"total_parameters_source":m["parameterSources"]["parameters"]["url"].as_str().unwrap_or(s(m,"sourceUrl")),"active_parameters_source":m["parameterSources"]["activeParameters"]["url"].as_str().unwrap_or(s(m,"sourceUrl")),"parameter_sources":m["parameterSources"],"open_weights":m["openWeights"],"license":m["license"],"reasoning":m["reasoning"],"deprecated":m["deprecated"],"distance":distance(m,a.scales(),&metrics),"distance_metrics":metrics.join(" + "),"source":m["sourceUrl"],"source_model":m["pricing"]["sourceModelName"].as_str().unwrap_or(s(m,"name")),"cost_method":if m["pricing"].is_object(){if m["cost"].is_null(){"Awaiting source breakdown"}else{"Calculated from source usage"}}else{"Published"},"pricing_source":m["pricing"]["sourceUrl"].as_str().unwrap_or(s(m,"sourceUrl")),"pricing":m["pricing"]})).collect();
    if format=="json"{serde_json::to_string_pretty(&json!({"updatedAt":a.snapshot["updatedAt"],"indexVersion":a.snapshot["indexVersion"],"distanceNormalization":a.scales(),"distanceMetrics":metrics,"distanceFormula":distance_formula(&metrics),"models":rows})).unwrap()}else{let headings=["model","provider","intelligence_index","total_cost_usd","total_output_tokens","end_to_end_seconds","input_price_per_million_tokens","output_price_per_million_tokens","total_parameters_billions","active_parameters_billions","model_weights_url","total_parameters_source","active_parameters_source","open_weights","license","reasoning","deprecated","distance","distance_metrics","source","source_model","cost_method","pricing_source"];let mut lines=vec![headings.join(",")];lines.extend(rows.iter().map(|row|headings.iter().map(|key|csv_cell(&row[key])).collect::<Vec<_>>().join(",")));lines.join("\r\n")}});
    let result = blob(
        &output,
        if format == "json" {
            "application/json"
        } else {
            "text/csv;charset=utf-8"
        },
    )
    .and_then(|b| download(&b, &format!("intelligence-index-selection.{format}")));
    if let Err(e) = result {
        fail(js_error(e));
    }
    if let Ok(Some(menu)) = document().query_selector(".export-menu") {
        let _ = menu.remove_attribute("open");
    }
}
fn style(e: &Element, value: &str) {
    let _ = e.set_attribute("style", value);
}
fn all(e: &Element) -> Vec<Element> {
    let mut result = vec![e.clone()];
    if let Ok(nodes) = e.query_selector_all("*") {
        for i in 0..nodes.length() {
            if let Some(e) = nodes.item(i).and_then(|n| n.dyn_into::<Element>().ok()) {
                result.push(e);
            }
        }
    }
    result
}
fn frozen(source: &Element) -> Result<Element, JsValue> {
    let copy = source.clone_node_with_deep(true)?.dyn_into::<Element>()?;
    let originals = all(source);
    let copies = all(&copy);
    for (original, copy) in originals.iter().zip(&copies) {
        if let Some(computed) = window().get_computed_style(original)? {
            let mut css = String::new();
            for i in 0..computed.length() {
                let key = computed.item(i);
                if key == "font" {
                    continue;
                }
                css.push_str(&format!("{key}:{};", computed.get_property_value(&key)?));
            }
            style(copy, &css);
        }
        if let Some(input) = original.dyn_ref::<HtmlInputElement>() {
            copy.set_attribute("value", &input.value())?;
            if input.checked() {
                copy.set_attribute("checked", "")?;
            }
        }
        if original.tag_name().eq_ignore_ascii_case("svg") {
            let b = original.get_bounding_client_rect();
            let mut css = attribute(copy, "style");
            css.push_str(&format!(
                "width:{}px;height:{}px;max-height:none;",
                b.width(),
                b.height()
            ));
            style(copy, &css);
        }
    }
    for (original, copy) in originals.iter().zip(&copies) {
        if original.has_attribute("data-screenshot-parameters") {
            let mut labels = Vec::new();
            if let Ok(selects) = original.query_selector_all("select") {
                for i in 0..selects.length() {
                    if let Some(select) = selects
                        .item(i)
                        .and_then(|n| n.dyn_into::<HtmlSelectElement>().ok())
                    {
                        let option = select.query_selector("option:checked")?;
                        labels.push(option.and_then(|v| v.text_content()).unwrap_or_default());
                    }
                }
            }
            copy.set_inner_html(
                &labels
                    .iter()
                    .map(|v| format!("<span>{}</span>", html(v)))
                    .collect::<String>(),
            );
        }
        if original.matches(".chart-scroll,.table-scroll,.ranking-list")? {
            let viewport = document().create_element("div")?;
            style(
                &viewport,
                &format!(
                    "position:absolute;left:0;top:0;width:{}px;height:{}px;overflow:hidden",
                    original.client_width(),
                    original.client_height()
                ),
            );
            let mut css = attribute(copy, "style");
            css.push_str("position:relative;overflow:hidden;");
            style(copy, &css);
            while let Some(child) = copy.first_child() {
                viewport.append_child(&child)?;
            }
            let children = original.query_selector_all(":scope > *")?;
            let copied_children = viewport.query_selector_all(":scope > *")?;
            let parent_bounds = original.get_bounding_client_rect();
            for i in 0..children.length() {
                if let (Some(source), Some(child)) = (
                    children.item(i).and_then(|n| n.dyn_into::<Element>().ok()),
                    copied_children
                        .item(i)
                        .and_then(|n| n.dyn_into::<Element>().ok()),
                ) {
                    let bounds = source.get_bounding_client_rect();
                    let mut css = attribute(&child, "style");
                    css.push_str(&format!("position:absolute;left:{}px;top:{}px;width:{}px;height:{}px;max-width:none;margin:0;",bounds.x()-parent_bounds.x(),bounds.y()-parent_bounds.y(),bounds.width(),bounds.height()));
                    style(&child, &css);
                }
            }
            copy.append_child(&viewport)?;
            if original.matches(".table-scroll")? {
                if let Ok(headers) = copy.query_selector_all("th") {
                    for i in 0..headers.length() {
                        if let Some(e) = headers.item(i).and_then(|n| n.dyn_into::<Element>().ok())
                        {
                            let mut css = attribute(&e, "style");
                            css.push_str(&format!(
                                "position:relative;top:auto;transform:translateY({}px)",
                                original.scroll_top()
                            ));
                            style(&e, &css);
                        }
                    }
                }
            }
        }
    }
    if let Ok(nodes) = copy.query_selector_all("[data-screenshot-exclude]") {
        for i in 0..nodes.length() {
            if let Some(e) = nodes.item(i).and_then(|n| n.dyn_into::<Element>().ok()) {
                e.remove();
            }
        }
    }
    let bounds = source.get_bounding_client_rect();
    let mut css = attribute(&copy, "style");
    css.push_str(&format!(
        "width:{}px;height:{}px;box-sizing:border-box;margin:0;flex:0 0 auto;",
        bounds.width(),
        bounds.height()
    ));
    style(&copy, &css);
    Ok(copy)
}
fn data_url(blob: &Blob) -> Promise {
    let reader = FileReader::new().unwrap();
    Promise::new(&mut |resolve, reject| {
        let r = reader.clone();
        let closure = Closure::once_into_js(move |_: Event| match r.result() {
            Ok(value) => {
                let _ = resolve.call1(&JsValue::UNDEFINED, &value);
            }
            Err(e) => {
                let _ = reject.call1(&JsValue::UNDEFINED, &e);
            }
        });
        reader.set_onloadend(Some(closure.unchecked_ref()));
        let _ = reader.read_as_data_url(blob);
    })
}
async fn asset(url: &str) -> Result<String, JsValue> {
    let response = JsFuture::from(window().fetch_with_str(url))
        .await?
        .dyn_into::<Response>()?;
    if !response.ok() {
        return Err(JsValue::from_str("Asset unavailable"));
    }
    let mime = response.headers().get("Content-Type")?.unwrap_or_default();
    if mime.contains("html") {
        return Err(JsValue::from_str("Asset unavailable"));
    }
    let b = JsFuture::from(response.blob()?).await?.dyn_into::<Blob>()?;
    Ok(JsFuture::from(data_url(&b))
        .await?
        .as_string()
        .unwrap_or_default())
}
async fn fonts() -> Result<String, JsValue> {
    let response = JsFuture::from(window().fetch_with_str("/assets/fonts.css"))
        .await?
        .dyn_into::<Response>()?;
    let mut css = JsFuture::from(response.text()?)
        .await?
        .as_string()
        .unwrap_or_default();
    let mut urls = Vec::new();
    let mut rest = css.as_str();
    while let Some(pos) = rest.find("url(") {
        rest = &rest[pos + 4..];
        let Some(end) = rest.find(')') else {
            break;
        };
        urls.push(rest[..end].trim_matches(['\'', '"']).to_string());
        rest = &rest[end + 1..];
    }
    for url in urls {
        let full = if url.starts_with('/') {
            url.clone()
        } else {
            format!("/assets/{url}")
        };
        if let Ok(data) = asset(&full).await {
            css = css.replace(&url, &data);
        }
    }
    Ok(css)
}
async fn render_png(
    frame: Element,
    width: f64,
    height: f64,
    background: String,
) -> Result<Blob, JsValue> {
    for e in all(&frame) {
        let key = if e.tag_name().eq_ignore_ascii_case("img") {
            Some("src")
        } else if e.tag_name().eq_ignore_ascii_case("image") {
            Some("href")
        } else {
            None
        };
        if let Some(key) = key {
            let src = attribute(&e, key);
            if !src.starts_with("data:") {
                let inline=asset(&src).await.unwrap_or_else(|_|"data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='1' height='1'/%3E".into());
                e.set_attribute(key, &inline)?;
            }
        }
    }
    let styles = document().create_element("style")?;
    styles.set_text_content(Some(&fonts().await?));
    frame.prepend_with_node_1(&styles)?;
    let content = XmlSerializer::new()?.serialize_to_string(&frame)?;
    let svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{width}\" height=\"{height}\" viewBox=\"0 0 {width} {height}\"><foreignObject width=\"100%\" height=\"100%\">{content}</foreignObject></svg>"
    );
    let source = blob(&svg, "image/svg+xml;charset=utf-8")?;
    let image = HtmlImageElement::new()?;
    image.set_src(
        &JsFuture::from(data_url(&source))
            .await?
            .as_string()
            .unwrap_or_default(),
    );
    JsFuture::from(image.decode()).await?;
    let mut scale = (32_000_000. / (width * height))
        .sqrt()
        .min(16384. / width.max(height));
    let minimum = scale.min(1.);
    loop {
        let canvas = document()
            .create_element("canvas")?
            .dyn_into::<HtmlCanvasElement>()?;
        canvas.set_width((width * scale).floor().max(1.) as u32);
        canvas.set_height((height * scale).floor().max(1.) as u32);
        let ctx = canvas
            .get_context("2d")?
            .ok_or_else(|| JsValue::from_str("Canvas allocation failed"))?
            .dyn_into::<CanvasRenderingContext2d>()?;
        let result = async {
            ctx.draw_image_with_html_image_element_and_dw_and_dh(
                &image,
                0.,
                0.,
                canvas.width() as f64,
                canvas.height() as f64,
            )?;
            ctx.set_global_composite_operation("destination-over")?;
            ctx.set_fill_style_str(&background);
            ctx.fill_rect(0., 0., canvas.width() as f64, canvas.height() as f64);
            let promise = Promise::new(&mut |resolve, reject| {
                let callback = Closure::once_into_js(move |value: JsValue| {
                    let _ = resolve.call1(&JsValue::UNDEFINED, &value);
                });
                if let Err(e) = canvas.to_blob_with_type(callback.unchecked_ref(), "image/png") {
                    let _ = reject.call1(&JsValue::UNDEFINED, &e);
                }
            });
            JsFuture::from(promise).await?.dyn_into::<Blob>()
        }
        .await;
        canvas.set_width(0);
        canvas.set_height(0);
        if let Ok(result) = result {
            if result.size() > 0. {
                return Ok(result);
            }
        }
        if scale <= minimum {
            break;
        }
        scale = (scale / 2.).max(minimum);
    }
    Err(JsValue::from_str(
        "Try a smaller graph viewport and copy again",
    ))
}
fn frame() -> Result<(Element, f64, f64, String), JsValue> {
    let workspace =
        element("workspace").ok_or_else(|| JsValue::from_str("Open a graph or table first"))?;
    let bounds = workspace.get_bounding_client_rect();
    let root = document().document_element().unwrap();
    let background = window()
        .get_computed_style(&root)?
        .map(|s| s.get_property_value("--page").unwrap_or_default())
        .filter(|v| !v.is_empty())
        .unwrap_or("#000".into());
    let width = bounds.width() + 104.;
    let frame = document().create_element("div")?;
    frame.set_attribute("xmlns", "http://www.w3.org/1999/xhtml")?;
    style(
        &frame,
        &format!(
            "box-sizing:border-box;display:flex;flex-direction:column;gap:16px;width:{width}px;padding:40px 52px;background:{background};color:var(--text-primary);font-family:'DM Sans Variable';"
        ),
    );
    let header = document().create_element("div")?;
    style(
        &header,
        "display:flex;flex-direction:column;align-items:flex-start;gap:8px;",
    );
    for selector in ["[data-screenshot-heading]", "[data-screenshot-filters]"] {
        if let Some(source) = document().query_selector(selector)? {
            let copy = frozen(&source)?;
            let mut css = attribute(&copy, "style");
            css.push_str("width:auto;height:auto;padding:0;flex:0 0 auto;");
            style(&copy, &css);
            header.append_child(&copy)?;
        }
    }
    frame.append_child(&header)?;
    let workspace_copy = frozen(&workspace)?;
    frame.append_child(&workspace_copy)?;
    // Measure only the frozen output. It is hidden from the interactive page.
    let wrapper = document().create_element("div")?;
    style(
        &wrapper,
        "position:fixed;left:-100000px;top:0;pointer-events:none;",
    );
    wrapper.append_child(&frame)?;
    document().body().unwrap().append_child(&wrapper)?;
    let height = frame.get_bounding_client_rect().height().ceil();
    wrapper.remove();
    Ok((frame, width.ceil(), height, background))
}
fn clipboard(promise: &Promise) -> Result<Promise, JsValue> {
    let record = Object::new();
    Reflect::set(&record, &JsValue::from_str("image/png"), promise)?;
    let item = ClipboardItem::new_with_record_from_str_to_blob_promise(&record)?;
    let items = Array::new();
    items.push(&item);
    Ok(window().navigator().clipboard().write(&items))
}
pub fn screenshot() {
    if APP.with(|c| c.borrow().screenshot_busy) {
        return;
    }
    let mut resolve_fn: Option<Function> = None;
    let mut reject_fn: Option<Function> = None;
    let promise = Promise::new(&mut |resolve, reject| {
        resolve_fn = Some(resolve);
        reject_fn = Some(reject);
    });
    let writing = clipboard(&promise);
    let frozen = frame();
    update(|a| {
        a.screenshot_busy = true;
        a.screenshot_state = "copying".into();
        a.screenshot_error.clear();
        a.prepared_png = None;
    });
    let resolve = resolve_fn.unwrap();
    let reject = reject_fn.unwrap();
    spawn_local(async move {
        let result = match frozen {
            Ok((frame, w, h, bg)) => render_png(frame, w, h, bg).await,
            Err(e) => Err(e),
        };
        match result {
            Ok(image) => {
                let _ = resolve.call1(&JsValue::UNDEFINED, &image);
                update(|a| {
                    a.prepared_png = Some(image);
                    a.screenshot_busy = false;
                });
                let success = match writing {
                    Ok(p) => JsFuture::from(p).await.is_ok(),
                    Err(_) => false,
                };
                if success {
                    later(3000, || {
                        update(|a| {
                            if a.screenshot_state == "copied" {
                                a.screenshot_state = "idle".into();
                            }
                        })
                    });
                }
                update(|a| {
                    if success {
                        a.prepared_png = None;
                    }
                    a.screenshot_state = if success { "copied" } else { "ready" }.into();
                });
            }
            Err(e) => {
                let _ = reject.call1(&JsValue::UNDEFINED, &e);
                if let Ok(p) = writing {
                    let _ = JsFuture::from(p).await;
                }
                update(|a| {
                    a.screenshot_busy = false;
                    a.screenshot_error = js_error(e);
                    a.screenshot_state = "error".into();
                });
            }
        }
    });
}

pub fn copy_prepared() {
    if APP.with(|c| c.borrow().screenshot_busy) {
        return;
    }
    let image = APP.with(|c| c.borrow().prepared_png.clone());
    let Some(image) = image else {
        return;
    };
    let result = clipboard(&Promise::resolve::<JsValue>(&JsValue::from(image)));
    update(|a| {
        a.screenshot_busy = true;
        a.screenshot_state = "copying".into();
    });
    spawn_local(async move {
        let success = match result {
            Ok(p) => JsFuture::from(p).await.is_ok(),
            Err(_) => false,
        };
        update(|a| {
            a.screenshot_busy = false;
            if success {
                a.prepared_png = None;
                a.screenshot_error.clear();
                a.screenshot_state = "copied".into();
            } else {
                a.screenshot_error =
                    "Allow clipboard access in site settings or save the PNG.".into();
                a.screenshot_state = "error".into();
            }
        });
        if success {
            later(3000, || {
                update(|a| {
                    if a.screenshot_state == "copied" {
                        a.screenshot_state = "idle".into();
                    }
                })
            });
        }
    });
}
