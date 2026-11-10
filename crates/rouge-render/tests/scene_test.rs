use rouge_render::{Color, DrawList, Quad, Rect, Shadow};

#[test]
fn primitives_accumulate_until_cleared() {
    let mut list = DrawList::new();
    list.push_quad(Quad::new(BOUNDS, Color::WHITE));
    list.push_shadow(Shadow::new(BOUNDS, Color::BLACK, 4.0));
    list.push_quad(Quad::new(BOUNDS, Color::WHITE));
    assert_eq!(list.len(), 3);
    list.clear();
    assert!(list.is_empty());
}

#[test]
fn invisible_primitives_are_culled() {
    let mut list = DrawList::new();
    list.push_quad(Quad::new(Rect::ZERO, Color::WHITE));
    list.push_quad(Quad {
        clip: Rect::new(50.0, 50.0, 5.0, 5.0),
        ..Quad::new(BOUNDS, Color::WHITE)
    });
    list.push_shadow(Shadow::new(BOUNDS, Color::TRANSPARENT, 4.0));
    assert!(list.is_empty());
}

const BOUNDS: Rect = Rect::new(0.0, 0.0, 10.0, 10.0);
