#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

pub fn clamp(cursor: (i32, i32), size: (i32, i32), work: Rect) -> Rect {
    let width = size.0.max(1).min((work.right - work.left).max(1));
    let height = size.1.max(1).min((work.bottom - work.top).max(1));
    let x = if cursor.0.saturating_add(width) > work.right {
        cursor.0.saturating_sub(width)
    } else {
        cursor.0
    };
    let y = if cursor.1.saturating_add(height) > work.bottom {
        cursor.1.saturating_sub(height)
    } else {
        cursor.1
    };
    let left = x.clamp(work.left, work.right - width);
    let top = y.clamp(work.top, work.bottom - height);
    Rect {
        left,
        top,
        right: left + width,
        bottom: top + height,
    }
}
/// Keeps a remembered top-left corner, shifted just enough to fit the work area.
pub fn fit(origin: (i32, i32), size: (i32, i32), work: Rect) -> Rect {
    let width = size.0.max(1).min((work.right - work.left).max(1));
    let height = size.1.max(1).min((work.bottom - work.top).max(1));
    let left = origin.0.clamp(work.left, work.right - width);
    let top = origin.1.clamp(work.top, work.bottom - height);
    Rect {
        left,
        top,
        right: left + width,
        bottom: top + height,
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn remembered_position_and_size_fit_the_work_area() {
        let w = Rect {
            left: 0,
            top: 0,
            right: 3440,
            bottom: 1392,
        };
        let r = |left, top, right, bottom| Rect {
            left,
            top,
            right,
            bottom,
        };
        assert_eq!(fit((100, 200), (600, 700), w), r(100, 200, 700, 900));
        assert_eq!(fit((3300, 1300), (600, 700), w), r(2840, 692, 3440, 1392));
        assert_eq!(fit((-50, -9), (600, 700), w), r(0, 0, 600, 700));
        assert_eq!(fit((10, 10), (5000, 2000), w), w);
    }
    #[test]
    fn edges_negative_monitor_and_oversized() {
        let w = Rect {
            left: -1920,
            top: 0,
            right: 0,
            bottom: 1040,
        };
        for p in [(-1920, 0), (-1, 0), (-1920, 1039), (-1, 1039), (-1000, 500)] {
            let r = clamp(p, (600, 700), w);
            assert!(
                r.left >= w.left && r.top >= w.top && r.right <= w.right && r.bottom <= w.bottom
            );
        }
        assert_eq!(
            clamp((-1, 1039), (600, 700), w),
            Rect {
                left: -601,
                top: 339,
                right: -1,
                bottom: 1039
            }
        );
        assert_eq!(clamp((-100, 30), (9999, 9999), w), w);
        for dpi in [96, 120, 144, 192] {
            let r = clamp((-1, 1039), (480 * dpi / 96, 480 * dpi / 96), w);
            assert!(r.right <= 0 && r.bottom <= 1040);
        }
    }
}
