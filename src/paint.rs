//! The drawing surface: pixel buffer, anti-aliased geometry, text and history.

use std::sync::OnceLock;

use ab_glyph::{Font, FontVec, ScaleFont};
use eframe::egui::{Color32, ColorImage, Pos2, Vec2};
use image::ImageBuffer;

pub const CANVAS_W: usize = 1400;
pub const CANVAS_H: usize = 900;

const MAX_HISTORY: usize = 64;
const FONT_BYTES: &[u8] = include_bytes!("../assets/Ubuntu-Light.ttf");

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tool {
    Select,
    Brush,
    Line,
    Arrow,
    Rect,
    Ellipse,
    Angle,
    Measure,
    Text,
    Eraser,
    Picker,
}

/// Shared geometry for the live angle preview and the rasterized angle mark.
///
/// Keeping the arc, rays, label position and displayed degrees in one place
/// prevents the on-screen preview from disagreeing with the committed mark.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AngleGeometry {
    pub first_angle: f32,
    pub second_angle: f32,
    pub start: f32,
    pub sweep: f32,
    pub degrees: f32,
    pub first_ray: f32,
    pub second_ray: f32,
    pub radius: f32,
}

impl AngleGeometry {
    pub fn mid_angle(self) -> f32 {
        self.start + self.sweep * 0.5
    }

    pub fn label(self) -> String {
        format!("{:.1}°", self.degrees)
    }
}

/// Derive the smaller angle between two rays from a common vertex.
///
/// Returns `None` when either ray is too short to define a direction; callers
/// then keep only the first-ray guide instead of drawing a bogus zero ray or
/// an arc with a `NaN` center.
pub fn angle_geometry(vertex: Pos2, first: Pos2, second: Pos2) -> Option<AngleGeometry> {
    let first_vec = first - vertex;
    let second_vec = second - vertex;
    if first_vec.length_sq() < 9.0 || second_vec.length_sq() < 9.0 {
        return None;
    }

    let first_angle = first_vec.angle();
    let second_angle = second_vec.angle();
    let mut sweep = (second_angle - first_angle).rem_euclid(std::f32::consts::TAU);
    let mut start = first_angle;
    if sweep > std::f32::consts::PI {
        sweep = std::f32::consts::TAU - sweep;
        start = second_angle;
    }

    // Extend very short rays so the arc and degree label always fit, but keep
    // long diagram rays from covering the paper.
    let first_ray = first_vec.length().clamp(28.0, 84.0);
    let second_ray = second_vec.length().clamp(28.0, 84.0);
    let radius = (first_ray.min(second_ray) * 0.5).clamp(14.0, 42.0);

    Some(AngleGeometry {
        first_angle,
        second_angle,
        start,
        sweep,
        degrees: sweep.to_degrees(),
        first_ray,
        second_ray,
        radius,
    })
}

impl Tool {
    pub fn label(self) -> &'static str {
        match self {
            Tool::Select => "Select",
            Tool::Brush => "Freehand",
            Tool::Line => "Line",
            Tool::Arrow => "Arrow / vector",
            Tool::Rect => "Rectangle",
            Tool::Ellipse => "Circle / ellipse",
            Tool::Angle => "Angle",
            Tool::Measure => "Measure",
            Tool::Text => "Text",
            Tool::Eraser => "Eraser",
            Tool::Picker => "Pick color",
        }
    }

    /// Tools that are committed when the pointer is released.
    pub fn is_shape(self) -> bool {
        matches!(
            self,
            Tool::Line | Tool::Arrow | Tool::Rect | Tool::Ellipse | Tool::Angle | Tool::Measure
        )
    }

    /// Whether the tool needs two drags instead of one.
    pub fn is_two_step(self) -> bool {
        self == Tool::Angle
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RectPx {
    pub x: usize,
    pub y: usize,
    pub w: usize,
    pub h: usize,
}

impl RectPx {
    fn union(self, other: Self) -> Self {
        let x = self.x.min(other.x);
        let y = self.y.min(other.y);
        let right = (self.x + self.w).max(other.x + other.w);
        let bottom = (self.y + self.h).max(other.y + other.h);
        Self {
            x,
            y,
            w: right - x,
            h: bottom - y,
        }
    }
}

/// A saved rectangular region used for undo/redo.
struct Patch {
    rect: RectPx,
    pixels: Vec<Color32>,
}

/// Paper guide style, shared by the on-screen paper and PNG export.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PaperTemplate {
    Plain,
    Square,
    Dot,
    Polar,
    Log,
}

impl PaperTemplate {
    pub const ALL: [Self; 5] = [Self::Plain, Self::Square, Self::Dot, Self::Polar, Self::Log];

    pub fn label(self) -> &'static str {
        match self {
            Self::Plain => "Plain",
            Self::Square => "Square",
            Self::Dot => "Dot",
            Self::Polar => "Polar",
            Self::Log => "Log",
        }
    }
}

pub struct Painting {
    /// Authoritative pixel data. Transparent pixels show the paper underneath.
    pub image: ColorImage,
    before: Option<ColorImage>,
    dirty: Option<RectPx>,
    upload: Option<RectPx>,
    undo: Vec<Patch>,
    redo: Vec<Patch>,
    /// Incremented on every committed change; used by autosave.
    pub revision: u64,
}

impl Default for Painting {
    fn default() -> Self {
        Self::new()
    }
}

impl Painting {
    pub fn new() -> Self {
        Self {
            image: ColorImage::new([CANVAS_W, CANVAS_H], Color32::TRANSPARENT),
            before: None,
            dirty: None,
            upload: None,
            undo: Vec::new(),
            redo: Vec::new(),
            revision: 0,
        }
    }

    pub fn begin_stroke(&mut self) {
        self.before = Some(self.image.clone());
        self.dirty = None;
    }

    pub fn end_stroke(&mut self) {
        let Some(rect) = self.dirty.take() else {
            self.before = None;
            return;
        };
        if let Some(before) = self.before.take() {
            self.push_undo(Patch {
                rect,
                pixels: extract(&before, rect),
            });
            self.revision = self.revision.wrapping_add(1);
        }
    }

    pub fn cancel_stroke(&mut self) {
        if let Some(before) = self.before.take() {
            let full = full_rect();
            self.image = before;
            self.mark_upload(full);
        }
        self.dirty = None;
        self.revision = self.revision.wrapping_add(1);
    }

    pub fn take_upload(&mut self) -> Option<RectPx> {
        self.upload.take()
    }

    fn mark_upload(&mut self, rect: RectPx) {
        self.upload = Some(match self.upload.take() {
            Some(old) => old.union(rect),
            None => rect,
        });
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn undo(&mut self) -> bool {
        let Some(patch) = self.undo.pop() else {
            return false;
        };
        let current = extract(&self.image, patch.rect);
        apply(&mut self.image, &patch);
        self.redo.push(Patch {
            rect: patch.rect,
            pixels: current,
        });
        self.mark_upload(patch.rect);
        self.revision = self.revision.wrapping_add(1);
        true
    }

    pub fn redo(&mut self) -> bool {
        let Some(patch) = self.redo.pop() else {
            return false;
        };
        let current = extract(&self.image, patch.rect);
        apply(&mut self.image, &patch);
        self.undo.push(Patch {
            rect: patch.rect,
            pixels: current,
        });
        self.mark_upload(patch.rect);
        self.revision = self.revision.wrapping_add(1);
        true
    }

    pub fn clear(&mut self) {
        let rect = full_rect();
        let pixels = extract(&self.image, rect);
        self.push_undo(Patch { rect, pixels });
        for pixel in &mut self.image.pixels {
            *pixel = Color32::TRANSPARENT;
        }
        self.mark_upload(rect);
        self.revision = self.revision.wrapping_add(1);
    }

    /// Read the color at a canvas position (ignores fully transparent pixels).
    pub fn sample(&self, x: f32, y: f32) -> Option<Color32> {
        let px = x as usize;
        let py = y as usize;
        if px >= CANVAS_W || py >= CANVAS_H {
            return None;
        }
        let c = self.image.pixels[py * CANVAS_W + px];
        (c.a() > 0).then_some(c)
    }

    /// Copy a canvas rectangle into a small image.
    pub fn region(&self, rect: RectPx) -> ColorImage {
        let rect = self.clip(rect);
        let w = rect.w.max(1);
        let h = rect.h.max(1);
        let mut out = ColorImage::new([w, h], Color32::TRANSPARENT);
        if rect.w == 0 || rect.h == 0 {
            return out;
        }
        for y in 0..rect.h {
            for x in 0..rect.w {
                out.pixels[y * w + x] = self.image.pixels[(rect.y + y) * CANVAS_W + rect.x + x];
            }
        }
        out
    }

    /// Erase a canvas rectangle, used when cutting or moving a selection.
    pub fn clear_region(&mut self, rect: RectPx) {
        let rect = self.clip(rect);
        if rect.w == 0 || rect.h == 0 {
            return;
        }
        for y in rect.y..rect.y + rect.h {
            let row = y * CANVAS_W;
            for pixel in &mut self.image.pixels[row + rect.x..row + rect.x + rect.w] {
                *pixel = Color32::TRANSPARENT;
            }
        }
        self.mark_dirty(rect);
        self.mark_upload(rect);
        self.revision = self.revision.wrapping_add(1);
    }

    /// Composite an image onto the canvas, clipping at the edges.
    pub fn paste_region(&mut self, patch: &ColorImage, x: f32, y: f32) {
        let x0 = x.round() as isize;
        let y0 = y.round() as isize;
        let max_x = (x0 + patch.width() as isize).min(CANVAS_W as isize);
        let max_y = (y0 + patch.height() as isize).min(CANVAS_H as isize);
        let x0 = x0.max(0);
        let y0 = y0.max(0);
        if x0 >= max_x || y0 >= max_y {
            return;
        }
        for py in y0..max_y {
            for px in x0..max_x {
                let src = patch.pixels[(py - y0) as usize * patch.width() + (px - x0) as usize];
                if src.a() == 0 {
                    continue;
                }
                let idx = py as usize * CANVAS_W + px as usize;
                self.image.pixels[idx] = blend_over(self.image.pixels[idx], src, 1.0);
            }
        }
        let rect = RectPx {
            x: x0 as usize,
            y: y0 as usize,
            w: (max_x - x0) as usize,
            h: (max_y - y0) as usize,
        };
        self.mark_dirty(rect);
        self.mark_upload(rect);
        self.revision = self.revision.wrapping_add(1);
    }

    fn clip(&self, rect: RectPx) -> RectPx {
        let x0 = rect.x.min(CANVAS_W);
        let y0 = rect.y.min(CANVAS_H);
        let x1 = rect.x.saturating_add(rect.w).min(CANVAS_W);
        let y1 = rect.y.saturating_add(rect.h).min(CANVAS_H);
        RectPx {
            x: x0,
            y: y0,
            w: x1.saturating_sub(x0),
            h: y1.saturating_sub(y0),
        }
    }

    /// Stamp an anti-aliased circular brush at a canvas position.
    pub fn stamp(
        &mut self,
        x: f32,
        y: f32,
        radius: f32,
        color: Color32,
        opacity: f32,
        erase: bool,
    ) {
        let r = radius.max(0.5);
        let min_x = ((x - r - 1.0).floor() as isize).max(0) as usize;
        let min_y = ((y - r - 1.0).floor() as isize).max(0) as usize;
        let max_x = ((x + r + 1.0).ceil() as isize).min(CANVAS_W as isize - 1) as usize;
        let max_y = ((y + r + 1.0).ceil() as isize).min(CANVAS_H as isize - 1) as usize;

        for py in min_y..=max_y {
            for px in min_x..=max_x {
                let dx = px as f32 + 0.5 - x;
                let dy = py as f32 + 0.5 - y;
                let dist = (dx * dx + dy * dy).sqrt();
                let coverage = (r + 0.5 - dist).clamp(0.0, 1.0);
                if coverage <= 0.0 {
                    continue;
                }
                let alpha = (coverage * opacity).clamp(0.0, 1.0);
                let idx = py * CANVAS_W + px;
                let dst = self.image.pixels[idx];
                self.image.pixels[idx] = if erase {
                    blend_erase(dst, alpha)
                } else {
                    blend_over(dst, color, alpha)
                };
            }
        }

        let rect = RectPx {
            x: min_x,
            y: min_y,
            w: max_x - min_x + 1,
            h: max_y - min_y + 1,
        };
        self.mark_dirty(rect);
        self.mark_upload(rect);
    }

    fn mark_dirty(&mut self, rect: RectPx) {
        self.dirty = Some(match self.dirty.take() {
            Some(old) => old.union(rect),
            None => rect,
        });
    }

    /// Draw a straight stroke of the given full width.
    pub fn segment(
        &mut self,
        from: Pos2,
        to: Pos2,
        width: f32,
        color: Color32,
        opacity: f32,
        erase: bool,
    ) {
        let len = (to - from).length();
        let step = (width * 0.25).clamp(0.75, 3.0);
        let steps = (len / step).ceil().max(1.0) as usize;
        for i in 0..=steps {
            let t = i as f32 / steps as f32;
            let p = from.lerp(to, t);
            self.stamp(p.x, p.y, width * 0.5, color, opacity, erase);
        }
    }

    pub fn polyline(
        &mut self,
        points: &[Pos2],
        width: f32,
        color: Color32,
        opacity: f32,
        erase: bool,
    ) {
        for pair in points.windows(2) {
            self.segment(pair[0], pair[1], width, color, opacity, erase);
        }
    }

    pub fn ellipse(
        &mut self,
        center: Pos2,
        radii: Vec2,
        width: f32,
        color: Color32,
        opacity: f32,
        erase: bool,
    ) {
        let radius = radii.x.max(radii.y).max(1.0);
        let steps = (2.0 * std::f32::consts::PI * radius / (width * 0.35).clamp(0.8, 3.0))
            .clamp(48.0, 1200.0) as usize;
        let points: Vec<Pos2> = (0..=steps)
            .map(|i| {
                let a = i as f32 / steps as f32 * std::f32::consts::TAU;
                Pos2::new(center.x + radii.x * a.cos(), center.y + radii.y * a.sin())
            })
            .collect();
        self.polyline(&points, width, color, opacity, erase);
    }

    pub fn arc(
        &mut self,
        center: Pos2,
        radius: f32,
        start: f32,
        sweep: f32,
        width: f32,
        color: Color32,
        opacity: f32,
        erase: bool,
    ) {
        let steps =
            (radius * sweep.abs() / (width * 0.35).clamp(0.8, 3.0)).clamp(12.0, 600.0) as usize;
        let points: Vec<Pos2> = (0..=steps)
            .map(|i| {
                let a = start + sweep * (i as f32 / steps as f32);
                Pos2::new(center.x + radius * a.cos(), center.y + radius * a.sin())
            })
            .collect();
        self.polyline(&points, width, color, opacity, erase);
    }

    fn arrow_head(&mut self, tip: Pos2, direction: Vec2, width: f32, color: Color32, opacity: f32) {
        let head = (width * 3.6).clamp(9.0, 28.0);
        let normal = Vec2::new(-direction.y, direction.x);
        for side in [-1.0, 1.0] {
            let back = tip - direction * head + normal * (side * head * 0.42);
            self.segment(back, tip, width, color, opacity, false);
        }
    }

    pub fn arrow(&mut self, from: Pos2, to: Pos2, width: f32, color: Color32, opacity: f32) {
        self.segment(from, to, width, color, opacity, false);
        if (to - from).length() > 4.0 {
            self.arrow_head(to, (to - from).normalized(), width, color, opacity);
        }
    }

    pub fn double_arrow(&mut self, from: Pos2, to: Pos2, width: f32, color: Color32, opacity: f32) {
        self.arrow(from, to, width, color, opacity);
        if (to - from).length() > 4.0 {
            self.arrow_head(from, (from - to).normalized(), width, color, opacity);
        }
    }

    /// A dimension line with ticks and a centred label (used by the measure tool).
    pub fn dimension(
        &mut self,
        from: Pos2,
        to: Pos2,
        label: &str,
        width: f32,
        size: f32,
        color: Color32,
    ) {
        let thin = width.max(1.4);
        let dir = to - from;
        if dir.length() < 6.0 {
            return;
        }
        let dir = dir.normalized();
        let normal = Vec2::new(-dir.y, dir.x);
        self.double_arrow(from, to, thin, color, 0.95);
        for end in [from, to] {
            self.segment(
                end - normal * 7.0,
                end + normal * 7.0,
                thin,
                color,
                0.95,
                false,
            );
        }
        let mid = from.lerp(to, 0.5) + normal * 13.0;
        let tw = self.text_width(label, size);
        self.text(mid.x - tw * 0.5, mid.y, label, size, color);
    }

    /// Two rays, an arc and a degree label (used by the angle tool).
    pub fn angle_mark(
        &mut self,
        vertex: Pos2,
        first: Pos2,
        second: Pos2,
        width: f32,
        size: f32,
        color: Color32,
        opacity: f32,
    ) {
        let Some(geometry) = angle_geometry(vertex, first, second) else {
            return;
        };
        let thin = width.max(1.4);
        let opacity = opacity.clamp(0.0, 1.0);
        self.segment(
            vertex,
            vertex + Vec2::angled(geometry.first_angle) * geometry.first_ray,
            thin,
            color,
            opacity,
            false,
        );
        self.segment(
            vertex,
            vertex + Vec2::angled(geometry.second_angle) * geometry.second_ray,
            thin,
            color,
            opacity,
            false,
        );
        self.arc(
            vertex,
            geometry.radius,
            geometry.start,
            geometry.sweep,
            thin,
            color,
            opacity,
            false,
        );
        let label = geometry.label();
        let label_pos = vertex + Vec2::angled(geometry.mid_angle()) * (geometry.radius + 12.0);
        let tw = self.text_width(&label, size);
        let [r, g, b, a] = eframe::egui::Rgba::from(color).to_srgba_unmultiplied();
        let label_color =
            Color32::from_rgba_unmultiplied(r, g, b, ((a as f32) * opacity).round() as u8);
        self.text(
            label_pos.x - tw * 0.5,
            label_pos.y,
            &label,
            size,
            label_color,
        );
    }

    /// Rasterize a UTF-8 string with the bundled Ubuntu font into the canvas.
    pub fn text(&mut self, x: f32, y: f32, text: &str, size: f32, color: Color32) {
        let font = font();
        let scaled = font.as_scaled(size.max(6.0));
        let line_height = scaled.height() * 1.18;
        let mut cursor_x = x;
        let mut cursor_y = y;
        let mut previous = None;

        for ch in text.chars() {
            if ch == '\n' {
                cursor_x = x;
                cursor_y += line_height;
                previous = None;
                continue;
            }
            let id = scaled.glyph_id(ch);
            if let Some(prev) = previous {
                cursor_x += scaled.kern(prev, id);
            }
            let mut glyph = scaled.scaled_glyph(ch);
            glyph.position = ab_glyph::point(cursor_x, cursor_y + scaled.ascent());
            if let Some(outlined) = font.outline_glyph(glyph) {
                let bounds = outlined.px_bounds();
                // `OutlinedGlyph::draw` yields coordinates RELATIVE to
                // `px_bounds()`. Without adding the bounds origin every glyph
                // lands at the canvas origin (0,0), so text appeared to
                // "vanish" when placed while the on-screen preview was drawn
                // at the requested position.
                let bx = bounds.min.x as isize;
                let by = bounds.min.y as isize;
                outlined.draw(|gx, gy, coverage| {
                    let px = bx + gx as isize;
                    let py = by + gy as isize;
                    if px < 0
                        || py < 0
                        || px >= CANVAS_W as isize
                        || py >= CANVAS_H as isize
                        || coverage <= 0.0
                    {
                        return;
                    }
                    let idx = py as usize * CANVAS_W + px as usize;
                    let dst = self.image.pixels[idx];
                    self.image.pixels[idx] = blend_over(dst, color, coverage);
                });
                let rect = RectPx {
                    x: (bounds.min.x.floor() as isize).max(0) as usize,
                    y: (bounds.min.y.floor() as isize).max(0) as usize,
                    w: (bounds.max.x.ceil() as isize).max(0) as usize,
                    h: (bounds.max.y.ceil() as isize).max(0) as usize,
                };
                let rect = RectPx {
                    x: rect.x.min(CANVAS_W - 1),
                    y: rect.y.min(CANVAS_H - 1),
                    w: (rect.w.saturating_sub(rect.x)).min(CANVAS_W - rect.x),
                    h: (rect.h.saturating_sub(rect.y)).min(CANVAS_H - rect.y),
                };
                self.mark_dirty(rect);
                self.mark_upload(rect);
            }
            cursor_x += scaled.h_advance(id);
            previous = Some(id);
        }
    }

    pub fn text_width(&self, text: &str, size: f32) -> f32 {
        let font = font();
        let scaled = font.as_scaled(size.max(6.0));
        let mut width = 0.0;
        let mut previous = None;
        for ch in text.chars() {
            if ch == '\n' {
                break;
            }
            let id = scaled.glyph_id(ch);
            if let Some(prev) = previous {
                width += scaled.kern(prev, id);
            }
            width += scaled.h_advance(id);
            previous = Some(id);
        }
        width
    }

    /// Draw a divided formula triangle as one undoable stroke: a solid outline,
    /// a solid horizontal divider under the top label and a solid vertical
    /// divider between the two bottom labels.
    pub fn formula_triangle(
        &mut self,
        center: Pos2,
        width: f32,
        height: f32,
        top: &str,
        left: &str,
        right: &str,
        color: Color32,
    ) {
        let apex_y = center.y - height * 0.5;
        let base_y = center.y + height * 0.5;
        let divider_y = center.y - height * 0.08;
        let t = (divider_y - apex_y) / (base_y - apex_y);
        let divider_half = width * 0.5 * t;

        let apex = Pos2::new(center.x, apex_y);
        let base_l = Pos2::new(center.x - width * 0.5, base_y);
        let base_r = Pos2::new(center.x + width * 0.5, base_y);
        let divider_l = Pos2::new(center.x - divider_half, divider_y);
        let divider_r = Pos2::new(center.x + divider_half, divider_y);
        let foot = Pos2::new(center.x, base_y);

        // Letters follow the triangle, but stay in a readable band.
        let mut size = (height * 0.17).clamp(18.0, 58.0);
        let limit = width * 0.24;
        while size > 14.0 {
            let widest = self
                .text_width(top, size)
                .max(self.text_width(left, size))
                .max(self.text_width(right, size));
            if widest <= limit {
                break;
            }
            size -= 1.0;
        }

        self.begin_stroke();
        let stroke = 3.4_f32;
        self.polyline(&[apex, base_l, base_r, apex], stroke, color, 1.0, false);
        self.polyline(&[divider_l, divider_r], stroke, color, 1.0, false);
        self.polyline(
            &[foot, Pos2::new(center.x, divider_y)],
            stroke,
            color,
            1.0,
            false,
        );

        let top_y = apex_y + (divider_y - apex_y) * 0.5 - size * 0.55;
        let bottom_y = divider_y + (base_y - divider_y) * 0.5 - size * 0.55;
        self.text(
            center.x - self.text_width(top, size) * 0.5,
            top_y,
            top,
            size,
            color,
        );
        self.text(
            center.x - width * 0.22 - self.text_width(left, size) * 0.5,
            bottom_y,
            left,
            size,
            color,
        );
        self.text(
            center.x + width * 0.22 - self.text_width(right, size) * 0.5,
            bottom_y,
            right,
            size,
            color,
        );
        self.end_stroke();
    }

    /// Export the painting as PNG, optionally burning in the paper guides.
    pub fn save_png(&self, path: &std::path::Path, grid: bool, axes: bool) -> Result<(), String> {
        self.save_png_with(path, grid, axes, PaperTemplate::Square, 1, None)
    }

    /// Full-featured export: paper template, opaque background and upscaling.
    pub fn save_png_with(
        &self,
        path: &std::path::Path,
        grid: bool,
        axes: bool,
        template: PaperTemplate,
        scale: u32,
        paper_bg: Option<Color32>,
    ) -> Result<(), String> {
        let mut image = self.image.clone();
        if let Some(bg) = paper_bg {
            for pixel in &mut image.pixels {
                *pixel = blend_over(bg, *pixel, 1.0);
            }
        }
        if grid || axes {
            draw_guides(&mut image, grid, axes, template);
        }
        let scale = scale.clamp(1, 4).max(1);
        let w = CANVAS_W as u32 * scale;
        let h = CANVAS_H as u32 * scale;
        let mut buffer: ImageBuffer<image::Rgba<u8>, Vec<u8>> = ImageBuffer::new(w, h);
        for y in 0..h {
            let sy = (y / scale) as usize;
            for x in 0..w {
                let sx = (x / scale) as usize;
                let [r, g, b, a] = eframe::egui::Rgba::from(image.pixels[sy * CANVAS_W + sx])
                    .to_srgba_unmultiplied();
                buffer.put_pixel(x, y, image::Rgba([r, g, b, a]));
            }
        }
        buffer.save(path).map_err(|e| e.to_string())
    }

    fn push_undo(&mut self, patch: Patch) {
        self.undo.push(patch);
        if self.undo.len() > MAX_HISTORY {
            self.undo.remove(0);
        }
        self.redo.clear();
    }
}

fn font() -> &'static FontVec {
    static FONT: OnceLock<FontVec> = OnceLock::new();
    FONT.get_or_init(|| FontVec::try_from_vec(FONT_BYTES.to_vec()).expect("bundled font is valid"))
}

fn full_rect() -> RectPx {
    RectPx {
        x: 0,
        y: 0,
        w: CANVAS_W,
        h: CANVAS_H,
    }
}

fn extract(image: &ColorImage, rect: RectPx) -> Vec<Color32> {
    let mut pixels = Vec::with_capacity(rect.w * rect.h);
    for y in 0..rect.h {
        let row = (rect.y + y) * CANVAS_W + rect.x;
        pixels.extend_from_slice(&image.pixels[row..row + rect.w]);
    }
    pixels
}

fn apply(image: &mut ColorImage, patch: &Patch) {
    let r = patch.rect;
    for y in 0..r.h {
        let row = (r.y + y) * CANVAS_W + r.x;
        image.pixels[row..row + r.w].copy_from_slice(&patch.pixels[y * r.w..(y + 1) * r.w]);
    }
}

/// "Source over" compositing in egui's premultiplied linear color space.
fn blend_over(dst: Color32, src: Color32, alpha: f32) -> Color32 {
    if alpha <= 0.0 {
        return dst;
    }
    let s = eframe::egui::Rgba::from(src);
    let d = eframe::egui::Rgba::from(dst);
    let keep = 1.0 - alpha * s.a();
    eframe::egui::Rgba::from_rgba_premultiplied(
        s.r() * alpha + d.r() * keep,
        s.g() * alpha + d.g() * keep,
        s.b() * alpha + d.b() * keep,
        s.a() * alpha + d.a() * keep,
    )
    .into()
}

/// Fade an existing pixel toward transparent while preserving its hue.
fn blend_erase(dst: Color32, alpha: f32) -> Color32 {
    let keep = 1.0 - alpha;
    let d = eframe::egui::Rgba::from(dst);
    eframe::egui::Rgba::from_rgba_premultiplied(
        d.r() * keep,
        d.g() * keep,
        d.b() * keep,
        d.a() * keep,
    )
    .into()
}

fn draw_guides(image: &mut ColorImage, grid: bool, axes: bool, template: PaperTemplate) {
    const SPACING: usize = 40;
    let minor = Color32::from_rgba_unmultiplied(128, 128, 128, 28);
    let major = Color32::from_rgba_unmultiplied(128, 128, 128, 64);
    if grid {
        match template {
            PaperTemplate::Plain => {}
            PaperTemplate::Square => {
                for x in (0..CANVAS_W).step_by(SPACING) {
                    let color = if x % (SPACING * 5) == 0 { major } else { minor };
                    for y in 0..CANVAS_H {
                        let idx = y * CANVAS_W + x;
                        image.pixels[idx] = blend_over(image.pixels[idx], color, 1.0);
                    }
                }
                for y in (0..CANVAS_H).step_by(SPACING) {
                    let color = if y % (SPACING * 5) == 0 { major } else { minor };
                    for x in 0..CANVAS_W {
                        let idx = y * CANVAS_W + x;
                        image.pixels[idx] = blend_over(image.pixels[idx], color, 1.0);
                    }
                }
            }
            PaperTemplate::Dot => {
                for y in (0..CANVAS_H).step_by(SPACING) {
                    for x in (0..CANVAS_W).step_by(SPACING) {
                        let color = if x % (SPACING * 5) == 0 && y % (SPACING * 5) == 0 {
                            major
                        } else {
                            minor
                        };
                        for dy in 0..2 {
                            for dx in 0..2 {
                                let idx = (y + dy) * CANVAS_W + x + dx;
                                image.pixels[idx] = blend_over(image.pixels[idx], color, 1.0);
                            }
                        }
                    }
                }
            }
            PaperTemplate::Polar => {
                let cx = CANVAS_W as f32 * 0.5;
                let cy = CANVAS_H as f32 * 0.5;
                let max_r = cx.min(cy);
                let mut r = SPACING as f32;
                let mut ring = 0;
                while r < max_r {
                    let color = if ring % 5 == 4 { major } else { minor };
                    for i in 0..720 {
                        let a = i as f32 * std::f32::consts::TAU / 720.0;
                        let x = (cx + r * a.cos()).round() as isize;
                        let y = (cy + r * a.sin()).round() as isize;
                        if x >= 0 && y >= 0 && x < CANVAS_W as isize && y < CANVAS_H as isize {
                            let idx = y as usize * CANVAS_W + x as usize;
                            image.pixels[idx] = blend_over(image.pixels[idx], color, 1.0);
                        }
                    }
                    r += SPACING as f32;
                    ring += 1;
                }
                for i in 0..12 {
                    let a = i as f32 * std::f32::consts::TAU / 12.0;
                    for t in 0..(max_r as usize) {
                        let x = (cx + t as f32 * a.cos()).round() as isize;
                        let y = (cy + t as f32 * a.sin()).round() as isize;
                        if x >= 0 && y >= 0 && x < CANVAS_W as isize && y < CANVAS_H as isize {
                            let idx = y as usize * CANVAS_W + x as usize;
                            image.pixels[idx] = blend_over(image.pixels[idx], minor, 1.0);
                        }
                    }
                }
            }
            PaperTemplate::Log => {
                let mut x = SPACING as f32;
                let mut i = 0;
                while x < CANVAS_W as f32 {
                    let px = x.round() as usize;
                    let color = if i % 10 == 0 { major } else { minor };
                    for y in 0..CANVAS_H {
                        let idx = y * CANVAS_W + px;
                        image.pixels[idx] = blend_over(image.pixels[idx], color, 1.0);
                    }
                    x *= 1.25;
                    i += 1;
                }
                let mut y = SPACING as f32;
                let mut j = 0;
                while y < CANVAS_H as f32 {
                    let py = y.round() as usize;
                    let color = if j % 10 == 0 { major } else { minor };
                    for x in 0..CANVAS_W {
                        let idx = py * CANVAS_W + x;
                        image.pixels[idx] = blend_over(image.pixels[idx], color, 1.0);
                    }
                    y *= 1.25;
                    j += 1;
                }
            }
        }
    }
    if axes {
        let axis = Color32::from_rgba_unmultiplied(150, 150, 150, 120);
        let cx = CANVAS_W / 2;
        let cy = CANVAS_H / 2;
        for y in 0..CANVAS_H {
            for x in cx.saturating_sub(1)..=(cx + 1).min(CANVAS_W - 1) {
                let idx = y * CANVAS_W + x;
                image.pixels[idx] = blend_over(image.pixels[idx], axis, 1.0);
            }
        }
        for x in 0..CANVAS_W {
            for y in cy.saturating_sub(1)..=(cy + 1).min(CANVAS_H - 1) {
                let idx = y * CANVAS_W + x;
                image.pixels[idx] = blend_over(image.pixels[idx], axis, 1.0);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(painting: &Painting, x: usize, y: usize) -> Color32 {
        painting.image.pixels[y * CANVAS_W + x]
    }

    #[test]
    fn angle_geometry_uses_the_smaller_sweep_in_either_direction() {
        let vertex = Pos2::new(200.0, 200.0);
        let right = Pos2::new(300.0, 200.0);
        let down = Pos2::new(200.0, 300.0);

        let clockwise = angle_geometry(vertex, right, down).unwrap();
        let counter_clockwise = angle_geometry(vertex, down, right).unwrap();
        assert!((clockwise.degrees - 90.0).abs() < 0.01);
        assert!((counter_clockwise.degrees - 90.0).abs() < 0.01);
        assert!((clockwise.start - counter_clockwise.start).abs() < 0.01);
        assert!((clockwise.sweep - counter_clockwise.sweep).abs() < 0.01);
        assert_eq!(clockwise.label(), "90.0°");
        assert_eq!(clockwise.first_ray, 84.0);
        assert!(clockwise.radius <= 42.0);
        assert!(
            angle_geometry(vertex, vertex, down).is_none(),
            "a zero-length ray must not produce a bogus angle"
        );
    }

    #[test]
    fn angle_mark_respects_opacity_and_is_one_undo_step() {
        let mut painting = Painting::new();
        painting.begin_stroke();
        painting.angle_mark(
            Pos2::new(200.0, 200.0),
            Pos2::new(300.0, 200.0),
            Pos2::new(200.0, 300.0),
            3.0,
            20.0,
            Color32::WHITE,
            0.5,
        );
        painting.end_stroke();

        let mut opaque = Painting::new();
        opaque.angle_mark(
            Pos2::new(200.0, 200.0),
            Pos2::new(300.0, 200.0),
            Pos2::new(200.0, 300.0),
            3.0,
            20.0,
            Color32::WHITE,
            1.0,
        );
        let ray = at(&painting, 250, 200).a();
        let opaque_ray = at(&opaque, 250, 200).a();
        assert!(
            ray > 0 && ray < opaque_ray,
            "ray alpha: {ray} vs {opaque_ray}"
        );
        assert!(painting.image.pixels.iter().any(|px| px.a() > 0));
        assert!(painting.can_undo());
        assert!(painting.undo());
        assert!(painting.image.pixels.iter().all(|px| px.a() == 0));
    }

    #[test]
    fn stroke_can_be_undone_and_redone() {
        let mut painting = Painting::new();
        painting.begin_stroke();
        painting.stamp(40.0, 40.0, 6.0, Color32::from_rgb(255, 0, 0), 1.0, false);
        let painted = at(&painting, 40, 40);
        assert_eq!(painted, Color32::from_rgb(255, 0, 0));

        painting.end_stroke();
        assert!(painting.can_undo());
        assert!(painting.undo());
        assert_eq!(at(&painting, 40, 40), Color32::TRANSPARENT);

        assert!(painting.redo());
        assert_eq!(at(&painting, 40, 40), painted);
    }

    #[test]
    fn eraser_removes_color() {
        let mut painting = Painting::new();
        painting.begin_stroke();
        painting.stamp(40.0, 40.0, 6.0, Color32::from_rgb(0, 180, 220), 1.0, false);
        painting.stamp(40.0, 40.0, 6.0, Color32::WHITE, 1.0, true);
        assert_eq!(at(&painting, 40, 40), Color32::TRANSPARENT);
    }

    #[test]
    fn clear_is_undoable() {
        let mut painting = Painting::new();
        painting.begin_stroke();
        painting.stamp(10.0, 10.0, 4.0, Color32::WHITE, 1.0, false);
        painting.end_stroke();

        painting.clear();
        assert_eq!(at(&painting, 10, 10), Color32::TRANSPARENT);
        painting.undo();
        assert_eq!(at(&painting, 10, 10), Color32::WHITE);
    }

    #[test]
    fn selection_can_be_cut_and_pasted_with_undo() {
        let mut painting = Painting::new();
        painting.begin_stroke();
        painting.stamp(20.0, 20.0, 3.0, Color32::WHITE, 1.0, false);
        painting.end_stroke();

        let rect = RectPx {
            x: 10,
            y: 10,
            w: 20,
            h: 20,
        };
        let patch = painting.region(rect);
        assert_eq!(patch.width(), 20);
        assert_eq!(patch.pixels[10 * 20 + 10].a(), 255);

        painting.begin_stroke();
        painting.clear_region(rect);
        painting.end_stroke();
        assert_eq!(at(&painting, 20, 20), Color32::TRANSPARENT);

        painting.begin_stroke();
        painting.paste_region(&patch, 100.0, 100.0);
        painting.end_stroke();
        assert_eq!(at(&painting, 110, 110), Color32::WHITE);

        assert!(painting.undo());
        assert_eq!(at(&painting, 110, 110), Color32::TRANSPARENT);
        assert!(painting.undo());
        assert_eq!(at(&painting, 20, 20), Color32::WHITE);
    }

    #[test]
    fn partial_opacity_keeps_the_paint_color() {
        let mut painting = Painting::new();
        painting.stamp(25.0, 25.0, 3.0, Color32::from_rgb(200, 40, 60), 0.5, false);
        let painted = at(&painting, 25, 25);
        assert!((painted.a() as i32 - 128).abs() <= 1);
        let [r, g, b, _] = eframe::egui::Rgba::from(painted).to_srgba_unmultiplied();
        assert!((r as i32 - 200).abs() <= 3);
        assert!((g as i32 - 40).abs() <= 3);
        assert!((b as i32 - 60).abs() <= 3);
    }

    #[test]
    fn geometry_is_rasterized() {
        let mut painting = Painting::new();
        painting.begin_stroke();
        painting.segment(
            Pos2::new(20.0, 20.0),
            Pos2::new(120.0, 20.0),
            3.0,
            Color32::WHITE,
            1.0,
            false,
        );
        painting.ellipse(
            Pos2::new(200.0, 200.0),
            Vec2::splat(40.0),
            3.0,
            Color32::WHITE,
            1.0,
            false,
        );
        painting.end_stroke();
        assert_eq!(at(&painting, 70, 20), Color32::WHITE);
        assert_eq!(at(&painting, 240, 200), Color32::WHITE);
        assert_eq!(at(&painting, 200, 200), Color32::TRANSPARENT);
    }

    #[test]
    fn text_is_rasterized_into_the_canvas() {
        let mut painting = Painting::new();
        painting.begin_stroke();
        painting.text(50.0, 50.0, "E = mc²", 28.0, Color32::WHITE);
        painting.end_stroke();
        let painted: usize = painting.image.pixels.iter().filter(|p| p.a() > 0).count();
        assert!(painted > 50, "expected glyph pixels, got {painted}");
        assert!(painting.text_width("E = mc²", 28.0) > 40.0);
    }

    #[test]
    fn text_is_drawn_at_the_requested_position() {
        let mut painting = Painting::new();
        painting.text(400.0, 300.0, "E=mc²", 32.0, Color32::WHITE);
        let mut min_x = CANVAS_W;
        let mut min_y = CANVAS_H;
        let mut max_x = 0;
        let mut max_y = 0;
        let mut count = 0;
        for y in 0..CANVAS_H {
            for x in 0..CANVAS_W {
                if painting.image.pixels[y * CANVAS_W + x].a() > 0 {
                    count += 1;
                    min_x = min_x.min(x);
                    min_y = min_y.min(y);
                    max_x = max_x.max(x);
                    max_y = max_y.max(y);
                }
            }
        }
        assert!(count > 50, "expected glyph pixels, got {count}");
        assert!(
            (390..440).contains(&min_x),
            "text must start near x=400, got {min_x}"
        );
        assert!(
            (290..340).contains(&min_y),
            "text must start near y=300, got {min_y}"
        );
        assert!(max_x > min_x + 40, "text must advance horizontally");
        assert!(max_y > min_y + 10, "text must have glyph height");
    }

    #[test]
    fn formula_triangle_draws_dividers_and_is_one_undo_step() {
        let mut painting = Painting::new();
        painting.formula_triangle(
            Pos2::new(600.0, 400.0),
            420.0,
            280.0,
            "F",
            "m",
            "a",
            Color32::WHITE,
        );
        let points = [
            (600, 260), // apex
            (390, 540), // base left
            (810, 540), // base right
            (520, 378), // horizontal divider
            (600, 450), // vertical divider
        ];
        for (x, y) in points {
            assert!(
                at(&painting, x, y).a() > 0,
                "missing triangle line at {x},{y}"
            );
        }
        assert!(painting.can_undo());
        assert!(painting.undo());
        for (x, y) in points {
            assert_eq!(at(&painting, x, y), Color32::TRANSPARENT);
        }
    }

    #[test]
    fn saves_a_png_file() {
        let mut painting = Painting::new();
        painting.stamp(30.0, 30.0, 4.0, Color32::from_rgb(10, 20, 30), 1.0, false);
        let path = std::env::temp_dir().join(format!("spaint-test-{}.png", std::process::id()));
        painting
            .save_png(&path, true, true)
            .expect("PNG export should work");
        let bytes = std::fs::read(&path).expect("file should exist");
        assert_eq!(&bytes[1..4], b"PNG");
        std::fs::remove_file(path).ok();
    }

    #[test]
    fn sampling_ignores_transparent_pixels() {
        let mut painting = Painting::new();
        assert_eq!(painting.sample(15.0, 15.0), None);
        painting.stamp(15.0, 15.0, 3.0, Color32::from_rgb(10, 20, 30), 1.0, false);
        assert_eq!(
            painting.sample(15.0, 15.0),
            Some(Color32::from_rgb(10, 20, 30))
        );
    }
}
