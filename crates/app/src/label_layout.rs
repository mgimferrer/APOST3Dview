//! Where the labels drawn over the molecule go on screen, so they never
//! overlap (item 4 of the improvement backlog, 2026-10-19): measurements
//! first, never left out (the user asked for each one); then the others
//! from the nearest to the camera, each kept at its place, or moved up or
//! down by up to a line when that place is taken, or left out when even
//! that overlaps. A value left out is still in the APOST-3D window's
//! tables; rotating or zooming usually brings it back.

use egui::{Pos2, Rect, Vec2};

/// A label to place: where it wants to be on screen (pixels), how big it
/// is, and how it may be treated.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Request {
    pub(crate) center: Pos2,
    pub(crate) size: Vec2,
    /// Placed in increasing order: the nearest labels first.
    pub(crate) order: f32,
    /// May be moved up or down to make room.
    pub(crate) may_move: bool,
    /// Always placed, even over another label (measurements).
    pub(crate) required: bool,
}

/// Space kept between two labels, in pixels.
const PADDING: f32 = 2.0;

/// The offset (pixels, y down) each request gets, `None` for one left out.
pub(crate) fn place(requests: &[Request]) -> Vec<Option<Vec2>> {
    let mut order: Vec<usize> = (0..requests.len()).collect();
    // Required first, then by order; ties by index, for the same layout
    // every time.
    order.sort_by(|&a, &b| {
        let (ra, rb) = (&requests[a], &requests[b]);
        rb.required.cmp(&ra.required).then(ra.order.total_cmp(&rb.order)).then(a.cmp(&b))
    });
    let mut taken: Vec<Rect> = Vec::new();
    let mut result = vec![None; requests.len()];
    for index in order {
        let request = &requests[index];
        let line = request.size.y + PADDING;
        let tries: &[f32] = if request.may_move { &[0.0, -1.0, 1.0] } else { &[0.0] };
        let fits = |offset: Vec2| {
            let rect = Rect::from_center_size(request.center + offset, request.size).expand(PADDING / 2.0);
            !taken.iter().any(|t| overlap(*t, rect))
        };
        let offset = tries.iter().map(|&k| Vec2::new(0.0, k * line)).find(|&o| fits(o));
        let offset = match offset {
            Some(offset) => Some(offset),
            None if request.required => Some(Vec2::ZERO),
            None => None,
        };
        if let Some(offset) = offset {
            taken.push(Rect::from_center_size(request.center + offset, request.size).expand(PADDING / 2.0));
            result[index] = Some(offset);
        }
    }
    result
}

/// Whether two rectangles share some area (touching edges don't).
fn overlap(a: Rect, b: Rect) -> bool {
    a.min.x < b.max.x && b.min.x < a.max.x && a.min.y < b.max.y && b.min.y < a.max.y
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(x: f32, y: f32, order: f32) -> Request {
        Request { center: Pos2::new(x, y), size: Vec2::new(40.0, 16.0), order, may_move: true, required: false }
    }

    #[test]
    fn apart_labels_stay_where_they_are() {
        let placed = place(&[request(0.0, 0.0, 1.0), request(100.0, 0.0, 2.0)]);
        assert_eq!(placed, vec![Some(Vec2::ZERO), Some(Vec2::ZERO)]);
    }

    #[test]
    fn the_farther_label_moves_then_is_left_out() {
        // Three on the same spot: the nearest stays, the next moves up a
        // line, the third down a line; a fourth has no room.
        let placed = place(&[request(0.0, 0.0, 3.0), request(0.0, 0.0, 1.0), request(0.0, 0.0, 2.0), request(0.0, 0.0, 4.0)]);
        assert_eq!(placed[1], Some(Vec2::ZERO));
        assert_eq!(placed[2], Some(Vec2::new(0.0, -18.0)));
        assert_eq!(placed[0], Some(Vec2::new(0.0, 18.0)));
        assert_eq!(placed[3], None);
    }

    #[test]
    fn required_labels_are_always_placed_and_first() {
        let measurement = Request { required: true, may_move: false, ..request(0.0, 0.0, 9.0) };
        let placed = place(&[request(0.0, 0.0, 1.0), measurement, measurement]);
        assert_eq!(placed[1], Some(Vec2::ZERO));
        assert_eq!(placed[2], Some(Vec2::ZERO), "even over another required one");
        assert_eq!(placed[0], Some(Vec2::new(0.0, -18.0)), "the value makes room");
    }

    #[test]
    fn a_label_that_may_not_move_is_left_out() {
        let fixed = Request { may_move: false, ..request(0.0, 0.0, 2.0) };
        assert_eq!(place(&[request(0.0, 0.0, 1.0), fixed]), vec![Some(Vec2::ZERO), None]);
    }
}
