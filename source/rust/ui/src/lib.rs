use index_core::*;
use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
};
use wasm_bindgen::{JsCast, prelude::*};
use wasm_bindgen_futures::{JsFuture, spawn_local};
use web_sys::*;
mod bar;
mod charts;
mod derived;
mod dialogs;
mod distance;
mod distance_nodes;
mod dom;
mod drawing;
mod exports;
mod labels;
mod pareto;
mod picker;
mod render;
mod retained;
mod table;
mod tooltips;
mod visual;
#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace=Math,js_name=hypot)]
    fn browser_hypot3(x: f64, y: f64, z: f64) -> f64;
    #[wasm_bindgen(js_namespace=Math,js_name=hypot)]
    fn browser_hypot4(x: f64, y: f64, z: f64, w: f64) -> f64;
}
pub(crate) fn camera_turn(b: Quaternion, dx: f64, dy: f64) -> Quaternion {
    let length = js_sys::Math::hypot(dx, dy);
    if length == 0. {
        return b;
    }
    let half = length / 2.;
    let a = Quaternion {
        x: dy / length * js_sys::Math::sin(half),
        y: dx / length * js_sys::Math::sin(half),
        z: 0.,
        w: js_sys::Math::cos(half),
    };
    let q = Quaternion {
        x: a.w * b.x + a.x * b.w + a.y * b.z - a.z * b.y,
        y: a.w * b.y - a.x * b.z + a.y * b.w + a.z * b.x,
        z: a.w * b.z + a.x * b.y - a.y * b.x + a.z * b.w,
        w: a.w * b.w - a.x * b.x - a.y * b.y - a.z * b.z,
    };
    let l = browser_hypot4(q.x, q.y, q.z, q.w);
    Quaternion {
        x: q.x / l,
        y: q.y / l,
        z: q.z / l,
        w: q.w / l,
    }
}
pub(crate) fn distance(model: &Value, scales: &Value, metrics: &[String]) -> Option<f64> {
    if !distance_eligible(model, scales, metrics) {
        return None;
    }
    let values = metrics
        .iter()
        .map(|m| shortfall(n(model, m).unwrap(), m, scales))
        .collect::<Vec<_>>();
    Some(if values.len() == 3 {
        browser_hypot3(values[0], values[1], values[2])
    } else {
        js_sys::Math::hypot(values[0], values[1])
    })
}
pub(crate) fn precision(value: Option<f64>, metric: &str) -> String {
    if parameter_metric(metric) {
        value
            .map(|v| {
                if v >= 1000. {
                    format!("{}T", format_number(v / 1000., 12))
                } else {
                    format!("{}B", format_number(v, 9))
                }
            })
            .unwrap_or("Unreported".into())
    } else {
        index_core::precision(value, metric)
    }
}
pub(crate) const STORAGE: &str = "aa-intelligence-workspace-v1";
pub(crate) fn window() -> Window {
    web_sys::window().unwrap()
}
pub(crate) fn document() -> Document {
    window().document().unwrap()
}
pub(crate) fn element(id: &str) -> Option<Element> {
    document().get_element_by_id(id)
}
pub(crate) fn attribute(e: &Element, key: &str) -> String {
    e.get_attribute(key).unwrap_or_default()
}
pub(crate) fn toggle(set: &mut HashSet<String>, key: String) {
    if !set.remove(&key) {
        set.insert(key);
    }
}
pub(crate) fn listen(target: &EventTarget, name: &str, handler: impl FnMut(Event) + 'static) {
    let closure = Closure::wrap(Box::new(handler) as Box<dyn FnMut(Event)>);
    target
        .add_event_listener_with_callback(name, closure.as_ref().unchecked_ref())
        .unwrap();
    closure.forget();
}
pub(crate) fn later(ms: i32, f: impl FnOnce() + 'static) {
    let closure = Closure::once_into_js(f);
    let _ =
        window().set_timeout_with_callback_and_timeout_and_arguments_0(closure.unchecked_ref(), ms);
}
pub(crate) fn js_error(e: JsValue) -> String {
    e.as_string().unwrap_or_else(|| format!("{e:?}"))
}
pub(crate) async fn request(
    path: &str,
    method: &str,
    body: Option<Value>,
) -> Result<Value, String> {
    let options = RequestInit::new();
    options.set_method(method);
    let _ = js_sys::Reflect::set(&options, &JsValue::from_str("keepalive"), &JsValue::TRUE);
    if let Some(value) = body {
        options.set_body(&JsValue::from_str(&value.to_string()));
    }
    let req = Request::new_with_str_and_init(path, &options).map_err(js_error)?;
    req.headers()
        .set("Content-Type", "application/json")
        .map_err(js_error)?;
    let response = JsFuture::from(window().fetch_with_request(&req))
        .await
        .map_err(js_error)?
        .dyn_into::<Response>()
        .map_err(js_error)?;
    let content = JsFuture::from(response.text().map_err(js_error)?)
        .await
        .map_err(js_error)?
        .as_string()
        .unwrap_or_default();
    let value: Value = serde_json::from_str(&content).map_err(|e| e.to_string())?;
    if response.ok() {
        Ok(value)
    } else {
        Err(value["error"]
            .as_str()
            .unwrap_or("The local application is temporarily unavailable")
            .into())
    }
}
#[derive(Default)]
pub(crate) struct Picker {
    pub draft: HashSet<String>,
    pub query: String,
    pub provider: String,
    pub selected_only: bool,
    pub complete_only: bool,
    pub expanded: HashSet<String>,
    pub providers: HashSet<String>,
    pub saving: bool,
    pub save_error: String,
}
pub(crate) struct Drag {
    pub x: f64,
    pub y: f64,
    pub moved: bool,
    pub mode: String,
    pub original: Option<drawing::Annotation>,
}
pub(crate) struct App {
    pub snapshot_revision: u64,
    pub derived: RefCell<derived::Derived>,
    pub sizes: HashMap<String, (f64, f64)>,
    pub pareto_shown: Vec<String>,
    pub pareto_draw_key: String,
    pub distance_state_key: String,
    pub scan_active: bool,
    pub screenshot_state: String,
    pub screenshot_error: String,
    pub refresh_started: f64,
    pub prefs: Value,
    pub snapshot: Value,
    pub baseline: Option<Value>,
    pub picker: Option<Picker>,
    pub dialog: String,
    pub detail: String,
    pub error: String,
    pub notice: String,
    pub refreshing: bool,
    pub more_filters: bool,
    pub sort: String,
    pub descending: bool,
    pub camera: Quaternion,
    pub pin: String,
    pub hovered: String,
    pub focus: String,
    pub family_focus: bool,
    pub sweep: f64,
    pub playing: bool,
    pub last_frame: f64,
    pub drawing: drawing::Drawing,
    pub drag: Option<Drag>,
    pub plot: charts::Plot,
    pub comparison: Vec<String>,
    pub method_cost: String,
    pub method_tokens: String,
    pub prepared_png: Option<Blob>,
    pub screenshot_busy: bool,
    pub socket: Option<WebSocket>,
    pub page_open: bool,
}
impl App {
    fn new() -> Self {
        let stored = window()
            .local_storage()
            .ok()
            .flatten()
            .and_then(|s| s.get_item(STORAGE).ok().flatten())
            .and_then(|v| serde_json::from_str(&v).ok())
            .unwrap_or(Value::Null);
        Self {
            snapshot_revision: 0,
            derived: RefCell::new(derived::Derived::default()),
            sizes: HashMap::new(),
            pareto_shown: Vec::new(),
            pareto_draw_key: String::new(),
            distance_state_key: String::new(),
            scan_active: false,
            screenshot_state: "idle".into(),
            screenshot_error: String::new(),
            refresh_started: 0.,
            prefs: preferences(stored),
            snapshot: Value::Null,
            baseline: None,
            picker: None,
            dialog: String::new(),
            detail: String::new(),
            error: String::new(),
            notice: String::new(),
            refreshing: false,
            more_filters: false,
            sort: "intelligence".into(),
            descending: true,
            camera: charts::initial_camera(),
            pin: String::new(),
            hovered: String::new(),
            focus: String::new(),
            family_focus: false,
            sweep: 0.,
            playing: false,
            last_frame: 0.,
            drawing: Default::default(),
            drag: None,
            plot: Default::default(),
            comparison: Vec::new(),
            method_cost: String::new(),
            method_tokens: String::new(),
            prepared_png: None,
            screenshot_busy: false,
            socket: None,
            page_open: true,
        }
    }
    pub fn models(&self) -> &[Value] {
        array(&self.snapshot["models"])
    }
    pub fn selected(&self) -> HashSet<String> {
        array(&self.prefs["selected"])
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect()
    }
    pub fn scales(&self) -> &Value {
        if self.prefs["scales"].is_null() {
            &self.snapshot["normalization"]
        } else {
            &self.prefs["scales"]
        }
    }
    pub fn distance_metrics(&self) -> Vec<String> {
        read_distance_metrics(&self.prefs["distanceMetrics"])
    }
    pub fn metric(&self) -> &str {
        s(
            &self.prefs,
            if s(&self.prefs, "view") == "pareto" {
                "paretoMetric"
            } else {
                "metric"
            },
        )
    }
    pub fn size_only(&self) -> bool {
        s(&self.prefs, "view") != "perfection" && parameter_metric(self.metric())
    }
    pub fn mount_plot(&mut self) {
        self.drawing = Default::default();
        self.pin.clear();
        self.comparison.clear();
        self.pareto_draw_key.clear();
        self.distance_state_key.clear();
        self.sweep = 0.;
        self.scan_active = false;
        self.playing = false;
        self.camera = charts::initial_camera();
        self.sort = "intelligence".into();
        self.descending = true;
        self.reset_plot();
    }
    pub fn reset_plot(&mut self) {
        self.focus.clear();
        self.hovered.clear();
        if s(&self.prefs, "view") != "perfection" {
            self.sweep = 0.;
            self.scan_active = false;
            self.playing = false;
        }
        let visible = self.visible();
        let empty = if s(&self.prefs, "view") == "index" {
            !visible.iter().any(|m| n(m, self.metric()).is_some())
        } else {
            visible.is_empty()
        };
        let eligible = visible
            .iter()
            .map(|m| s(m, "id").to_string())
            .collect::<HashSet<_>>();
        if s(&self.prefs, "view") == "perfection" {
            self.pin = self
                .pin
                .split('|')
                .filter(|id| eligible.contains(*id))
                .collect::<Vec<_>>()
                .join("|");
        }
        if empty {
            self.drawing = Default::default();
        }
    }
    pub fn persist(&self) {
        if !self.snapshot.is_null() {
            if let Ok(Some(storage)) = window().local_storage() {
                let _ = storage.set_item(
                    STORAGE,
                    &self.baseline.as_ref().unwrap_or(&self.prefs).to_string(),
                );
            }
        }
    }
    pub fn open_picker(&mut self) {
        self.picker = Some(Picker {
            draft: self.effective(),
            provider: "all".into(),
            ..Default::default()
        });
    }
    pub fn open_method(&mut self) {
        self.method_cost = n(self.scales(), "cost").unwrap_or(1.).to_string();
        self.method_tokens = n(self.scales(), "tokens").unwrap_or(1.).to_string();
        self.dialog = "method".into();
    }
}
thread_local! {pub(crate) static APP:RefCell<App>=RefCell::new(App::new());}
pub(crate) fn update(f: impl FnOnce(&mut App)) {
    APP.with(|c| {
        let mut a = c.borrow_mut();
        let preferences = a.prefs.clone();
        let baseline = a.baseline.clone();
        let loading = a.snapshot.is_null();
        f(&mut a);
        let preferences_changed = preferences != a.prefs;
        if preferences_changed {
            a.prepared_png = None;
        }
        if preferences_changed || baseline != a.baseline || loading && !a.snapshot.is_null() {
            a.persist();
        }
    });
    render::render();
}
pub(crate) fn fail(error: String) {
    update(|a| {
        a.error = error;
        a.refreshing = false;
        if let Some(p) = &mut a.picker {
            p.saving = false;
        }
    });
}
fn session() {
    APP.with(|c| {
        let mut a = c.borrow_mut();
        if !a.page_open || a.socket.is_some() {
            return;
        }
        let Ok(socket) = WebSocket::new("ws://127.0.0.1:4173/api/desktop/session") else {
            return;
        };
        listen(socket.as_ref(), "close", |_| {
            APP.with(|c| c.borrow_mut().socket = None);
            later(500, session);
        });
        a.socket = Some(socket);
    });
}
fn save_selection() {
    let (draft, temporary, keep_global) = APP.with(|c| {
        let mut a = c.borrow_mut();
        let draft = a.picker.as_ref().unwrap().draft.clone();
        let keep = s(&a.prefs, "view") == "pareto"
            && b(&a.prefs, "paretoGlobalFrontier")
            && pareto(a.models(), a.metric())
                .1
                .iter()
                .all(|m| draft.contains(s(m, "id")));
        let effective = a.effective();
        let old = a.selected();
        let ids: Vec<String> = if keep {
            draft
                .iter()
                .filter(|id| old.contains(*id) || !effective.contains(*id))
                .cloned()
                .collect()
        } else {
            draft.into_iter().collect()
        };
        a.picker.as_mut().unwrap().saving = true;
        a.picker.as_mut().unwrap().save_error.clear();
        (ids, a.baseline.is_some(), keep)
    });
    render::render();
    spawn_local(async move {
        let result = if temporary {
            Ok(json!({"selected":draft}))
        } else {
            request("/api/selection", "PUT", Some(json!({"selected":draft}))).await
        };
        match result {
            Ok(value) => update(|a| {
                a.prefs["selected"] = value["selected"].clone();
                if !keep_global {
                    a.prefs["paretoGlobalFrontier"] = json!(false);
                }
                a.picker = None;
                a.reset_plot();
            }),
            Err(_) => update(|a| {
                if let Some(p) = &mut a.picker {
                    p.saving = false;
                    p.save_error =
                        "Keep the app open and apply your selection again to save it.".into();
                }
            }),
        }
    });
}
fn refresh() {
    if APP.with(|c| c.borrow().refreshing) {
        return;
    }
    update(|a| {
        a.refreshing = true;
        a.refresh_started = js_sys::Date::now();
        a.error.clear();
    });
    later(3000, || {
        if APP.with(|c| c.borrow().refreshing) {
            render::render();
        }
    });
    spawn_local(async {
        let result = async {
            request("/api/refresh", "POST", None).await?;
            request("/api/data", "GET", None).await
        }
        .await;
        match result {
            Ok(mut value) => {
                let mut raw = Value::Null;
                retain(&mut value, &mut raw);
                update(|a| {
                    a.set_snapshot(value);
                    let ids: HashSet<String> =
                        a.models().iter().map(|m| s(m, "id").into()).collect();
                    for prefs in std::iter::once(&mut a.prefs).chain(a.baseline.iter_mut()) {
                        prefs["selected"] = json!(
                            array(&prefs["selected"])
                                .iter()
                                .filter(|id| id.as_str().is_some_and(|id| ids.contains(id)))
                                .cloned()
                                .collect::<Vec<_>>()
                        );
                    }
                    a.refreshing = false;
                    a.notice = format!(
                        "Refreshed {} models from Artificial Analysis",
                        a.models().len()
                    );
                    a.reset_plot();
                });
            }
            Err(e) => fail(e),
        }
        later(6000, || update(|a| a.notice.clear()));
    });
}
fn action(action: &str, value: &str) {
    if APP.with(|c| c.borrow().picker.as_ref().is_some_and(|p| p.saving)) {
        return;
    }
    if !matches!(
        action,
        "screenshot" | "copy-prepared" | "save-png" | "dismiss" | "screenshot-dismiss"
    ) {
        APP.with(|c| c.borrow_mut().prepared_png = None);
    }
    match action {
        "apply-selection" => save_selection(),
        "refresh" => refresh(),
        "export" => exports::selection(value),
        "screenshot" => exports::screenshot(),
        "copy-prepared" => exports::copy_prepared(),
        "save-png" => exports::save_png(),
        _ => update(|a| match action {
            "reload" => {
                let _ = window().location().reload();
            }
            "screenshot-dismiss" => {
                a.screenshot_state = "idle".into();
                a.screenshot_error.clear();
                a.prepared_png = None;
            }
            "picker-search-clear" => {
                if let Some(p) = &mut a.picker {
                    p.query.clear();
                    p.expanded.clear();
                    p.providers = if p.provider == "all" {
                        HashSet::new()
                    } else {
                        HashSet::from([p.provider.clone()])
                    };
                }
            }
            "method-restore" => {
                a.prefs["scales"] = Value::Null;
                a.dialog.clear();
                a.reset_plot();
            }
            "view" => {
                if s(&a.prefs, "view") != value {
                    a.prefs["view"] = json!(value);
                    a.mount_plot();
                }
            }
            "metric" => {
                if s(&a.prefs, "view") == "pareto" {
                    if s(&a.prefs, "paretoMetric") != value {
                        a.prefs["paretoMetric"] = json!(value);
                        a.reset_plot();
                    }
                } else if s(&a.prefs, "metric") != value || s(&a.prefs, "chartMode") != "chart" {
                    a.prefs["metric"] = json!(value);
                    a.prefs["chartMode"] = json!("chart");
                    a.mount_plot();
                }
            }
            "mode" => {
                if s(&a.prefs, "chartMode") != value {
                    a.prefs["chartMode"] = json!(value);
                    a.mount_plot();
                }
            }
            "picker" => a.open_picker(),
            "method" => a.open_method(),
            "detail" => {
                a.detail = value.into();
                a.dialog = "detail".into();
            }
            "close" => {
                a.picker = None;
                a.dialog.clear();
            }
            "dismiss" => {
                a.error.clear();
                a.notice.clear();
            }
            "more-filters" => a.more_filters = !a.more_filters,
            "reset-filters" => {
                a.prefs["filters"] = default_filters();
                a.reset_plot();
            }
            "provider" => {
                let old = text(&a.prefs, &["filters", "provider"]);
                a.prefs["filters"]["provider"] = json!(if old == value { "all" } else { value });
                a.reset_plot();
            }
            "temporary" => {
                let previous_view = s(&a.prefs, "view").to_string();
                if let Some(saved) = a.baseline.take() {
                    a.prefs = saved;
                    a.picker = None;
                    a.dialog.clear();
                } else {
                    a.baseline = Some(a.prefs.clone());
                    a.prefs["filters"] = default_filters();
                    a.prefs["paretoGlobalFrontier"] = json!(false);
                    a.open_picker();
                }
                if previous_view != s(&a.prefs, "view") {
                    a.mount_plot();
                } else {
                    a.reset_plot();
                }
            }
            "theme" => {
                a.prepared_png = None;
                let root = document().document_element().unwrap();
                let theme = if attribute(&root, "data-theme") == "light" {
                    "dark"
                } else {
                    "light"
                };
                let _ = root.set_attribute("data-theme", theme);
                if let Ok(Some(storage)) = window().local_storage() {
                    let _ = storage.set_item("aa-intelligence-theme", theme);
                }
            }
            "sort" => {
                a.prepared_png = None;
                if a.sort == value {
                    a.descending = !a.descending;
                } else {
                    a.sort = value.into();
                    a.descending = value == "intelligence";
                }
            }
            "picker-provider" => {
                if let Some(p) = &mut a.picker {
                    toggle(&mut p.providers, value.into());
                }
            }
            "picker-family" => {
                if let Some(p) = &mut a.picker {
                    toggle(&mut p.expanded, value.into());
                }
            }
            "picker-all" | "picker-clear" => {
                let ids = picker::matches(a)
                    .iter()
                    .map(|m| s(m, "id").to_string())
                    .collect::<Vec<_>>();
                let temp = a.baseline.is_some();
                if let Some(p) = &mut a.picker {
                    if action == "picker-all" {
                        p.draft.extend(ids);
                    } else if temp {
                        p.draft.clear();
                    } else {
                        for id in ids {
                            p.draft.remove(&id);
                        }
                    }
                }
            }
            "picker-defaults" => {
                let ids = array(&a.snapshot["defaultModelIds"])
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect();
                a.picker = Some(Picker {
                    draft: ids,
                    provider: "all".into(),
                    ..Default::default()
                });
            }
            "method-default" => {
                a.method_cost = n(&a.snapshot["normalization"], "cost")
                    .unwrap_or(1.)
                    .to_string();
                a.method_tokens = n(&a.snapshot["normalization"], "tokens")
                    .unwrap_or(1.)
                    .to_string();
            }
            "method-apply" => {
                if let (Ok(cost), Ok(tokens)) =
                    (a.method_cost.parse::<f64>(), a.method_tokens.parse::<f64>())
                {
                    if cost.is_finite() && tokens.is_finite() && cost > 0. && tokens > 0. {
                        a.prefs["scales"] = json!({"cost":cost,"tokens":tokens});
                        a.dialog.clear();
                        a.reset_plot();
                    }
                }
            }
            "pareto-focus" | "pareto-family" => {
                a.prepared_png = None;
                a.focus = value.into();
                a.family_focus = action == "pareto-family";
                a.hovered.clear();
            }
            "clear-focus" => {
                a.focus.clear();
                a.hovered.clear();
                a.hovered.clear();
            }
            "zone" => {
                a.prefs["paretoZone"] = json!(value);
                a.hovered.clear();
            }
            "pin" => {
                let mut pins: HashSet<String> = a
                    .pin
                    .split('|')
                    .filter(|v| !v.is_empty())
                    .map(str::to_string)
                    .collect();
                toggle(&mut pins, value.into());
                a.pin = pins.into_iter().collect::<Vec<_>>().join("|");
                a.prepared_png = None;
            }
            "camera-reset" => {
                a.camera = charts::initial_camera();
            }
            "play" => {
                a.scan_active = true;
                a.playing = !a.playing;
                if a.playing && a.sweep >= 1. {
                    a.sweep = 0.;
                }
                a.last_frame = 0.;
            }
            "sweep-reset" => {
                a.scan_active = false;
                a.sweep = 0.;
                a.playing = false;
            }
            "compare-bars" => {
                a.hovered.clear();
                a.comparison = if a.comparison.is_empty() {
                    vec![String::new()]
                } else {
                    Vec::new()
                };
            }
            "bar" => {
                if a.comparison.is_empty() {
                    a.detail = value.into();
                    a.dialog = "detail".into();
                } else if a.comparison.len() > 1 || a.comparison[0].is_empty() {
                    a.comparison = vec![value.into()];
                } else if a.comparison[0] == value {
                    a.comparison = vec![String::new()];
                } else {
                    a.comparison.push(value.into());
                }
            }
            "tool" => {
                if a.drawing.tool != value {
                    a.drawing.color = "#ff0000".into();
                }
                a.drawing.editing = None;
                a.drawing.tool = if a.drawing.tool == value {
                    String::new()
                } else {
                    value.into()
                };
                a.drawing.pending = None;
                a.drawing.selected = None;
                a.hovered.clear();
            }
            "undo" => {
                if a.drawing.pending.take().is_none() {
                    a.drawing.annotations.pop();
                }
                a.drawing.selected = None;
            }
            "clear-drawing" => {
                a.drawing.tool.clear();
                a.drawing.annotations.clear();
                a.drawing.pending = None;
                a.drawing.selected = None;
                a.drawing.editing = None;
            }
            "delete-drawing" => {
                if let Some(i) = a.drawing.selected.take() {
                    a.drawing.annotations.remove(i);
                }
            }
            "edit-brace" => {
                a.drawing.editing = value.parse().ok();
            }
            "flip-brace" => {
                if let Some(i) = a.drawing.editing {
                    a.drawing.annotations[i].flipped = !a.drawing.annotations[i].flipped;
                }
            }
            _ => {}
        }),
    }
    if action == "play" && APP.with(|c| c.borrow().playing) {
        schedule_frame();
    }
}
fn form(e: Event) {
    if APP.with(|c| c.borrow().picker.as_ref().is_some_and(|p| p.saving)) {
        return;
    }
    let Some(target) = e.target().and_then(|t| t.dyn_into::<Element>().ok()) else {
        return;
    };
    let key = attribute(&target, "data-field");
    if key.is_empty() {
        return;
    }
    let input = target.dyn_ref::<HtmlInputElement>();
    let value = input
        .map(|v| v.value())
        .or_else(|| target.dyn_ref::<HtmlSelectElement>().map(|v| v.value()))
        .unwrap_or_default();
    let checked = input.is_some_and(HtmlInputElement::checked);
    if key == "sweep" {
        APP.with(|c| {
            let mut a = c.borrow_mut();
            a.sweep = value.parse::<f64>().unwrap_or(0.) / distance::max_radius(&a);
            a.scan_active = true;
            a.playing = false;
            a.prepared_png = None;
        });
        charts::update_sweep();
        return;
    }
    update(|a| match key.as_str() {
        "filter" => {
            let name = attribute(&target, "data-key");
            a.prefs["filters"][&name] = if name == "minScore" {
                json!(value.parse::<f64>().unwrap_or(0.))
            } else {
                json!(value)
            };
            a.reset_plot();
        }
        "preference" => {
            let key = attribute(&target, "data-key");
            a.prefs[&key] = json!(checked);
            if key == "paretoLogScale" {
                a.hovered.clear();
            } else {
                a.reset_plot();
            }
        }
        "distance-metric" => {
            let mut selected = a.distance_metrics();
            if checked {
                selected.push(value);
            } else {
                selected.retain(|m| *m != value);
            }
            if selected.len() >= 2 {
                a.prefs["distanceMetrics"] = json!(read_distance_metrics(&json!(selected)));
                a.reset_plot();
            }
        }
        "picker-query" => {
            let keys: Vec<_> = families(a.models()).iter().map(|f| f.0.clone()).collect();
            let providers: Vec<String> = a
                .models()
                .iter()
                .map(|m| s(m, "providerSlug").into())
                .collect();
            if let Some(p) = &mut a.picker {
                p.query = value;
                p.expanded = if p.query.trim().is_empty() {
                    HashSet::new()
                } else {
                    keys.into_iter().collect()
                };
                p.providers = if p.query.trim().is_empty() {
                    if p.provider == "all" {
                        HashSet::new()
                    } else {
                        HashSet::from([p.provider.clone()])
                    }
                } else {
                    providers.into_iter().collect()
                };
            }
        }
        "picker-provider" => {
            if let Some(p) = &mut a.picker {
                p.provider = value.clone();
                if value != "all" {
                    p.providers.insert(value);
                }
            }
        }
        "picker-selected" => {
            if let Some(p) = &mut a.picker {
                p.selected_only = checked;
            }
        }
        "picker-complete" => {
            if let Some(p) = &mut a.picker {
                p.complete_only = checked;
            }
        }
        "picker-model" => {
            if let Some(p) = &mut a.picker {
                toggle(&mut p.draft, value);
            }
        }
        "picker-group" => {
            let ids: Vec<String> = families(a.models())
                .iter()
                .find(|f| f.0 == value)
                .map(|f| f.2.iter().map(|m| s(m, "id").into()).collect())
                .unwrap_or_default();
            if let Some(p) = &mut a.picker {
                let all = ids.iter().all(|id| p.draft.contains(id));
                for id in ids {
                    if all {
                        p.draft.remove(&id);
                    } else {
                        p.draft.insert(id);
                    }
                }
            }
        }
        "method-cost" => a.method_cost = value,
        "method-tokens" => a.method_tokens = value,
        "drawing-color" => {
            a.drawing.color = value.clone();
            if let Some(p) = &mut a.drawing.pending {
                p.color = value;
            }
        }
        "brace-text" => {
            a.prepared_png = None;
            if let Some(i) = a.drawing.editing {
                a.drawing.annotations[i].text = value;
            }
        }
        _ => {}
    });
}
fn keyboard(event: KeyboardEvent) {
    let typing = event
        .target()
        .and_then(|t| t.dyn_into::<Element>().ok())
        .is_some_and(|e| matches!(e.tag_name().as_str(), "INPUT" | "TEXTAREA" | "SELECT"));
    if typing && !matches!(event.key().as_str(), "Escape" | "Enter") {
        return;
    }
    let key = event.key();
    if APP.with(|c| c.borrow().drawing.editing.is_some())
        && matches!(key.as_str(), "Enter" | "Escape")
    {
        event.prevent_default();
        update(|a| a.drawing.editing = None);
        return;
    }
    if key == "Enter" || key == " " {
        if let Some(target) = event
            .target()
            .and_then(|t| t.dyn_into::<Element>().ok())
            .filter(|e| e.namespace_uri().as_deref() == Some("http://www.w3.org/2000/svg"))
        {
            if let Ok(Some(button)) = target.closest("[data-action]") {
                event.prevent_default();
                action(
                    &attribute(&button, "data-action"),
                    &attribute(&button, "data-value"),
                );
                return;
            }
        }
    }
    if key == "Escape" {
        APP.with(|c| {
            let mut a = c.borrow_mut();
            if s(&a.prefs, "view") == "pareto" && a.picker.is_none() && a.dialog.is_empty() {
                a.focus.clear();
                a.hovered.clear();
            }
        });
    }
    let mut changed = false;
    APP.with(|c| {
        let mut a = c.borrow_mut();
        match key.as_str() {
            "Escape" => {
                if a.picker.is_some() || !a.dialog.is_empty() {
                    a.picker = None;
                    a.dialog.clear();
                } else if a.drag.is_some() {
                    a.drag = None;
                } else if a.drawing.pending.is_some() {
                    a.drawing.pending = None;
                } else if a.drawing.selected.is_some() {
                    a.drawing.selected = None;
                } else if !a.drawing.tool.is_empty() {
                    a.drawing.tool.clear();
                } else {
                    a.focus.clear();
                    a.hovered.clear();
                }
                changed = true;
            }
            "f" | "F"
                if a.drawing
                    .pending
                    .as_ref()
                    .is_some_and(|p| p.kind == "brace") =>
            {
                if let Some(p) = &mut a.drawing.pending {
                    p.flipped = !p.flipped;
                    changed = true;
                }
            }
            "ArrowLeft" | "ArrowRight" | "ArrowUp" | "ArrowDown"
                if a.drawing.selected.is_some() =>
            {
                let i = a.drawing.selected.unwrap();
                let step = if event.shift_key() { 10. } else { 1. };
                let dx = if key == "ArrowRight" {
                    step
                } else if key == "ArrowLeft" {
                    -step
                } else {
                    0.
                } / a.plot.width;
                let dy = if key == "ArrowDown" {
                    step
                } else if key == "ArrowUp" {
                    -step
                } else {
                    0.
                } / a.plot.height;
                for p in &mut a.drawing.annotations[i].points {
                    p.x += dx;
                    p.y += dy;
                }
                changed = true;
            }
            "Delete" | "Backspace" if a.drawing.selected.is_some() => {
                let i = a.drawing.selected.take().unwrap();
                a.drawing.annotations.remove(i);
                changed = true;
            }
            "z" | "Z" if (event.ctrl_key() || event.meta_key()) && !event.shift_key() => {
                if a.drawing.pending.take().is_none() {
                    a.drawing.annotations.pop();
                }
                a.drawing.selected = None;
                changed = true;
            }
            "Home" if s(&a.prefs, "view") == "perfection" => {
                a.camera = charts::initial_camera();
                changed = true;
            }
            "ArrowLeft" | "ArrowRight" | "ArrowUp" | "ArrowDown"
                if s(&a.prefs, "view") == "perfection"
                    && a.distance_metrics().len() == 3
                    && event
                        .target()
                        .and_then(|t| t.dyn_into::<Element>().ok())
                        .is_some_and(|e| e.id() == "graph-svg") =>
            {
                let (x, y, angle) = match key.as_str() {
                    "ArrowLeft" => (0., 1., -0.1),
                    "ArrowRight" => (0., 1., 0.1),
                    "ArrowUp" => (1., 0., -0.1),
                    _ => (1., 0., 0.1),
                };
                a.camera = camera_turn(a.camera, y * angle, x * angle);
                changed = true;
            }
            _ => {}
        }
    });
    if changed {
        event.prevent_default();
        APP.with(|c| c.borrow_mut().prepared_png = None);
        render::render();
    }
}
fn frame(timestamp: f64) {
    let redraw = APP.with(|c| {
        let mut a = c.borrow_mut();
        if !a.playing {
            return false;
        }
        a.prepared_png = None;
        if a.last_frame > 0. {
            a.sweep = (a.sweep + (timestamp - a.last_frame) / 12000.).min(1.);
        }
        a.last_frame = timestamp;
        if a.sweep >= 1. {
            a.playing = false;
        }
        true
    });
    if redraw {
        charts::update_sweep();
    }
    if APP.with(|c| c.borrow().playing) {
        schedule_frame();
    }
}
thread_local! {static FRAME_PENDING:std::cell::Cell<bool>=const{std::cell::Cell::new(false)};}
fn schedule_frame() {
    if FRAME_PENDING.with(|pending| pending.replace(true)) {
        return;
    }
    let f = Closure::once_into_js(move |t: f64| {
        FRAME_PENDING.with(|pending| pending.set(false));
        frame(t)
    });
    let _ = window().request_animation_frame(f.unchecked_ref());
}
#[wasm_bindgen(start)]
pub fn main() {
    let theme = window()
        .local_storage()
        .ok()
        .flatten()
        .and_then(|s| s.get_item("aa-intelligence-theme").ok().flatten())
        .unwrap_or("dark".into());
    let _ = document()
        .document_element()
        .unwrap()
        .set_attribute("data-theme", &theme);
    let root = element("root").unwrap();
    listen(window().as_ref(), "storage", |e| {
        if let Some(e) = e.dyn_ref::<StorageEvent>() {
            if e.key().is_none_or(|k| k == "aa-intelligence-theme") {
                if let Some(root) = document().document_element() {
                    let _ = root.set_attribute(
                        "data-theme",
                        if e.new_value().as_deref() == Some("light") {
                            "light"
                        } else {
                            "dark"
                        },
                    );
                }
                render::render();
            }
        }
    });
    let broken_image = Closure::wrap(Box::new(|e: Event| {
        if let Some(image) = e
            .target()
            .and_then(|t| t.dyn_into::<HtmlImageElement>().ok())
        {
            if image.class_list().contains("model-logo") {
                let _ = image.style().set_property("visibility", "hidden");
            }
        }
    }) as Box<dyn FnMut(Event)>);
    let _ = root.add_event_listener_with_callback_and_bool(
        "error",
        broken_image.as_ref().unchecked_ref(),
        true,
    );
    broken_image.forget();
    listen(root.as_ref(), "click", |e| {
        let Some(target) = e.target().and_then(|t| t.dyn_into::<Element>().ok()) else {
            return;
        };
        if element("graph-svg").is_some_and(|svg| svg.has_attribute("data-suppress-click")) {
            e.prevent_default();
            return;
        }
        if let Ok(Some(button)) = target.closest("[data-action]") {
            if button.has_attribute("disabled") {
                return;
            }
            if button
                .dyn_ref::<HtmlInputElement>()
                .is_none_or(|i| i.type_() != "checkbox")
            {
                e.prevent_default();
            }
            action(
                &attribute(&button, "data-action"),
                &attribute(&button, "data-value"),
            );
        }
    });
    listen(root.as_ref(), "dblclick", |e| {
        if let Some(target) = e
            .target()
            .and_then(|t| t.dyn_into::<Element>().ok())
            .and_then(|t| t.closest("[data-annotation]").ok().flatten())
        {
            let index = attribute(&target, "data-annotation");
            let brace = index.parse::<usize>().ok().is_some_and(|i| {
                APP.with(|c| {
                    c.borrow()
                        .drawing
                        .annotations
                        .get(i)
                        .is_some_and(|a| a.kind == "brace")
                })
            });
            if brace {
                e.prevent_default();
                action("edit-brace", &index);
            }
        }
    });
    listen(root.as_ref(), "mousedown", |e| {
        if e.target()
            .and_then(|t| t.dyn_into::<Element>().ok())
            .is_some_and(|t| t.closest(".chart-brace-flip-btn").ok().flatten().is_some())
        {
            e.prevent_default();
        }
    });
    listen(root.as_ref(), "focusout", |e| {
        if e.target()
            .and_then(|t| t.dyn_into::<Element>().ok())
            .is_some_and(|t| t.id() == "brace-text")
            && APP.with(|c| c.borrow().drawing.editing.is_some())
        {
            update(|a| a.drawing.editing = None);
        }
    });
    listen(root.as_ref(), "input", |e| {
        if e.target()
            .and_then(|t| t.dyn_into::<HtmlInputElement>().ok())
            .is_some_and(|i| i.type_() != "checkbox")
        {
            form(e);
        }
    });
    listen(root.as_ref(), "change", |e| {
        if e.target()
            .and_then(|t| t.dyn_into::<HtmlInputElement>().ok())
            .is_none_or(|i| i.type_() == "checkbox")
        {
            form(e);
        }
    });
    listen(document().as_ref(), "keydown", |e| {
        if let Ok(e) = e.dyn_into() {
            keyboard(e)
        }
    });
    for name in ["pointerdown", "pointermove", "pointerup", "pointercancel"] {
        listen(document().as_ref(), name, move |e| {
            if let Ok(e) = e.dyn_into::<PointerEvent>() {
                drawing::pointer(name, e);
            }
        });
    }
    for name in ["pointerover", "pointermove", "focusin"] {
        listen(root.as_ref(), name, move |e| {
            let Some(target) = e.target().and_then(|t| t.dyn_into::<Element>().ok()) else {
                return;
            };
            let Some(model) = target.closest("[data-hover]").ok().flatten() else {
                return;
            };
            if APP.with(|c| c.borrow().drag.is_some()) {
                return;
            }
            if let Ok(nodes) = document().query_selector_all("[data-current-hover]") {
                for i in 0..nodes.length() {
                    if let Some(n) = nodes.item(i).and_then(|n| n.dyn_into::<Element>().ok()) {
                        let _ = n.remove_attribute("data-current-hover");
                    }
                }
            }
            let _ = model.set_attribute("data-current-hover", "");
            let id = attribute(&model, "data-hover");
            let changed = APP.with(|c| {
                let mut a = c.borrow_mut();
                let changed = a.hovered != id;
                a.hovered = id.clone();
                changed
            });
            if changed && APP.with(|c| s(&c.borrow().prefs, "view") == "perfection") {
                distance::update_hover();
            }
            if name == "focusin" && APP.with(|c| s(&c.borrow().prefs, "view") != "perfection") {
                let r = model.get_bounding_client_rect();
                tooltips::show(&model, &id, r.left(), r.top());
            }
            if name != "focusin" {
                let mouse = e
                    .dyn_ref::<MouseEvent>()
                    .map(|m| (m.client_x() as f64, m.client_y() as f64))
                    .unwrap_or((0., 0.));
                tooltips::show(&model, &id, mouse.0, mouse.1);
            }
        });
    }
    for name in ["pointerout", "focusout"] {
        listen(root.as_ref(), name, |e| {
            let inside = e
                .dyn_ref::<MouseEvent>()
                .and_then(|m| m.related_target())
                .or_else(|| e.dyn_ref::<FocusEvent>().and_then(|e| e.related_target()))
                .and_then(|t| t.dyn_into::<Element>().ok())
                .is_some_and(|t| {
                    t.closest("[data-hover],#chart-tooltip")
                        .ok()
                        .flatten()
                        .is_some()
                });
            if !inside {
                APP.with(|c| c.borrow_mut().hovered.clear());
                if APP.with(|c| s(&c.borrow().prefs, "view") == "perfection") {
                    distance::update_hover();
                }
                tooltips::hide();
            }
        });
    }
    listen(root.as_ref(), "cancel", |e| {
        e.prevent_default();
        action("close", "");
    });
    listen(root.as_ref(), "scroll", |_| {
        APP.with(|c| c.borrow_mut().hovered.clear());
        tooltips::hide();
    });
    listen(window().as_ref(), "resize", |_| render::render());
    listen(window().as_ref(), "pagehide", |_| {
        APP.with(|c| {
            let mut a = c.borrow_mut();
            a.page_open = false;
            if let Some(socket) = a.socket.take() {
                let _ = socket.close();
            }
        })
    });
    listen(window().as_ref(), "pageshow", |_| {
        APP.with(|c| c.borrow_mut().page_open = true);
        session();
    });
    session();
    render::render();
    spawn_local(async {
        if let Ok(promise) = document().fonts().ready() {
            let _ = JsFuture::from(promise).await;
            labels::invalidate();
            render::render();
        }
    });
    spawn_local(async {
        let result = async {
            let mut data = request("/api/data", "GET", None).await?;
            let selection = request("/api/selection", "GET", None).await?;
            let mut raw = Value::Null;
            retain(&mut data, &mut raw);
            let selected = if selection["selected"].is_null() {
                let prefs = APP.with(|c| c.borrow().prefs["selected"].clone());
                let initial = if array(&prefs).is_empty() {
                    data["defaultModelIds"].clone()
                } else {
                    prefs
                };
                request(
                    "/api/selection",
                    "PUT",
                    Some(json!({"selected":initial,"initialize":true})),
                )
                .await?["selected"]
                    .clone()
            } else {
                selection["selected"].clone()
            };
            Ok::<_, String>((data, selected))
        }
        .await;
        match result {
            Ok((data, selected)) => update(|a| {
                a.set_snapshot(data);
                a.prefs["selected"] = selected;
            }),
            Err(e) => fail(e),
        }
    });
}
