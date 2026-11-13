use serde::{Deserialize, Serialize};

pub const MIN_WIDTH: f32 = 240.0;
pub const GAP: f32 = 24.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, clap::ValueEnum)]
#[serde(rename_all = "kebab-case")]
pub enum Corner {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

impl Corner {
    pub fn left(self) -> bool {
        matches!(self, Self::TopLeft | Self::BottomLeft)
    }

    pub fn top(self) -> bool {
        matches!(self, Self::TopLeft | Self::TopRight)
    }

    fn from_sides(left: bool, top: bool) -> Self {
        match (left, top) {
            (true, true) => Self::TopLeft,
            (false, true) => Self::TopRight,
            (true, false) => Self::BottomLeft,
            (false, false) => Self::BottomRight,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Size {
    pub width: f32,
    pub height: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Rect {
    pub fn contains(&self, x: f32, y: f32) -> bool {
        x >= self.x && x < self.x + self.width && y >= self.y && y < self.y + self.height
    }

    pub fn moved(&self, dx: f32, dy: f32) -> Self {
        Self { x: self.x + dx, y: self.y + dy, ..*self }
    }

    pub fn size(&self) -> Size {
        Size { width: self.width, height: self.height }
    }
}

pub fn in_corner(screen: Size, size: Size, corner: Corner) -> Rect {
    let x = if corner.left() { GAP } else { screen.width - size.width - GAP };
    let y = if corner.top() { GAP } else { screen.height - size.height - GAP };
    Rect { x, y, width: size.width, height: size.height }
}

pub fn nearest_corner(screen: Size, rect: Rect) -> Corner {
    let left = rect.x + rect.width / 2.0 < screen.width / 2.0;
    let top = rect.y + rect.height / 2.0 < screen.height / 2.0;
    Corner::from_sides(left, top)
}

pub fn fitted_width(screen: Size, width: f32) -> f32 {
    width.clamp(MIN_WIDTH, (screen.width - 2.0 * GAP).max(MIN_WIDTH))
}

pub fn sized(width: f32, aspect: f32) -> Size {
    Size { width, height: (width / aspect).round() }
}

pub fn resized(rect: Rect, corner: Corner, dx: f32, dy: f32, aspect: f32, screen: Size) -> Rect {
    let grow_x = if corner.left() { dx } else { -dx };
    let grow_y = if corner.top() { dy } else { -dy };
    let growth = if grow_x.abs() > (grow_y * aspect).abs() { grow_x } else { grow_y * aspect };
    let size = sized(fitted_width(screen, rect.width + growth), aspect);
    in_corner(screen, size, corner)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gesture {
    Move,
    Resize,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Drag {
    pub gesture: Gesture,
    pub from: Rect,
    pub corner: Corner,
    pub aspect: f32,
}

impl Drag {
    pub fn after(&self, dx: f32, dy: f32, screen: Size) -> Rect {
        match self.gesture {
            Gesture::Move => self.from.moved(dx, dy),
            Gesture::Resize => resized(self.from, self.corner, dx, dy, self.aspect, screen),
        }
    }

    pub fn dropped_in(&self, rect: Rect, screen: Size) -> Corner {
        match self.gesture {
            Gesture::Move => nearest_corner(screen, rect),
            Gesture::Resize => self.corner,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_move_lands_in_the_nearest_corner_and_a_resize_stays_put() {
        let from = in_corner(SCREEN, SMALL, Corner::BottomRight);
        let drag = |gesture| Drag { gesture, from, corner: Corner::BottomRight, aspect: 16.0 / 9.0 };
        let moved = drag(Gesture::Move).after(-1200.0, -700.0, SCREEN);
        assert_eq!(drag(Gesture::Move).dropped_in(moved, SCREEN), Corner::TopLeft);
        let grown = drag(Gesture::Resize).after(-1200.0, -700.0, SCREEN);
        assert_eq!(drag(Gesture::Resize).dropped_in(grown, SCREEN), Corner::BottomRight);
    }

    const SCREEN: Size = Size { width: 1920.0, height: 1080.0 };
    const SMALL: Size = Size { width: 480.0, height: 270.0 };

    #[test]
    fn bottom_right_keeps_a_gap_from_both_edges() {
        let rect = in_corner(SCREEN, SMALL, Corner::BottomRight);
        assert_eq!((rect.x, rect.y), (1920.0 - 480.0 - GAP, 1080.0 - 270.0 - GAP));
    }

    #[test]
    fn a_window_dropped_anywhere_snaps_to_the_corner_it_is_closest_to() {
        let dropped = Rect { x: 1000.0, y: 100.0, width: 480.0, height: 270.0 };
        assert_eq!(nearest_corner(SCREEN, dropped), Corner::TopRight);
        assert_eq!(nearest_corner(SCREEN, dropped.moved(-800.0, 600.0)), Corner::BottomLeft);
    }

    #[test]
    fn every_corner_is_its_own_nearest_corner() {
        [Corner::TopLeft, Corner::TopRight, Corner::BottomLeft, Corner::BottomRight]
            .into_iter()
            .for_each(|corner| {
                assert_eq!(nearest_corner(SCREEN, in_corner(SCREEN, SMALL, corner)), corner)
            });
    }

    #[test]
    fn resizing_keeps_the_corner_pinned_and_the_shape() {
        let start = in_corner(SCREEN, SMALL, Corner::BottomRight);
        let bigger = resized(start, Corner::BottomRight, -160.0, 0.0, 16.0 / 9.0, SCREEN);
        assert_eq!(bigger.size(), Size { width: 640.0, height: 360.0 });
        assert_eq!(bigger, in_corner(SCREEN, bigger.size(), Corner::BottomRight));
    }

    #[test]
    fn width_never_goes_below_the_minimum_or_past_the_screen() {
        assert_eq!(fitted_width(SCREEN, 10.0), MIN_WIDTH);
        assert_eq!(fitted_width(SCREEN, 5000.0), 1920.0 - 2.0 * GAP);
    }
}
