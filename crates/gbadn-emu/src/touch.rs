//! On-screen buttons for phones, with several fingers at once.

use std::collections::HashMap;

use eframe::egui::{self, Align2, Color32, FontId, Pos2, Rect, Stroke, TouchPhase, pos2, vec2};

use crate::keys;

const SCREEN_ASPECT: f32 = crate::WIDTH as f32 / crate::HEIGHT as f32;
/// Fingers are imprecise, so buttons accept touches a little outside what they draw.
const HIT_MARGIN: f32 = 1.2;
/// Touches this close to the D-pad's centre press nothing.
const DPAD_DEAD_ZONE: f32 = 0.2;
/// How far off an axis a touch may lean and still press that direction, giving diagonals.
const DPAD_DIAGONAL: f32 = 0.38;
const FILL: Color32 = Color32::from_rgba_premultiplied(38, 38, 38, 38);
const OUTLINE: Color32 = Color32::from_rgba_premultiplied(120, 120, 120, 120);
const LABEL: Color32 = Color32::from_rgba_premultiplied(210, 210, 210, 210);

/// Where the controls sit in an area, which leaves the centre to the game.
struct Layout {
    dpad: (Pos2, f32),
    round: [(Pos2, f32, u32, &'static str); 2],
    pills: [(Rect, u32, &'static str); 4],
}

fn layout(area: Rect) -> Layout {
    let (width, height) = (area.width(), area.height());
    let side = ((width - height * SCREEN_ASPECT) / 2.0).max(height * 0.35);
    let middle = area.center().y;
    let pill = vec2(side * 0.8, height * 0.12);

    let dpad_radius = (side * 0.42).min(height * 0.24);
    let button_radius = dpad_radius * 0.48;
    let right = area.right();
    let left = area.left();

    let top_left = Rect::from_min_size(pos2(left + side * 0.1, area.top() + height * 0.02), pill);
    let top_right = Rect::from_min_size(pos2(right - side * 0.9, area.top() + height * 0.02), pill);
    let bottom_left =
        Rect::from_min_size(pos2(left + side * 0.1, area.bottom() - height * 0.14), pill);
    let bottom_right = Rect::from_min_size(
        pos2(right - side * 0.9, area.bottom() - height * 0.14),
        pill,
    );

    Layout {
        dpad: (pos2(left + side / 2.0, middle), dpad_radius),
        round: [
            (
                pos2(right - side * 0.27, middle - height * 0.09),
                button_radius,
                keys::A,
                "A",
            ),
            (
                pos2(right - side * 0.73, middle + height * 0.09),
                button_radius,
                keys::B,
                "B",
            ),
        ],
        pills: [
            (top_left, keys::L, "L"),
            (top_right, keys::R, "R"),
            (bottom_left, keys::SELECT, "Select"),
            (bottom_right, keys::START, "Start"),
        ],
    }
}

fn dpad_mask(center: Pos2, radius: f32, touch: Pos2) -> u32 {
    let offset = touch - center;
    let distance = offset.length();
    if distance > radius * HIT_MARGIN || distance < radius * DPAD_DEAD_ZONE {
        return 0;
    }
    let (x, y) = (offset.x / distance, offset.y / distance);
    let mut mask = 0;
    if x > DPAD_DIAGONAL {
        mask |= keys::RIGHT;
    } else if x < -DPAD_DIAGONAL {
        mask |= keys::LEFT;
    }
    if y > DPAD_DIAGONAL {
        mask |= keys::DOWN;
    } else if y < -DPAD_DIAGONAL {
        mask |= keys::UP;
    }
    mask
}

fn touch_mask(layout: &Layout, touch: Pos2) -> u32 {
    let mut mask = dpad_mask(layout.dpad.0, layout.dpad.1, touch);
    for (center, radius, button, _) in layout.round {
        if touch.distance(center) <= radius * HIT_MARGIN {
            mask |= button;
        }
    }
    for (rect, button, _) in layout.pills {
        if rect
            .expand(rect.height() * (HIT_MARGIN - 1.0))
            .contains(touch)
        {
            mask |= button;
        }
    }
    mask
}

/// The fingers on the screen, and the area the controls were last drawn in.
#[derive(Default)]
pub struct TouchControls {
    touches: HashMap<u64, Pos2>,
    area: Option<Rect>,
}

impl TouchControls {
    /// Follows this paint's finger movements.
    pub fn update(&mut self, ctx: &egui::Context) {
        ctx.input(|input| {
            for event in &input.events {
                if let egui::Event::Touch { id, phase, pos, .. } = event {
                    match phase {
                        TouchPhase::Start | TouchPhase::Move => {
                            self.touches.insert(id.0, *pos);
                        }
                        TouchPhase::End | TouchPhase::Cancel => {
                            self.touches.remove(&id.0);
                        }
                    }
                }
            }
        });
    }

    /// The GBA buttons under the fingers.
    pub fn pressed(&self) -> u32 {
        let Some(area) = self.area else {
            return 0;
        };
        let layout = layout(area);
        self.touches
            .values()
            .fold(0, |mask, touch| mask | touch_mask(&layout, *touch))
    }

    /// Where a finger would press a button, for tests.
    pub fn position_of(&self, button: u32) -> Option<Pos2> {
        let layout = layout(self.area?);
        let round = layout.round.iter().find(|r| r.2 == button).map(|r| r.0);
        let pill = layout
            .pills
            .iter()
            .find(|p| p.1 == button)
            .map(|p| p.0.center());
        round.or(pill)
    }

    pub fn draw(&mut self, painter: &egui::Painter, area: Rect) {
        self.area = Some(area);
        let layout = layout(area);
        let stroke = Stroke::new(1.5, OUTLINE);
        let held = self.pressed();
        let fill = |active: bool| if active { OUTLINE } else { FILL };

        let (center, radius) = layout.dpad;
        painter.circle(center, radius, FILL, stroke);
        let directions = [
            (keys::UP, vec2(0.0, -1.0), "^"),
            (keys::DOWN, vec2(0.0, 1.0), "v"),
            (keys::LEFT, vec2(-1.0, 0.0), "<"),
            (keys::RIGHT, vec2(1.0, 0.0), ">"),
        ];
        for (button, direction, label) in directions {
            let position = center + direction * radius * 0.62;
            painter.circle_filled(position, radius * 0.28, fill(held & button != 0));
            draw_label(painter, position, label, radius * 0.3);
        }

        for (center, radius, button, label) in layout.round {
            painter.circle(center, radius, fill(held & button != 0), stroke);
            draw_label(painter, center, label, radius * 0.7);
        }
        for (rect, button, label) in layout.pills {
            let corners = rect.height() / 2.0;
            painter.rect(
                rect,
                corners,
                fill(held & button != 0),
                stroke,
                egui::StrokeKind::Middle,
            );
            draw_label(painter, rect.center(), label, rect.height() * 0.45);
        }
    }
}

fn draw_label(painter: &egui::Painter, position: Pos2, text: &str, size: f32) {
    painter.text(
        position,
        Align2::CENTER_CENTER,
        text,
        FontId::proportional(size),
        LABEL,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The console's area on a typical phone held sideways, in points.
    fn phone() -> Rect {
        Rect::from_min_size(pos2(0.0, 40.0), vec2(794.0, 321.0))
    }

    #[test]
    fn every_button_is_inside_the_area_and_beside_the_game() {
        let area = phone();
        let layout = layout(area);
        let game_width = area.height() * SCREEN_ASPECT;
        let game = Rect::from_center_size(area.center(), vec2(game_width, area.height()));

        let (center, radius) = layout.dpad;
        assert!(area.contains(center - vec2(radius, radius)));
        assert!(center.x + radius <= game.left());
        for (center, radius, _, _) in layout.round {
            assert!(area.contains(center + vec2(radius, radius)));
            assert!(center.x - radius >= game.right());
        }
        for (rect, _, _) in layout.pills {
            assert!(area.contains_rect(rect));
            assert!(!rect.intersects(game));
        }
    }

    #[test]
    fn no_two_controls_overlap() {
        let layout = layout(phone());
        let [(a, a_radius, ..), (b, b_radius, ..)] = layout.round;
        assert!(a.distance(b) > (a_radius + b_radius) * HIT_MARGIN);
        for (rect, button, _) in layout.pills {
            assert_eq!(touch_mask(&layout, rect.center()), button);
        }
        assert_eq!(touch_mask(&layout, a), keys::A);
        assert_eq!(touch_mask(&layout, b), keys::B);
    }

    #[test]
    fn the_d_pad_gives_directions_and_diagonals() {
        let center = pos2(100.0, 100.0);
        let at = |x, y| dpad_mask(center, 50.0, center + vec2(x, y));
        assert_eq!(at(0.0, -40.0), keys::UP);
        assert_eq!(at(0.0, 40.0), keys::DOWN);
        assert_eq!(at(-40.0, 0.0), keys::LEFT);
        assert_eq!(at(40.0, 0.0), keys::RIGHT);
        assert_eq!(at(30.0, -30.0), keys::RIGHT | keys::UP);
        assert_eq!(at(2.0, 2.0), 0);
        assert_eq!(at(200.0, 0.0), 0);
    }

    #[test]
    fn empty_space_presses_nothing() {
        let area = phone();
        assert_eq!(touch_mask(&layout(area), area.center()), 0);
    }

    #[test]
    fn several_fingers_combine() {
        let area = phone();
        let shape = layout(area);
        let mut controls = TouchControls {
            area: Some(area),
            ..Default::default()
        };
        controls.touches.insert(1, shape.round[0].0);
        controls
            .touches
            .insert(2, shape.dpad.0 + vec2(shape.dpad.1 * 0.8, 0.0));
        assert_eq!(controls.pressed(), keys::A | keys::RIGHT);

        controls.touches.remove(&1);
        assert_eq!(controls.pressed(), keys::RIGHT);
    }
}
