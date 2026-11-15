use crate::*;

#[derive(Clone, PartialEq)]
struct Workspace {
    snapshot: u64,
    preferences: Value,
    sizes: HashMap<String, (f64, f64)>,
    drawing: drawing::Drawing,
    camera: Quaternion,
    comparison: Vec<String>,
    sort: String,
    descending: bool,
    pin: String,
    hovered: String,
    focus: String,
    family_focus: bool,
    sweep: f64,
    scan_active: bool,
    playing: bool,
    dragging: bool,
    light: bool,
}
impl Workspace {
    fn of(a: &App) -> Self {
        Self {
            snapshot: a.snapshot_revision,
            preferences: a.prefs.clone(),
            sizes: a.sizes.clone(),
            drawing: a.drawing.clone(),
            camera: a.camera,
            comparison: a.comparison.clone(),
            sort: a.sort.clone(),
            descending: a.descending,
            pin: a.pin.clone(),
            hovered: a.hovered.clone(),
            focus: a.focus.clone(),
            family_focus: a.family_focus,
            sweep: a.sweep,
            scan_active: a.scan_active,
            playing: a.playing,
            dragging: a
                .drag
                .as_ref()
                .is_some_and(|d| d.mode == "camera" && d.moved),
            light: document()
                .document_element()
                .is_some_and(|e| attribute(&e, "data-theme") == "light"),
        }
    }
}
#[derive(Default)]
struct Cache {
    state: Option<Workspace>,
    revision: u64,
}
thread_local! { static CACHE: RefCell<Cache> = RefCell::new(Cache::default()); }
pub fn invalidate() {
    CACHE.with(|c| c.borrow_mut().state = None);
}

// Unchanged regions are represented by an empty, revision-marked shell. The DOM
// reconciler retains their live children without generating or parsing them again.
// No duplicate chart HTML or second live chart tree is kept in memory.
pub fn workspace(a: &mut App) -> String {
    let state = Workspace::of(a);
    let (changed, revision) = CACHE.with(|c| {
        let mut cache = c.borrow_mut();
        let changed = cache.state.as_ref() != Some(&state);
        if changed {
            cache.revision = cache.revision.wrapping_add(1);
        }
        (changed, cache.revision)
    });
    let content = if changed {
        distance_nodes::invalidate();
        let chart = charts::workspace(a);
        let legend = render::legend(a);
        CACHE.with(|c| c.borrow_mut().state = Some(Workspace::of(a)));
        legend + &chart
    } else {
        String::new()
    };
    format!(
        r#"<div id="workspace" class="{}" data-screenshot-target data-rust-retain="{revision}">{content}</div>"#,
        if s(&a.prefs, "view") == "index" {
            "index-workspace"
        } else {
            "visual-workspace"
        }
    )
}
