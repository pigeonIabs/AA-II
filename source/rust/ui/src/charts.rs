use crate::*;
#[derive(Clone, Copy, Default)]
pub struct Plot {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub canvas_width: f64,
    pub canvas_height: f64,
}
impl Plot {
    pub fn pixel(&self, p: Point) -> Point {
        Point {
            x: self.x + p.x * self.width,
            y: self.y + p.y * self.height,
        }
    }
}
pub fn initial_camera() -> Quaternion {
    camera_turn(camera_turn(Quaternion::default(), -0.6, 0.), 0., 0.4)
}
pub fn workspace(a: &mut App) -> String {
    match s(&a.prefs, "view") {
        "pareto" => crate::pareto::render(a),
        "perfection" => {
            crate::distance::prepare(a);
            crate::distance::render(a)
        }
        _ => {
            if s(&a.prefs, "chartMode") == "table" {
                crate::table::render(a)
            } else {
                crate::bar::render(a)
            }
        }
    }
}
pub fn update_sweep() {
    crate::distance::update_sweep()
}
