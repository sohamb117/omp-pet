use serde::{Deserialize, Serialize};

pub const PET_SIZE: f64 = 112.;
pub const TAB_SIZE: f64 = 8.;
pub const SNAP_DISTANCE: f64 = 24.;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
pub struct Point { pub x: f64, pub y: f64 }
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Rect { pub x: f64, pub y: f64, pub w: f64, pub h: f64 }
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Edge { Left, Right, Bottom, Top }
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Placement { pub origin: Point, pub edge: Option<Edge>, pub screen: Rect }

impl Rect {
    pub fn contains(self, p: Point) -> bool {
        p.x >= self.x && p.x <= self.x + self.w && p.y >= self.y && p.y <= self.y + self.h
    }
    pub fn clamp(self, p: Point, w: f64, h: f64) -> Point {
        Point { x: p.x.clamp(self.x, self.x + (self.w - w).max(0.)),
            y: p.y.clamp(self.y, self.y + (self.h - h).max(0.)) }
    }
}
impl Placement {
    pub fn new(screen: Rect) -> Self {
        Self { origin: Point { x: screen.x + screen.w - PET_SIZE - 24., y: screen.y + 40. }, edge: None, screen }
    }
    pub fn snap(&mut self, origin: Point, screen: Rect) {
        self.screen = screen;
        self.origin = screen.clamp(origin, PET_SIZE, PET_SIZE);
        let distances = [(Edge::Left, (origin.x - screen.x).abs()),
            (Edge::Right, (origin.x + PET_SIZE - screen.x - screen.w).abs()),
            (Edge::Bottom, (origin.y - screen.y).abs()),
            (Edge::Top, (origin.y + PET_SIZE - screen.y - screen.h).abs())];
        self.edge = distances.into_iter().min_by(|a,b| a.1.total_cmp(&b.1))
            .filter(|(_, distance)| *distance <= SNAP_DISTANCE).map(|(edge,_)| edge);
        self.align();
    }
    pub fn align(&mut self) {
        self.origin = self.screen.clamp(self.origin, PET_SIZE, PET_SIZE);
        match self.edge {
            Some(Edge::Left) => self.origin.x = self.screen.x,
            Some(Edge::Right) => self.origin.x = self.screen.x + self.screen.w - PET_SIZE,
            Some(Edge::Bottom) => self.origin.y = self.screen.y,
            Some(Edge::Top) => self.origin.y = self.screen.y + self.screen.h - PET_SIZE,
            None => {}
        }
    }
    pub fn frame(self, tucked: bool) -> Rect {
        let mut r = Rect { x: self.origin.x, y: self.origin.y, w: PET_SIZE, h: PET_SIZE };
        if tucked {
            match self.edge {
                Some(Edge::Left) => r.w = TAB_SIZE,
                Some(Edge::Right) => { r.x += PET_SIZE - TAB_SIZE; r.w = TAB_SIZE; }
                Some(Edge::Bottom) => r.h = TAB_SIZE,
                Some(Edge::Top) => { r.y += PET_SIZE - TAB_SIZE; r.h = TAB_SIZE; }
                None => {}
            }
        }
        r
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tucking_stays_on_selected_monitor_and_preserves_activation_segment() {
        let screen = Rect { x: -1440., y: 200., w: 1440., h: 900. };
        let mut p = Placement::new(screen);
        p.snap(Point { x: -110., y: 450. }, screen);
        assert_eq!(p.edge, Some(Edge::Right));
        let tab = p.frame(true);
        assert_eq!(tab.x + tab.w, 0.);
        assert_eq!(tab.h, PET_SIZE);
        assert!(tab.contains(Point { x: -1., y: 500. }));
        assert!(!tab.contains(Point { x: -1., y: 700. }));
    }
    #[test]
    fn screen_changes_clamp_floating_and_docked_positions() {
        let screen = Rect { x: 0., y: 0., w: 1280., h: 720. };
        let mut p = Placement { origin: Point { x: -1000., y: 3000. }, edge: Some(Edge::Top), screen };
        p.align();
        assert_eq!(p.origin.x, 0.);
        assert_eq!(p.origin.y, 608.);
    }
}
