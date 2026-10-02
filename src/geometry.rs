use serde::{Deserialize, Serialize};

pub const PET_SIZE: f64 = 112.;
pub const TAB_SIZE: f64 = 3.;
pub const SNAP_DISTANCE: f64 = 24.;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Edge {
    Left,
    Right,
    Bottom,
    Top,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Placement {
    pub origin: Point,
    pub edge: Option<Edge>,
    pub screen: Rect,
    #[serde(default = "default_size")]
    pub size: f64,
}

fn default_size() -> f64 {
    PET_SIZE
}

impl Rect {
    pub fn contains(self, p: Point) -> bool {
        p.x >= self.x && p.x <= self.x + self.w && p.y >= self.y && p.y <= self.y + self.h
    }
    pub fn clamp(self, p: Point, w: f64, h: f64) -> Point {
        Point {
            x: p.x.clamp(self.x, self.x + (self.w - w).max(0.)),
            y: p.y.clamp(self.y, self.y + (self.h - h).max(0.)),
        }
    }
}
impl Placement {
    pub fn new(screen: Rect) -> Self {
        Self {
            origin: Point {
                x: screen.x + screen.w - PET_SIZE - 24.,
                y: screen.y + 40.,
            },
            edge: None,
            screen,
            size: PET_SIZE,
        }
    }
    pub fn snap(&mut self, origin: Point, screen: Rect) {
        self.screen = screen;
        self.origin = screen.clamp(origin, self.size, self.size);
        let origin = self.origin;
        let distances = [
            (Edge::Left, (origin.x - screen.x).abs()),
            (
                Edge::Right,
                (origin.x + self.size - screen.x - screen.w).abs(),
            ),
            (Edge::Bottom, (origin.y - screen.y).abs()),
            (
                Edge::Top,
                (origin.y + self.size - screen.y - screen.h).abs(),
            ),
        ];
        self.edge = distances
            .into_iter()
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .filter(|(_, distance)| *distance <= SNAP_DISTANCE)
            .map(|(edge, _)| edge);
        self.align();
    }
    pub fn align(&mut self) {
        self.origin = self.screen.clamp(self.origin, self.size, self.size);
        match self.edge {
            Some(Edge::Left) => self.origin.x = self.screen.x,
            Some(Edge::Right) => self.origin.x = self.screen.x + self.screen.w - self.size,
            Some(Edge::Bottom) => self.origin.y = self.screen.y,
            Some(Edge::Top) => self.origin.y = self.screen.y + self.screen.h - self.size,
            None => {}
        }
    }
    /// The pointer can reach the physical edge through Dock/menu-bar insets.
    pub fn tucked_frame(self, physical: Rect) -> Rect {
        let mut r = self.frame(true);
        match self.edge {
            Some(Edge::Left) => r.x = physical.x,
            Some(Edge::Right) => r.x = physical.x + physical.w - TAB_SIZE,
            Some(Edge::Bottom) => r.y = physical.y,
            Some(Edge::Top) => r.y = physical.y + physical.h - TAB_SIZE,
            None => {}
        }
        r
    }
    pub fn activation(self, physical: Rect) -> Rect {
        let mut r = self.frame(false);
        match self.edge {
            Some(Edge::Left) => {
                r.x = physical.x;
                r.w = (self.screen.x - physical.x + 8.).max(8.);
            }
            Some(Edge::Right) => {
                r.x = self.screen.x + self.screen.w - 8.;
                r.w = (physical.x + physical.w - r.x).max(8.);
            }
            Some(Edge::Bottom) => {
                r.y = physical.y;
                r.h = (self.screen.y - physical.y + 8.).max(8.);
            }
            Some(Edge::Top) => {
                r.y = self.screen.y + self.screen.h - 8.;
                r.h = (physical.y + physical.h - r.y).max(8.);
            }
            None => {
                r.w = 0.;
                r.h = 0.;
            }
        }
        // Extend activation to physical corners when the pet is near a usable-frame corner.
        let grace = self.size / 2. + SNAP_DISTANCE;
        if matches!(self.edge, Some(Edge::Left | Edge::Right)) {
            if self.origin.y <= self.screen.y + grace {
                let end = r.y + r.h;
                r.y = physical.y;
                r.h = end - r.y;
            }
            if self.origin.y + self.size >= self.screen.y + self.screen.h - grace {
                r.h = physical.y + physical.h - r.y;
            }
        } else if matches!(self.edge, Some(Edge::Top | Edge::Bottom)) {
            if self.origin.x <= self.screen.x + grace {
                let end = r.x + r.w;
                r.x = physical.x;
                r.w = end - r.x;
            }
            if self.origin.x + self.size >= self.screen.x + self.screen.w - grace {
                r.w = physical.x + physical.w - r.x;
            }
        }
        r
    }
    pub fn frame(self, tucked: bool) -> Rect {
        let mut r = Rect {
            x: self.origin.x,
            y: self.origin.y,
            w: self.size,
            h: self.size,
        };
        if tucked {
            match self.edge {
                Some(Edge::Left) => r.w = TAB_SIZE,
                Some(Edge::Right) => {
                    r.x += self.size - TAB_SIZE;
                    r.w = TAB_SIZE;
                }
                Some(Edge::Bottom) => r.h = TAB_SIZE,
                Some(Edge::Top) => {
                    r.y += self.size - TAB_SIZE;
                    r.h = TAB_SIZE;
                }
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
        let screen = Rect {
            x: -1440.,
            y: 200.,
            w: 1440.,
            h: 900.,
        };
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
    fn dragging_the_sprite_center_into_an_edge_docks_even_if_frame_overhangs() {
        let screen = Rect {
            x: 0.,
            y: 80.,
            w: 1512.,
            h: 869.,
        };
        for (origin, edge) in [
            (Point { x: 1456., y: 300. }, Edge::Right),
            (Point { x: -56., y: 300. }, Edge::Left),
            (Point { x: 500., y: 24. }, Edge::Bottom),
            (Point { x: 500., y: 893. }, Edge::Top),
        ] {
            let mut p = Placement::new(screen);
            p.snap(origin, screen);
            assert_eq!(p.edge, Some(edge));
        }
    }
    #[test]
    fn activation_reaches_physical_edge_across_dock_inset() {
        let physical = Rect {
            x: 0.,
            y: 0.,
            w: 1512.,
            h: 982.,
        };
        let mut p = Placement::new(Rect {
            x: 0.,
            y: 80.,
            w: 1512.,
            h: 869.,
        });
        p.edge = Some(Edge::Bottom);
        p.align();
        assert_eq!(p.tucked_frame(physical).y, 0.);
        assert!(p.activation(physical).contains(Point {
            x: p.origin.x + 56.,
            y: 0.
        }));
        assert!(!p.activation(physical).contains(Point {
            x: p.origin.x - 10.,
            y: 0.
        }));
    }
    #[test]
    fn corner_reveal_and_resized_tabs_cover_display_insets() {
        let physical = Rect {
            x: -1512.,
            y: 0.,
            w: 1512.,
            h: 982.,
        };
        let usable = Rect {
            x: -1432.,
            y: 80.,
            w: 1432.,
            h: 869.,
        };
        for edge in [Edge::Left, Edge::Right, Edge::Bottom, Edge::Top] {
            for size in [64., 112., 200.] {
                let mut p = Placement::new(usable);
                p.size = size;
                p.edge = Some(edge);
                p.origin = Point {
                    x: usable.x + 40.,
                    y: usable.y + 40.,
                };
                p.align();
                let corner = match edge {
                    Edge::Left | Edge::Bottom => Point {
                        x: physical.x,
                        y: physical.y,
                    },
                    Edge::Right => Point {
                        x: physical.x + physical.w,
                        y: physical.y,
                    },
                    Edge::Top => Point {
                        x: physical.x,
                        y: physical.y + physical.h,
                    },
                };
                assert!(
                    p.activation(physical).contains(corner),
                    "{edge:?} size {size}"
                );
                let tab = p.tucked_frame(physical);
                assert_eq!(
                    if matches!(edge, Edge::Left | Edge::Right) {
                        tab.h
                    } else {
                        tab.w
                    },
                    size
                );
            }
        }
    }
    #[test]
    fn screen_changes_clamp_floating_and_docked_positions() {
        let screen = Rect {
            x: 0.,
            y: 0.,
            w: 1280.,
            h: 720.,
        };
        let mut p = Placement {
            origin: Point {
                x: -1000.,
                y: 3000.,
            },
            edge: Some(Edge::Top),
            screen,
            size: PET_SIZE,
        };
        p.align();
        assert_eq!(p.origin.x, 0.);
        assert_eq!(p.origin.y, 608.);
    }
}
