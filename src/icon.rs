//! Draw the tray icon: a small dark badge with the battery percentage and a
//! level bar. It is rendered to RGBA and works on both light and dark panels.

use crate::types::ChargeState;

pub const SIZE: u32 = 32;

const GREEN: [u8; 4] = [76, 175, 80, 255];
const AMBER: [u8; 4] = [255, 179, 0, 255];
const RED: [u8; 4] = [244, 67, 54, 255];
const WHITE: [u8; 4] = [245, 246, 248, 255];
const GRAY: [u8; 4] = [165, 168, 174, 255];
const DARK: [u8; 4] = [28, 30, 34, 255];
const BAR_BG: [u8; 4] = [62, 64, 70, 255];
const BORDER: [u8; 4] = [70, 72, 78, 255];
const CHARGING: [u8; 4] = [255, 214, 79, 255];

pub fn render_rgba(
    percent: Option<u8>,
    charge: ChargeState,
    connected: bool,
    stale: bool,
) -> Vec<u8> {
    let mut canvas = vec![0u8; (SIZE * SIZE * 4) as usize];

    let badge = Rect {
        x: 1,
        y: 1,
        w: 30,
        h: 30,
        r: 7,
    };
    fill_rounded(&mut canvas, badge, DARK);

    let border = if !connected || stale {
        GRAY
    } else if charge.is_charging() {
        CHARGING
    } else if percent.map(|p| p < 20).unwrap_or(false) {
        RED
    } else {
        BORDER
    };
    stroke_rounded(&mut canvas, badge, 2, border);

    let text = match percent {
        Some(value) => value.min(100).to_string(),
        None => "--".to_string(),
    };
    let text_color = if !connected || stale || percent.is_none() {
        GRAY
    } else {
        WHITE
    };

    let charging = charge.is_charging() && connected && !stale;
    let scale = if text.len() >= 3 {
        2
    } else if charging {
        3
    } else {
        4
    };
    let gap = if scale >= 3 { 3 } else { 2 };
    let text_width = text.len() as i32 * 3 * scale + (text.len() as i32 - 1) * gap;
    let text_x = if charging {
        3
    } else {
        (SIZE as i32 - text_width) / 2
    };
    let text_y = match (text.len(), charging) {
        (3, _) => 8,
        (_, true) => 9,
        _ => 4,
    };
    draw_text(&mut canvas, &text, text_x, text_y, scale, gap, text_color);

    if charging {
        draw_bolt(&mut canvas, 25, 3);
    }

    // Level bar at the bottom of the badge.
    fill_rect(&mut canvas, 4, 27, 24, 3, BAR_BG);
    if let Some(value) = percent {
        let width = ((value.min(100) as i32 * 24 + 50) / 100).max(if value > 0 { 1 } else { 0 });
        let color = if stale || !connected {
            GRAY
        } else if value >= 50 {
            GREEN
        } else if value >= 20 {
            AMBER
        } else {
            RED
        };
        fill_rect(&mut canvas, 4, 27, width, 3, color);
    }

    canvas
}

#[derive(Clone, Copy)]
struct Rect {
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    r: i32,
}

fn fill_rounded(canvas: &mut [u8], rect: Rect, color: [u8; 4]) {
    for py in rect.y..rect.y + rect.h {
        for px in rect.x..rect.x + rect.w {
            if inside_rounded(px, py, rect) {
                put(canvas, px, py, color);
            }
        }
    }
}

fn stroke_rounded(canvas: &mut [u8], rect: Rect, thickness: i32, color: [u8; 4]) {
    let inner = Rect {
        x: rect.x + thickness,
        y: rect.y + thickness,
        w: rect.w - 2 * thickness,
        h: rect.h - 2 * thickness,
        r: (rect.r - thickness).max(1),
    };
    for py in rect.y..rect.y + rect.h {
        for px in rect.x..rect.x + rect.w {
            if inside_rounded(px, py, rect) && !inside_rounded(px, py, inner) {
                put(canvas, px, py, color);
            }
        }
    }
}

fn inside_rounded(px: i32, py: i32, rect: Rect) -> bool {
    if px < rect.x || py < rect.y || px >= rect.x + rect.w || py >= rect.y + rect.h {
        return false;
    }
    let radius = rect.r.min(rect.w / 2).min(rect.h / 2).max(0);
    if radius == 0 {
        return true;
    }
    let cx = if px < rect.x + radius {
        rect.x + radius
    } else if px >= rect.x + rect.w - radius {
        rect.x + rect.w - radius - 1
    } else {
        px
    };
    let cy = if py < rect.y + radius {
        rect.y + radius
    } else if py >= rect.y + rect.h - radius {
        rect.y + rect.h - radius - 1
    } else {
        py
    };
    let dx = px - cx;
    let dy = py - cy;
    dx * dx + dy * dy <= radius * radius
}

fn fill_rect(canvas: &mut [u8], x: i32, y: i32, width: i32, height: i32, color: [u8; 4]) {
    for py in y..y + height {
        for px in x..x + width {
            put(canvas, px, py, color);
        }
    }
}

fn put(canvas: &mut [u8], x: i32, y: i32, color: [u8; 4]) {
    if x < 0 || y < 0 || x >= SIZE as i32 || y >= SIZE as i32 {
        return;
    }
    let index = ((y * SIZE as i32 + x) * 4) as usize;
    canvas[index..index + 4].copy_from_slice(&color);
}

/// 3x5 bitmap digits. One bit per column, most significant bit first.
const FONT: [[u8; 5]; 10] = [
    [0b111, 0b101, 0b101, 0b101, 0b111], // 0
    [0b010, 0b110, 0b010, 0b010, 0b111], // 1
    [0b111, 0b001, 0b111, 0b100, 0b111], // 2
    [0b111, 0b001, 0b111, 0b001, 0b111], // 3
    [0b101, 0b101, 0b111, 0b001, 0b001], // 4
    [0b111, 0b100, 0b111, 0b001, 0b111], // 5
    [0b111, 0b100, 0b111, 0b101, 0b111], // 6
    [0b111, 0b001, 0b001, 0b010, 0b010], // 7
    [0b111, 0b101, 0b111, 0b101, 0b111], // 8
    [0b111, 0b101, 0b111, 0b001, 0b111], // 9
];

fn draw_text(canvas: &mut [u8], text: &str, x: i32, y: i32, scale: i32, gap: i32, color: [u8; 4]) {
    let advance = 3 * scale + gap;
    for (index, character) in text.chars().enumerate() {
        let origin_x = x + index as i32 * advance;
        if character == '-' {
            for px in 0..3 * scale {
                for py in 0..scale {
                    put(canvas, origin_x + px, y + 2 * scale + py, color);
                }
            }
            continue;
        }
        let Some(digit) = character.to_digit(10) else {
            continue;
        };
        let glyph = FONT[digit as usize];
        for (row, bits) in glyph.iter().enumerate() {
            for column in 0..3 {
                if bits & (1 << (2 - column)) == 0 {
                    continue;
                }
                for dy in 0..scale {
                    for dx in 0..scale {
                        put(
                            canvas,
                            origin_x + column * scale + dx,
                            y + row as i32 * scale + dy,
                            color,
                        );
                    }
                }
            }
        }
    }
}

const BOLT: [[u8; 5]; 7] = [
    [0, 0, 1, 0, 0],
    [0, 1, 1, 0, 0],
    [1, 1, 1, 1, 0],
    [0, 0, 1, 1, 0],
    [0, 1, 1, 0, 0],
    [0, 1, 0, 0, 0],
    [0, 1, 0, 0, 0],
];

fn draw_bolt(canvas: &mut [u8], x: i32, y: i32) {
    for (row, bits) in BOLT.iter().enumerate() {
        for (column, bit) in bits.iter().enumerate() {
            if *bit != 0 {
                put(canvas, x + column as i32, y + row as i32, CHARGING);
            }
        }
    }
}
