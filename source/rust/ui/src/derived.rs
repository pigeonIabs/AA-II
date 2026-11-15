use crate::*;

// Own indices rather than copies of model records. Preferences are the complete
// dependency key, so changing a filter, scale or selection cannot reuse stale data.
#[derive(Default)]
pub struct Derived {
    revision: u64,
    preferences: Value,
    effective: HashSet<String>,
    visible: Vec<usize>,
    ranked: Option<Vec<(usize, f64)>>,
    coordinates: HashMap<String, [f64; 3]>,
}
impl Derived {
    fn new(a: &App) -> Self {
        let view = s(&a.prefs, "view");
        let global: HashSet<String> = if view == "pareto" && b(&a.prefs, "paretoGlobalFrontier") {
            pareto(a.models(), a.metric())
                .1
                .iter()
                .map(|m| s(m, "id").to_owned())
                .collect()
        } else {
            HashSet::new()
        };
        let mut effective = a.selected();
        effective.extend(global.iter().cloned());
        let metrics = a.distance_metrics();
        let visible = a
            .models()
            .iter()
            .enumerate()
            .filter(|(_, m)| {
                effective.contains(s(m, "id"))
                    && (global.contains(s(m, "id"))
                        || matches_filters(m, &a.prefs["filters"], a.size_only()))
                    && match view {
                        "perfection" => distance_eligible(m, a.scales(), &metrics),
                        "pareto" => pareto_eligible(m),
                        _ => true,
                    }
            })
            .map(|(i, _)| i)
            .collect();
        Self {
            revision: a.snapshot_revision,
            preferences: a.prefs.clone(),
            effective,
            visible,
            ranked: None,
            coordinates: HashMap::new(),
        }
    }
    fn fill_ranking(&mut self, a: &App) {
        if self.ranked.is_some() {
            return;
        }
        let metrics = a.distance_metrics();
        let axes = distance::axes(a);
        let mut ranked = self
            .visible
            .iter()
            .filter_map(|i| {
                let m = &a.models()[*i];
                distance(m, a.scales(), &metrics).map(|d| (*i, d))
            })
            .collect::<Vec<_>>();
        ranked.sort_by(|(ia, x), (ib, y)| {
            let (ma, mb) = (&a.models()[*ia], &a.models()[*ib]);
            x.total_cmp(y)
                .then(
                    n(mb, "intelligence")
                        .unwrap()
                        .total_cmp(&n(ma, "intelligence").unwrap()),
                )
                .then_with(|| visual::compare(s(ma, "name"), s(mb, "name")))
        });
        for (index, _) in &ranked {
            let m = &a.models()[*index];
            let point = std::array::from_fn(|i| {
                if i < axes.len() {
                    100. - shortfall(n(m, &axes[i]).unwrap(), &axes[i], a.scales())
                } else {
                    0.
                }
            });
            self.coordinates.insert(s(m, "id").to_owned(), point);
        }
        self.ranked = Some(ranked);
    }
}
impl App {
    fn derived(&self) -> std::cell::RefMut<'_, Derived> {
        let mut cache = self.derived.borrow_mut();
        if cache.revision != self.snapshot_revision || cache.preferences != self.prefs {
            *cache = Derived::new(self);
        }
        cache
    }
    pub fn effective(&self) -> HashSet<String> {
        self.derived().effective.clone()
    }
    pub fn effective_count(&self) -> usize {
        self.derived().effective.len()
    }
    pub fn visible(&self) -> Vec<&Value> {
        self.derived()
            .visible
            .iter()
            .map(|i| &self.models()[*i])
            .collect()
    }
    pub fn ranked(&self) -> Vec<(&Value, f64)> {
        let mut cache = self.derived();
        cache.fill_ranking(self);
        cache
            .ranked
            .as_ref()
            .unwrap()
            .iter()
            .map(|(i, d)| (&self.models()[*i], *d))
            .collect()
    }
    pub fn coordinates(&self, model: &Value) -> [f64; 3] {
        let mut cache = self.derived();
        cache.fill_ranking(self);
        cache
            .coordinates
            .get(s(model, "id"))
            .copied()
            .unwrap_or_default()
    }
    pub fn distance_radius(&self) -> f64 {
        let mut cache = self.derived();
        cache.fill_ranking(self);
        (cache
            .ranked
            .as_ref()
            .unwrap()
            .iter()
            .map(|(_, d)| *d)
            .fold(100., f64::max)
            / 10.)
            .ceil()
            * 10.
    }
    pub fn set_snapshot(&mut self, snapshot: Value) {
        self.snapshot = snapshot;
        self.snapshot_revision = self.snapshot_revision.wrapping_add(1);
    }
}
