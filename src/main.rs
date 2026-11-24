mod paint;
mod plot;
mod reference;
mod style;

use std::path::PathBuf;
use std::time::{Duration, Instant};

use eframe::egui;
use egui::{
    Align, Align2, Button, Color32, ColorImage, CursorIcon, FontId, Frame, Id, Key, Layout, Margin,
    Painter, PointerButton, Pos2, Rect, RichText, Rounding, Sense, Stroke, TextureOptions, Vec2,
};

use paint::{CANVAS_H, CANVAS_W, Painting, PaperTemplate, RectPx, Tool, angle_geometry};

/// Quick swatches: graphite to chalk. The custom picker is still available
/// when a diagram genuinely needs a colour, but the default language is ink.
const PALETTE: [Color32; 12] = [
    Color32::from_rgb(18, 18, 18),
    Color32::from_rgb(42, 42, 42),
    Color32::from_rgb(66, 66, 66),
    Color32::from_rgb(92, 92, 92),
    Color32::from_rgb(120, 120, 120),
    Color32::from_rgb(150, 150, 150),
    Color32::from_rgb(180, 180, 180),
    Color32::from_rgb(208, 208, 208),
    Color32::from_rgb(232, 232, 232),
    Color32::from_rgb(248, 248, 248),
    Color32::from_rgb(255, 255, 255),
    Color32::from_rgb(128, 128, 128),
];

const TOOLS: [Tool; 11] = [
    Tool::Select,
    Tool::Brush,
    Tool::Line,
    Tool::Arrow,
    Tool::Rect,
    Tool::Ellipse,
    Tool::Angle,
    Tool::Measure,
    Tool::Text,
    Tool::Eraser,
    Tool::Picker,
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum RefTab {
    Formulas,
    Constants,
    Units,
}

struct TextEditor {
    at: Pos2,
    value: String,
}

enum SelectionDrag {
    Marquee {
        start: Pos2,
        current: Pos2,
    },
    Move {
        patch: ColorImage,
        source: RectPx,
        last: RectPx,
        grab: Vec2,
    },
}

struct App {
    painting: Painting,
    texture: Option<egui::TextureHandle>,

    tool: Tool,
    rgb: [u8; 3],
    brush_size: f32,
    opacity: f32,
    text_size: f32,

    zoom: f32,
    pan: Vec2,
    pan_last: Option<Pos2>,

    drawing: bool,
    prev: Option<Pos2>,
    last: Option<Pos2>,
    shape_start: Option<Pos2>,
    shape_current: Option<Pos2>,
    angle_step: u8,
    angle_first: Option<Pos2>,
    angle_vertex: Option<Pos2>,
    text_editor: Option<TextEditor>,
    pending_text: Option<String>,

    selection: Option<RectPx>,
    selection_drag: Option<SelectionDrag>,
    clipboard: Option<ColorImage>,

    show_grid: bool,
    show_axes: bool,
    snap: bool,
    grid: f32,
    px_per_unit: f32,
    export_guides: bool,
    export_bg_paper: bool,
    export_scale: u32,
    autosave: bool,
    autosave_path: String,
    last_autosave: Instant,
    saved_revision: u64,
    paper_template: PaperTemplate,
    theme_mode: u8,
    recent_colors: Vec<[u8; 3]>,
    plot_expr: String,
    plot_xmin: f32,
    plot_xmax: f32,
    plot_ymin: f32,
    plot_ymax: f32,
    plot_width: f32,
    plot_error: Option<String>,

    ref_open: bool,
    ref_tab: RefTab,
    ref_query: String,
    ref_category: String,
    conv_from: String,
    conv_to: String,
    conv_value: String,

    save_open: bool,
    help_open: bool,
    filename: String,
    toast: Option<(String, Instant)>,

    cursor_smooth: Option<Pos2>,
    last_frame: Instant,
    layout_ready: bool,
}

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("spaint — physics & maths notebook")
            // A stable Wayland app_id lets Hyprland windowrules target the app.
            .with_app_id("spaint")
            // The window manager decides where the window goes and how big it
            // is. Keep the requested size modest and the minimum tiny so the
            // app never forces a size that a tiled Hyprland layout cannot fit.
            .with_inner_size([1100.0, 700.0])
            .with_min_inner_size([320.0, 240.0])
            .with_clamp_size_to_monitor_size(true),
        ..Default::default()
    };

    eframe::run_native(
        "spaint",
        options,
        Box::new(|cc| {
            style::install(&cc.egui_ctx);
            Ok(Box::new(App::new()))
        }),
    )
}

impl App {
    fn new() -> Self {
        let ink = style::ink();
        Self {
            painting: Painting::new(),
            texture: None,
            tool: Tool::Brush,
            rgb: [ink.r(), ink.g(), ink.b()],
            brush_size: 4.0,
            opacity: 1.0,
            text_size: 28.0,
            zoom: 1.0,
            pan: Vec2::ZERO,
            pan_last: None,
            drawing: false,
            prev: None,
            last: None,
            shape_start: None,
            shape_current: None,
            angle_step: 0,
            angle_first: None,
            angle_vertex: None,
            text_editor: None,
            pending_text: None,
            selection: None,
            selection_drag: None,
            clipboard: None,
            show_grid: true,
            show_axes: true,
            snap: false,
            grid: 40.0,
            px_per_unit: 100.0,
            export_guides: true,
            export_bg_paper: false,
            export_scale: 2,
            autosave: true,
            autosave_path: "spaint-autosave.png".to_owned(),
            last_autosave: Instant::now(),
            saved_revision: 0,
            paper_template: PaperTemplate::Square,
            theme_mode: 0,
            recent_colors: Vec::new(),
            plot_expr: "sin(x)".to_owned(),
            plot_xmin: -10.0,
            plot_xmax: 10.0,
            plot_ymin: -5.0,
            plot_ymax: 5.0,
            plot_width: 2.0,
            plot_error: None,
            ref_open: true,
            ref_tab: RefTab::Formulas,
            ref_query: String::new(),
            ref_category: "Length".to_owned(),
            conv_from: "m".to_owned(),
            conv_to: "ft".to_owned(),
            conv_value: "1".to_owned(),
            save_open: false,
            help_open: false,
            filename: "diagram.png".to_owned(),
            toast: None,
            cursor_smooth: None,
            last_frame: Instant::now(),
            layout_ready: false,
        }
    }

    fn color(&self) -> Color32 {
        Color32::from_rgb(self.rgb[0], self.rgb[1], self.rgb[2])
    }

    fn notify(&mut self, message: impl Into<String>) {
        self.toast = Some((message.into(), Instant::now() + Duration::from_millis(2400)));
    }

    fn typing_in_field(ctx: &egui::Context) -> bool {
        ctx.memory(|m| m.focused()).is_some_and(|id| {
            id == Id::new("save-name")
                || id == Id::new("ref-search")
                || id == Id::new("conv-value")
                || id == Id::new("text-editor")
                || id == Id::new("plot-expr")
        })
    }

    fn handle_keys(&mut self, ctx: &egui::Context) {
        let text_editing = self.text_editor.is_some();
        // Commit from the global input path as well. The inline TextEdit's own
        // Enter check can be missed if focus moved during the Area's first
        // (invisible) sizing pass, and this guarantees one deterministic path.
        if text_editing && ctx.input(|i| i.key_pressed(Key::Enter)) {
            if let Some(editor) = self.text_editor.take() {
                self.commit_text(editor.at, editor.value);
                self.sync_texture(ctx);
                ctx.request_repaint();
            }
            return;
        }
        let actions = ctx.input(|i| {
            let mut out = Vec::new();
            for event in &i.events {
                let egui::Event::Key {
                    key,
                    pressed: true,
                    repeat: false,
                    modifiers,
                    ..
                } = event
                else {
                    continue;
                };
                let ctrl = modifiers.ctrl || modifiers.command;
                match key {
                    Key::Z if ctrl && modifiers.shift => out.push(Action::Redo),
                    Key::Z if ctrl => out.push(Action::Undo),
                    Key::Y if ctrl => out.push(Action::Redo),
                    Key::S if ctrl => out.push(Action::Save),
                    Key::A if ctrl => out.push(Action::SelectAll),
                    Key::C if ctrl => out.push(Action::Copy),
                    Key::X if ctrl => out.push(Action::Cut),
                    Key::V if ctrl => out.push(Action::Paste),
                    Key::Delete if ctrl => out.push(Action::Clear),
                    Key::Delete | Key::Backspace => out.push(Action::DeleteSelection),
                    Key::G if ctrl => out.push(Action::ToggleGrid),
                    Key::R if ctrl => out.push(Action::ToggleReference),
                    Key::V => out.push(Action::Tool(Tool::Select)),
                    Key::B => out.push(Action::Tool(Tool::Brush)),
                    Key::L => out.push(Action::Tool(Tool::Line)),
                    Key::A => out.push(Action::Tool(Tool::Arrow)),
                    Key::R => out.push(Action::Tool(Tool::Rect)),
                    Key::O => out.push(Action::Tool(Tool::Ellipse)),
                    Key::N => out.push(Action::Tool(Tool::Angle)),
                    Key::M => out.push(Action::Tool(Tool::Measure)),
                    Key::T => out.push(Action::Tool(Tool::Text)),
                    Key::E => out.push(Action::Tool(Tool::Eraser)),
                    Key::I => out.push(Action::Tool(Tool::Picker)),
                    Key::OpenBracket => out.push(Action::Size(-1.0)),
                    Key::CloseBracket => out.push(Action::Size(1.0)),
                    Key::Questionmark => out.push(Action::Help),
                    Key::Escape => out.push(Action::Escape),
                    _ => {}
                }
            }
            out
        });

        if self.save_open && actions.iter().any(|a| matches!(a, Action::Escape)) {
            self.save_open = false;
            return;
        }
        if text_editing || Self::typing_in_field(ctx) {
            return;
        }

        for action in actions {
            match action {
                Action::Undo => {
                    self.finish_selection_drag();
                    self.finish_stroke();
                    self.reset_shape_state();
                    if !self.painting.undo() {
                        self.notify("Nothing to undo");
                    }
                }
                Action::Redo => {
                    self.finish_selection_drag();
                    self.finish_stroke();
                    self.reset_shape_state();
                    if !self.painting.redo() {
                        self.notify("Nothing to redo");
                    }
                }
                Action::Save => self.save_open = true,
                Action::Clear => {
                    self.finish_selection_drag();
                    self.finish_stroke();
                    self.reset_shape_state();
                    self.selection = None;
                    self.painting.clear();
                    self.notify("Canvas cleared");
                }
                Action::ToggleGrid => self.show_grid = !self.show_grid,
                Action::ToggleReference => self.ref_open = !self.ref_open,
                Action::Help => self.help_open = !self.help_open,
                Action::Escape => {
                    if self.help_open {
                        self.help_open = false;
                    } else if self.selection_drag.is_some() {
                        self.cancel_selection_drag();
                    } else if self.drawing {
                        self.painting.cancel_stroke();
                        self.reset_shape_state();
                    } else if self.angle_step == 1 {
                        self.reset_shape_state();
                    } else if self.selection.is_some() {
                        self.selection = None;
                    }
                }
                Action::Copy => self.copy_selection(),
                Action::Cut => self.cut_selection(),
                Action::Paste => {
                    let at = self
                        .cursor_smooth
                        .unwrap_or_else(|| Pos2::new(CANVAS_W as f32 * 0.5, CANVAS_H as f32 * 0.5));
                    self.paste_clipboard(at);
                }
                Action::DeleteSelection => self.delete_selection(),
                Action::SelectAll => {
                    self.selection = Some(RectPx {
                        x: 0,
                        y: 0,
                        w: CANVAS_W,
                        h: CANVAS_H,
                    });
                    self.notify("All selected");
                }
                Action::Tool(tool) => self.select_tool(tool),
                Action::Size(delta) => self.brush_size = (self.brush_size + delta).clamp(1.0, 60.0),
            }
        }
    }

    fn select_tool(&mut self, tool: Tool) {
        self.finish_selection_drag();
        self.finish_stroke();
        self.reset_shape_state();
        self.tool = tool;
        self.text_editor = None;
        self.pending_text = None;
    }

    fn sync_texture(&mut self, ctx: &egui::Context) {
        if self.texture.is_none() {
            self.texture = Some(ctx.load_texture(
                "diagram",
                self.painting.image.clone(),
                TextureOptions::LINEAR,
            ));
        }
        let Some(rect) = self.painting.take_upload() else {
            return;
        };
        let Some(texture) = &mut self.texture else {
            return;
        };
        let mut patch = ColorImage::new([rect.w, rect.h], Color32::TRANSPARENT);
        for y in 0..rect.h {
            let src = (rect.y + y) * CANVAS_W + rect.x;
            let dst = y * rect.w;
            patch.pixels[dst..dst + rect.w]
                .copy_from_slice(&self.painting.image.pixels[src..src + rect.w]);
        }
        texture.set_partial([rect.x, rect.y], patch, TextureOptions::LINEAR);
    }

    fn stamp_at(&mut self, p: Pos2) {
        self.painting.stamp(
            p.x,
            p.y,
            self.brush_size * 0.5,
            self.color(),
            self.opacity,
            self.tool == Tool::Eraser,
        );
    }

    fn draw_line(&mut self, from: Pos2, to: Pos2) {
        let len = (to - from).length();
        let step = (self.brush_size * 0.2).clamp(0.7, 5.0);
        let steps = (len / step).ceil().max(1.0) as usize;
        for i in 1..=steps {
            self.stamp_at(from.lerp(to, i as f32 / steps as f32));
        }
    }

    /// Smooth a freehand sample into the stroke with a quadratic curve.
    fn smooth_to(&mut self, next: Pos2) {
        let Some(last) = self.last else { return };
        let start = self.prev.map_or(last, |p| p.lerp(last, 0.5));
        let end = last.lerp(next, 0.5);
        let approx = (start - last).length() + (last - end).length();
        let step = (self.brush_size * 0.2).clamp(0.7, 5.0);
        let steps = (approx / step).ceil().max(1.0) as usize;
        for i in 1..=steps {
            let t = i as f32 / steps as f32;
            let inv = 1.0 - t;
            self.stamp_at(Pos2::new(
                inv * inv * start.x + 2.0 * inv * t * last.x + t * t * end.x,
                inv * inv * start.y + 2.0 * inv * t * last.y + t * t * end.y,
            ));
        }
        self.prev = Some(last);
        self.last = Some(next);
    }

    fn reset_shape_state(&mut self) {
        self.drawing = false;
        self.prev = None;
        self.last = None;
        self.shape_start = None;
        self.shape_current = None;
        self.angle_step = 0;
        self.angle_first = None;
        self.angle_vertex = None;
    }

    fn finish_stroke(&mut self) {
        if !self.drawing {
            return;
        }
        if !self.tool.is_shape() && self.tool != Tool::Picker {
            if let (Some(prev), Some(last)) = (self.prev, self.last) {
                self.draw_line(prev.lerp(last, 0.5), last);
            }
            self.painting.end_stroke();
        }
        self.reset_shape_state();
    }

    fn begin_selection_drag(&mut self, point: Pos2) {
        let point = self.snap_point(point);
        if let Some(rect) = self.selection {
            if rect_contains(rect, point) {
                let patch = self.painting.region(rect);
                self.painting.begin_stroke();
                self.painting.clear_region(rect);
                self.painting
                    .paste_region(&patch, rect.x as f32, rect.y as f32);
                self.selection_drag = Some(SelectionDrag::Move {
                    patch,
                    source: rect,
                    last: rect,
                    grab: point - Pos2::new(rect.x as f32, rect.y as f32),
                });
                return;
            }
        }
        self.selection = None;
        self.selection_drag = Some(SelectionDrag::Marquee {
            start: point,
            current: point,
        });
    }

    fn update_selection_drag(&mut self, point: Pos2) {
        let snapped = self.snap_point(point);
        let Some(drag) = &mut self.selection_drag else {
            return;
        };
        match drag {
            SelectionDrag::Marquee { current, .. } => {
                *current = snapped;
            }
            SelectionDrag::Move {
                patch,
                source,
                last,
                grab,
            } => {
                let max_x = CANVAS_W.saturating_sub(source.w).max(1) as f32 - 1.0;
                let max_y = CANVAS_H.saturating_sub(source.h).max(1) as f32 - 1.0;
                let x = (point.x - grab.x).round().clamp(0.0, max_x) as usize;
                let y = (point.y - grab.y).round().clamp(0.0, max_y) as usize;
                let target = RectPx {
                    x,
                    y,
                    w: source.w,
                    h: source.h,
                };
                if target != *last {
                    self.painting.clear_region(*last);
                    self.painting.paste_region(patch, x as f32, y as f32);
                    *last = target;
                }
            }
        }
    }

    fn finish_selection_drag(&mut self) {
        let Some(drag) = self.selection_drag.take() else {
            return;
        };
        match drag {
            SelectionDrag::Marquee { start, current } => {
                self.selection = rect_between(start, current).filter(|r| r.w >= 2 && r.h >= 2);
            }
            SelectionDrag::Move { last, .. } => {
                self.painting.end_stroke();
                self.selection = Some(last);
            }
        }
    }

    fn cancel_selection_drag(&mut self) {
        let Some(drag) = self.selection_drag.take() else {
            return;
        };
        if matches!(drag, SelectionDrag::Move { .. }) {
            self.painting.cancel_stroke();
        }
    }

    fn copy_selection(&mut self) {
        let Some(rect) = self.selection else {
            self.notify("Nothing selected");
            return;
        };
        self.clipboard = Some(self.painting.region(rect));
        self.notify("Selection copied");
    }

    fn cut_selection(&mut self) {
        let Some(rect) = self.selection else {
            self.notify("Nothing selected");
            return;
        };
        self.clipboard = Some(self.painting.region(rect));
        self.painting.begin_stroke();
        self.painting.clear_region(rect);
        self.painting.end_stroke();
        self.selection = None;
        self.notify("Selection cut");
    }

    fn delete_selection(&mut self) {
        let Some(rect) = self.selection else {
            return;
        };
        self.painting.begin_stroke();
        self.painting.clear_region(rect);
        self.painting.end_stroke();
        self.selection = None;
        self.notify("Selection deleted");
    }

    fn paste_clipboard(&mut self, at: Pos2) {
        let Some(patch) = self.clipboard.clone() else {
            self.notify("Clipboard is empty");
            return;
        };
        let w = patch.width().min(CANVAS_W);
        let h = patch.height().min(CANVAS_H);
        let x = (at.x - w as f32 * 0.5)
            .round()
            .clamp(0.0, (CANVAS_W - w) as f32) as usize;
        let y = (at.y - h as f32 * 0.5)
            .round()
            .clamp(0.0, (CANVAS_H - h) as f32) as usize;
        self.painting.begin_stroke();
        self.painting.paste_region(&patch, x as f32, y as f32);
        self.painting.end_stroke();
        self.selection = Some(RectPx { x, y, w, h });
        self.notify("Pasted");
    }

    fn snap_point(&self, p: Pos2) -> Pos2 {
        if !self.snap {
            return p;
        }
        Pos2::new(
            (p.x / self.grid).round() * self.grid,
            (p.y / self.grid).round() * self.grid,
        )
    }

    fn constrain(&self, start: Pos2, end: Pos2, shift: bool) -> Pos2 {
        let mut p = self.snap_point(end);
        if shift {
            let delta = p - start;
            if delta.length() > 1.0 {
                let step = std::f32::consts::FRAC_PI_4;
                let angle = (delta.angle() / step).round() * step;
                p = start + Vec2::angled(angle) * delta.length();
            }
        }
        p
    }

    fn commit_shape(&mut self, shift: bool) {
        let Some(mut end) = self.shape_current else {
            self.reset_shape_state();
            return;
        };
        let Some(start) = self.shape_start.or(self.angle_vertex) else {
            self.reset_shape_state();
            return;
        };
        end = self.constrain(start, end, shift);

        // The angle tool uses two drags: the first stores the vertex and first
        // ray as an overlay, the second commits the finished arc.
        if self.tool == Tool::Angle && self.angle_step == 0 {
            if (end - start).length_sq() < 9.0 {
                self.reset_shape_state();
                self.notify("Drag the first ray away from the vertex");
                return;
            }
            self.angle_step = 1;
            self.angle_vertex = Some(start);
            self.angle_first = Some(end);
            self.drawing = false;
            self.shape_start = Some(start);
            self.shape_current = Some(end);
            return;
        }

        let color = self.color();
        let width = self.brush_size;
        let opacity = self.opacity;

        self.painting.begin_stroke();
        match self.tool {
            Tool::Line => self
                .painting
                .segment(start, end, width, color, opacity, false),
            Tool::Arrow => self.painting.arrow(start, end, width, color, opacity),
            Tool::Rect => {
                let a = Pos2::new(start.x.min(end.x), start.y.min(end.y));
                let b = Pos2::new(start.x.max(end.x), start.y.max(end.y));
                self.painting.polyline(
                    &[a, Pos2::new(b.x, a.y), b, Pos2::new(a.x, b.y), a],
                    width,
                    color,
                    opacity,
                    false,
                );
            }
            Tool::Ellipse => {
                let center = start.lerp(end, 0.5);
                let radii = Vec2::new((end.x - start.x).abs() * 0.5, (end.y - start.y).abs() * 0.5);
                self.painting
                    .ellipse(center, radii, width, color, opacity, false);
            }
            Tool::Measure => {
                let length = (end - start).length() / self.px_per_unit.max(0.001);
                let text = format!("{length:.2} m");
                self.painting
                    .dimension(start, end, &text, width, self.text_size, color);
            }
            Tool::Angle => {
                if let Some(first) = self.angle_first {
                    if angle_geometry(start, first, end).is_none() {
                        self.notify("Drag the second ray away from the vertex");
                    } else {
                        self.painting.angle_mark(
                            start,
                            first,
                            end,
                            width,
                            self.text_size,
                            color,
                            opacity,
                        );
                    }
                }
            }
            _ => {}
        }
        self.painting.end_stroke();
        self.reset_shape_state();
    }

    fn commit_text(&mut self, at: Pos2, text: String) {
        if text.trim().is_empty() {
            self.text_editor = None;
            return;
        }
        self.painting.begin_stroke();
        self.painting
            .text(at.x, at.y, text.trim(), self.text_size, self.color());
        self.painting.end_stroke();
        self.text_editor = None;
        self.pending_text = None;
        self.notify("Text placed");
    }

    fn run_autosave(&mut self, _ctx: &egui::Context) {
        if !self.autosave || self.painting.revision == self.saved_revision {
            return;
        }
        if self.last_autosave.elapsed() < Duration::from_secs(15) {
            return;
        }
        self.finish_selection_drag();
        self.finish_stroke();
        let path = PathBuf::from(&self.autosave_path);
        match self.painting.save_png(&path, false, false) {
            Ok(()) => {
                self.saved_revision = self.painting.revision;
                self.last_autosave = Instant::now();
            }
            Err(err) => {
                self.last_autosave = Instant::now();
                self.notify(format!("Autosave failed: {err}"));
            }
        }
    }

    fn save(&mut self) {
        let mut name = self.filename.trim().to_owned();
        if name.is_empty() {
            name = "diagram.png".to_owned();
        }
        if !name.to_lowercase().ends_with(".png") {
            name.push_str(".png");
        }
        let path = PathBuf::from(name);
        let guides = self.export_guides;
        let bg = self.export_bg_paper.then(style::paper);
        match self.painting.save_png_with(
            &path,
            guides && self.show_grid,
            guides && self.show_axes,
            self.paper_template,
            self.export_scale,
            bg,
        ) {
            Ok(()) => {
                self.save_open = false;
                self.saved_revision = self.painting.revision;
                self.last_autosave = Instant::now();
                self.notify(format!("Saved {}", path.display()));
            }
            Err(err) => self.notify(format!("Save failed: {err}")),
        }
    }

    fn use_color(&mut self, rgb: [u8; 3]) {
        self.rgb = rgb;
        self.recent_colors.retain(|c| *c != rgb);
        self.recent_colors.insert(0, rgb);
        self.recent_colors.truncate(6);
    }

    fn top_bar(&mut self, ctx: &egui::Context) {
        let width = ctx.screen_rect().width();
        let tiny = width < 520.0;
        let compact = width < 860.0;
        egui::TopBottomPanel::top("top")
            .frame(Frame {
                inner_margin: Margin::symmetric(if tiny { 8.0 } else { 16.0 }, 9.0),
                fill: style::panel(),
                stroke: Stroke::new(1.0_f32, style::hairline()),
                ..Default::default()
            })
            .show(ctx, |ui| {
                ui.horizontal_centered(|ui| {
                    let mark_size = if tiny { 16.0 } else { 22.0 };
                    let mark = ui.allocate_space(Vec2::splat(mark_size)).1;
                    let painter = ui.painter();
                    painter.circle_filled(mark.center(), mark_size * 0.45, style::ink_soft());
                    painter.circle_filled(mark.center(), mark_size * 0.18, style::ink());
                    painter.circle_stroke(
                        mark.center(),
                        mark_size * 0.45,
                        Stroke::new(1.0_f32, style::alpha(style::ink(), 90.0 / 255.0)),
                    );

                    if tiny {
                        ui.add_space(6.0);
                    } else {
                        ui.add_space(8.0);
                        ui.label(RichText::new("spaint").size(16.0).strong());
                        if width >= 1040.0 {
                            ui.label(RichText::new("·").color(style::ink_dim()));
                            ui.label(
                                RichText::new("physics & maths notebook")
                                    .size(11.5)
                                    .color(style::ink_dim()),
                            );
                        }
                        ui.add_space(if compact { 10.0 } else { 18.0 });
                    }

                    let undo = Button::new(if tiny { "U" } else { "Undo" })
                        .min_size(Vec2::new(if tiny { 26.0 } else { 58.0 }, 28.0));
                    if ui.add_enabled(self.painting.can_undo(), undo).clicked() {
                        self.finish_stroke();
                        self.painting.undo();
                    }
                    let redo = Button::new(if tiny { "R" } else { "Redo" })
                        .min_size(Vec2::new(if tiny { 26.0 } else { 58.0 }, 28.0));
                    if ui.add_enabled(self.painting.can_redo(), redo).clicked() {
                        self.finish_stroke();
                        self.painting.redo();
                    }
                    if width >= 940.0 && ui.button("Clear").clicked() {
                        self.finish_stroke();
                        self.painting.clear();
                        self.notify("Canvas cleared");
                    }

                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let (short, long) = match self.theme_mode {
                            0 => ("A", "Auto"),
                            1 => ("D", "Dark"),
                            _ => ("L", "Light"),
                        };
                        if ui
                            .button(if tiny { short } else { long })
                            .on_hover_text("Theme: Auto / Dark / Light")
                            .clicked()
                        {
                            self.theme_mode = (self.theme_mode + 1) % 3;
                            let pref = match self.theme_mode {
                                0 => egui::ThemePreference::System,
                                1 => egui::ThemePreference::Dark,
                                _ => egui::ThemePreference::Light,
                            };
                            ctx.set_theme(pref);
                            style::sync(ctx);
                        }
                        ui.add_space(5.0);
                        if tiny {
                            if ui.button("Save").on_hover_text("Save PNG").clicked() {
                                self.save_open = true;
                            }
                            ui.add_space(3.0);
                            if ui.button("?").on_hover_text("Shortcuts").clicked() {
                                self.help_open = !self.help_open;
                            }
                            ui.add_space(3.0);
                            let label = if self.ref_open { "Ref ✓" } else { "Ref" };
                            if ui.button(label).on_hover_text("Reference").clicked() {
                                self.ref_open = !self.ref_open;
                            }
                        } else {
                            let save = Button::new(RichText::new("Save PNG").strong())
                                .fill(style::ink_soft())
                                .min_size(Vec2::new(88.0, 28.0));
                            if ui.add(save).clicked() {
                                self.save_open = true;
                            }
                            ui.add_space(5.0);
                            if ui.button("?").on_hover_text("Shortcuts").clicked() {
                                self.help_open = !self.help_open;
                            }
                            ui.add_space(5.0);
                            let name = if compact { "Ref" } else { "Reference" };
                            let reference = if self.ref_open {
                                Button::new(RichText::new(name).strong()).fill(style::ink_soft())
                            } else {
                                Button::new(name)
                            };
                            if ui.add(reference).clicked() {
                                self.ref_open = !self.ref_open;
                            }
                        }
                    });
                });
            });
    }

    fn sidebar(&mut self, ui: &mut egui::Ui) {
        ui.add_space(6.0);
        ui.label(
            RichText::new("DRAWING TOOLS")
                .size(10.0)
                .strong()
                .color(style::ink_dim()),
        );
        ui.add_space(8.0);

        let width = ui.available_width();
        if width >= 190.0 {
            let cell = (width - 8.0) / 2.0;
            for row in TOOLS.chunks(2) {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(8.0, 8.0);
                    for tool in row {
                        if self.tool_button(ui, *tool, cell) {
                            self.select_tool(*tool);
                        }
                    }
                });
                ui.add_space(2.0);
            }
        } else {
            for tool in TOOLS {
                if self.tool_button(ui, tool, width) {
                    self.select_tool(tool);
                }
                ui.add_space(2.0);
            }
        }

        ui.add_space(10.0);
        ui.separator();
        ui.add_space(8.0);
        ui.label(
            RichText::new("COLOR")
                .size(10.0)
                .strong()
                .color(style::ink_dim()),
        );
        ui.add_space(8.0);
        ui.spacing_mut().item_spacing = Vec2::new(6.0, 6.0);
        ui.horizontal_wrapped(|ui| {
            for swatch in PALETTE {
                let (rect, response) = ui.allocate_exact_size(Vec2::splat(27.0), Sense::click());
                let center = rect.center();
                ui.painter().circle_filled(center, 10.0, swatch);
                ui.painter().circle_stroke(
                    center,
                    10.0,
                    Stroke::new(1.0_f32, Color32::from_black_alpha(80)),
                );
                if [swatch.r(), swatch.g(), swatch.b()] == self.rgb {
                    ui.painter()
                        .circle_stroke(center, 12.5, Stroke::new(1.5_f32, style::ink()));
                }
                if response.clicked() {
                    self.use_color([swatch.r(), swatch.g(), swatch.b()]);
                }
            }
        });
        ui.spacing_mut().item_spacing = Vec2::new(8.0, 8.0);
        ui.add_space(10.0);
        if !self.recent_colors.is_empty() {
            ui.horizontal_wrapped(|ui| {
                for rgb in self.recent_colors.clone() {
                    let swatch = Color32::from_rgb(rgb[0], rgb[1], rgb[2]);
                    let (rect, response) =
                        ui.allocate_exact_size(Vec2::splat(19.0), Sense::click());
                    ui.painter()
                        .rect_filled(rect.shrink(2.0), Rounding::same(3.0), swatch);
                    if response.on_hover_text("Recent color").clicked() {
                        self.use_color(rgb);
                    }
                }
            });
        }
        ui.horizontal(|ui| {
            if ui
                .color_edit_button_srgb(&mut self.rgb)
                .on_hover_text("Custom color")
                .changed()
            {
                self.use_color(self.rgb);
            }
            let hex = format!("#{:02X}{:02X}{:02X}", self.rgb[0], self.rgb[1], self.rgb[2]);
            ui.label(
                RichText::new(hex)
                    .monospace()
                    .size(10.5)
                    .color(style::ink_dim()),
            );
        });

        ui.add_space(12.0);
        ui.separator();
        ui.add_space(8.0);
        ui.label(
            RichText::new("SELECTION")
                .size(10.0)
                .strong()
                .color(style::ink_dim()),
        );
        ui.add_space(6.0);
        if let Some(rect) = self.selection {
            ui.label(
                RichText::new(format!(
                    "{} × {} px at {}, {}",
                    rect.w, rect.h, rect.x, rect.y
                ))
                .monospace()
                .size(10.0)
                .color(style::ink()),
            );
        } else {
            ui.label(
                RichText::new("Nothing selected")
                    .size(10.0)
                    .color(style::ink_dim()),
            );
        }
        ui.spacing_mut().item_spacing = Vec2::new(5.0, 5.0);
        ui.horizontal_wrapped(|ui| {
            let has_selection = self.selection.is_some();
            let has_clipboard = self.clipboard.is_some();
            if ui.add_enabled(has_selection, Button::new("Copy")).clicked() {
                self.copy_selection();
            }
            if ui.add_enabled(has_selection, Button::new("Cut")).clicked() {
                self.cut_selection();
            }
            if ui
                .add_enabled(has_clipboard, Button::new("Paste"))
                .clicked()
            {
                let at = self
                    .cursor_smooth
                    .unwrap_or_else(|| Pos2::new(CANVAS_W as f32 * 0.5, CANVAS_H as f32 * 0.5));
                self.paste_clipboard(at);
            }
            if ui
                .add_enabled(has_selection, Button::new("Delete"))
                .clicked()
            {
                self.delete_selection();
            }
            if ui.button("All").clicked() {
                self.selection = Some(RectPx {
                    x: 0,
                    y: 0,
                    w: CANVAS_W,
                    h: CANVAS_H,
                });
            }
            if ui
                .add_enabled(has_selection, Button::new("Clear"))
                .clicked()
            {
                self.selection = None;
            }
        });
        ui.spacing_mut().item_spacing = Vec2::new(8.0, 8.0);
        ui.label(
            RichText::new("V selects · drag inside the box to move · Ctrl+C/V copy & paste")
                .size(9.5)
                .color(style::ink_dim()),
        );

        ui.add_space(12.0);
        ui.separator();
        ui.add_space(8.0);
        ui.label(
            RichText::new("STROKE")
                .size(10.0)
                .strong()
                .color(style::ink_dim()),
        );
        ui.add_space(6.0);
        let slider_w = ui.available_width();
        ui.add_sized(
            [slider_w, 18.0],
            egui::Slider::new(&mut self.brush_size, 1.0..=40.0).text("Width"),
        );
        ui.add_sized(
            [slider_w, 18.0],
            egui::Slider::new(&mut self.opacity, 0.05..=1.0)
                .text("Opacity")
                .custom_formatter(|v, _| format!("{:.0}%", v * 100.0)),
        );
        ui.add_sized(
            [slider_w, 18.0],
            egui::Slider::new(&mut self.text_size, 12.0..=64.0).text("Text"),
        );
        ui.add_sized(
            [slider_w, 18.0],
            egui::Slider::new(&mut self.grid, 20.0..=100.0)
                .text("Grid")
                .custom_formatter(|v, _| format!("{v:.0} px")),
        );
        ui.add_sized(
            [slider_w, 18.0],
            egui::Slider::new(&mut self.px_per_unit, 10.0..=500.0)
                .text("Scale")
                .custom_formatter(|v, _| format!("{v:.0} px/m")),
        );

        ui.add_space(12.0);
        ui.separator();
        ui.add_space(8.0);
        ui.label(
            RichText::new("PAPER")
                .size(10.0)
                .strong()
                .color(style::ink_dim()),
        );
        ui.add_space(6.0);
        egui::ComboBox::from_label("Template")
            .selected_text(self.paper_template.label())
            .show_ui(ui, |ui| {
                for template in PaperTemplate::ALL {
                    ui.selectable_value(&mut self.paper_template, template, template.label());
                }
            });
        ui.add_space(4.0);
        toggle(ui, "Graph grid", &mut self.show_grid);
        toggle(ui, "Centre axes", &mut self.show_axes);
        toggle(ui, "Snap to grid", &mut self.snap);
        toggle(ui, "Print guides on export", &mut self.export_guides);

        ui.add_space(12.0);
        ui.separator();
        ui.add_space(8.0);
        self.plot_panel(ui);
        ui.add_space(12.0);
        ui.separator();
        ui.add_space(8.0);
        if ui.button("Fit canvas to view").clicked() {
            self.zoom = 1.0;
            self.pan = Vec2::ZERO;
        }
        ui.label(
            RichText::new("Shift constrains · space-drag pans · Ctrl+scroll zooms")
                .size(10.0)
                .color(style::ink_dim()),
        );
        ui.add_space(8.0);
    }

    fn plot_panel(&mut self, ui: &mut egui::Ui) {
        ui.label(
            RichText::new("FUNCTION PLOT")
                .size(10.0)
                .strong()
                .color(style::ink_dim()),
        );
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.label("y =");
            ui.add(
                egui::TextEdit::singleline(&mut self.plot_expr)
                    .id(Id::new("plot-expr"))
                    .desired_width(ui.available_width()),
            );
        });
        ui.horizontal(|ui| {
            ui.label("From");
            ui.add(egui::DragValue::new(&mut self.plot_xmin).speed(0.2));
            ui.label("to");
            ui.add(egui::DragValue::new(&mut self.plot_xmax).speed(0.2));
        });
        ui.horizontal(|ui| {
            ui.label("y range");
            ui.add(egui::DragValue::new(&mut self.plot_ymin).speed(0.2));
            ui.add(egui::DragValue::new(&mut self.plot_ymax).speed(0.2));
        });
        egui::ComboBox::from_label("Preset")
            .selected_text(self.plot_expr.chars().take(28).collect::<String>())
            .show_ui(ui, |ui| {
                for (name, expr, xmin, xmax, ymin, ymax) in plot::PRESETS {
                    if ui
                        .selectable_label(self.plot_expr == *expr, *name)
                        .clicked()
                    {
                        self.plot_expr = (*expr).to_owned();
                        self.plot_xmin = *xmin as f32;
                        self.plot_xmax = *xmax as f32;
                        self.plot_ymin = *ymin as f32;
                        self.plot_ymax = *ymax as f32;
                    }
                }
            });
        ui.label(
            RichText::new("Functions: sin cos tan exp ln sqrt abs mod fract square saw tri step sec csc cot atan2 hypot clamp lerp, and cond ? a : b")
                .size(9.5)
                .color(style::ink_dim()),
        );
        ui.add_sized(
            [ui.available_width(), 18.0],
            egui::Slider::new(&mut self.plot_width, 1.0..=8.0).text("Width"),
        );
        if let Some(err) = &self.plot_error {
            ui.label(RichText::new(err).size(10.0));
        }
        if ui.button("Draw curve").clicked() {
            let xmin = self.plot_xmin as f64;
            let xmax = self.plot_xmax as f64;
            let ymin = self.plot_ymin as f64;
            let ymax = self.plot_ymax as f64;
            if xmin >= xmax || ymin >= ymax {
                self.plot_error = Some("Invalid range".to_owned());
            } else {
                let count = 1600_usize;
                match plot::sample(&self.plot_expr, xmin, xmax, count) {
                    Ok(points) => {
                        self.painting.begin_stroke();
                        let width = self.plot_width;
                        let color = self.color();
                        let opacity = self.opacity;
                        self.painting.begin_stroke();
                        let mut segment: Vec<Pos2> = Vec::new();
                        let mut prev_y: Option<f64> = None;
                        let mut drew = false;
                        for (x, y) in points {
                            let visible = y.is_finite() && y >= ymin && y <= ymax;
                            if !visible {
                                if segment.len() >= 2 {
                                    self.painting
                                        .polyline(&segment, width, color, opacity, false);
                                    drew = true;
                                }
                                segment.clear();
                                prev_y = None;
                                continue;
                            }
                            let px = (x - xmin) / (xmax - xmin) * CANVAS_W as f64;
                            let py = (1.0 - (y - ymin) / (ymax - ymin)) * CANVAS_H as f64;
                            let pos = Pos2::new(px as f32, py as f32);
                            if let Some(pyv) = prev_y {
                                let jump = (y - pyv).abs() / (ymax - ymin).max(1e-9);
                                if jump > 0.35 && segment.len() >= 2 {
                                    self.painting
                                        .polyline(&segment, width, color, opacity, false);
                                    drew = true;
                                    segment.clear();
                                }
                            }
                            segment.push(pos);
                            prev_y = Some(y);
                        }
                        if segment.len() >= 2 {
                            self.painting
                                .polyline(&segment, width, color, opacity, false);
                            drew = true;
                        }
                        self.painting.end_stroke();
                        if drew {
                            self.plot_error = None;
                            self.notify("Curve drawn");
                        } else {
                            self.plot_error = Some("Curve outside the y range".to_owned());
                        }
                    }
                    Err(err) => self.plot_error = Some(err),
                }
            }
        }
    }

    fn tool_button(&self, ui: &mut egui::Ui, tool: Tool, width: f32) -> bool {
        let id = ui.id().with(format!("tool-{tool:?}"));
        let selected = self.tool == tool;
        let t = ui.ctx().animate_bool(id, selected);
        let (rect, response) = ui.allocate_exact_size(Vec2::new(width, 38.0), Sense::click());
        let base = if response.hovered() {
            style::panel()
        } else {
            style::panel_alt()
        };
        let fill = base.lerp_to_gamma(style::ink_soft(), t * 0.42);
        ui.painter().rect_filled(rect, Rounding::same(9.0), fill);
        ui.painter().rect_stroke(
            rect,
            Rounding::same(9.0),
            Stroke::new(1.0_f32, style::alpha(style::ink(), 0.12 + t * 0.55)),
        );
        let icon_at = rect.min + Vec2::new(13.0, 19.0);
        let icon_color = if selected {
            style::ink()
        } else {
            style::ink_dim()
        };
        draw_tool_icon(ui.painter(), icon_at, tool, icon_color);
        let name = if width < 118.0 {
            short_tool_label(tool)
        } else {
            match tool {
                Tool::Brush => "Freehand",
                Tool::Arrow => "Vector",
                Tool::Ellipse => "Ellipse",
                Tool::Measure => "Measure",
                Tool::Picker => "Pick",
                _ => tool.label(),
            }
        };
        ui.painter().text(
            rect.min + Vec2::new(32.0, 19.0),
            Align2::LEFT_CENTER,
            name,
            FontId::proportional(11.5),
            if selected {
                style::ink()
            } else {
                style::ink_dim()
            },
        );
        if width >= 118.0 {
            ui.painter().text(
                rect.max - Vec2::new(9.0, 19.0),
                Align2::RIGHT_CENTER,
                tool_key(tool),
                FontId::proportional(10.0),
                style::alpha(style::ink_dim(), 0.8),
            );
        }
        response.clicked()
    }

    fn canvas(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        let available = ui.available_size();
        let padding = if available.x < 460.0 || available.y < 360.0 {
            12.0
        } else {
            30.0
        };
        let view = Vec2::new(
            (available.x - padding * 2.0).max(1.0),
            (available.y - padding * 2.0).max(1.0),
        );
        let fit = (view.x / CANVAS_W as f32).min(view.y / CANVAS_H as f32);
        let scale = fit * self.zoom;
        let size = Vec2::new(CANVAS_W as f32 * scale, CANVAS_H as f32 * scale);
        let center = ui.cursor().min + available * 0.5 + self.pan;
        let rect = Rect::from_center_size(center, size);

        let pointer = ui.input(|i| i.pointer.latest_pos());
        let primary_down = ui.input(|i| i.pointer.primary_down());
        let primary_pressed = ui.input(|i| i.pointer.primary_pressed());
        let scroll = ui.input(|i| i.raw_scroll_delta);
        let ctrl = ui.input(|i| i.modifiers.ctrl || i.modifiers.command);
        let shift = ui.input(|i| i.modifiers.shift);
        let space_down = ui.input(|i| i.key_down(Key::Space));
        let middle_down = ui.input(|i| i.pointer.button_down(PointerButton::Middle));
        let panning = space_down || middle_down;

        if let Some(p) = pointer.filter(|p| rect.contains(*p)) {
            if ctrl && scroll.y != 0.0 {
                let canvas_point = (p - rect.min) / scale;
                let factor = (scroll.y * 0.0016).clamp(-0.3, 0.3);
                self.zoom = (self.zoom * (1.0 + factor)).clamp(0.2, 10.0);
                let new_scale = fit * self.zoom;
                let new_size = Vec2::new(CANVAS_W as f32 * new_scale, CANVAS_H as f32 * new_scale);
                self.pan = p - canvas_point * new_scale - (ui.cursor().min + available * 0.5)
                    + new_size * 0.5;
            } else if !ctrl && (scroll.x != 0.0 || scroll.y != 0.0) {
                self.pan -= scroll;
            }
        }

        if panning {
            if self.drawing {
                self.finish_stroke();
            }
            if let Some(p) = pointer {
                if let Some(last) = self.pan_last {
                    self.pan += p - last;
                }
                self.pan_last = Some(p);
            }
        } else {
            self.pan_last = None;
        }

        ui.interact(rect, ui.id().with("canvas"), Sense::click_and_drag());
        let over_canvas = pointer.is_some_and(|p| rect.contains(p));
        // `is_pointer_button_down_on()` goes false as soon as the pointer
        // leaves the widget, and calling handle_canvas_down on *every* held
        // frame closed the text editor again. Track the press edge instead:
        // start on press, update the live preview while held, commit on release.
        let pointer_down = (over_canvas || self.drawing || self.selection_drag.is_some())
            && primary_down
            && !panning;
        if pointer_down {
            if let Some(p) = pointer {
                let point = Pos2::new(
                    ((p.x - rect.min.x) / scale).clamp(0.0, CANVAS_W as f32),
                    ((p.y - rect.min.y) / scale).clamp(0.0, CANVAS_H as f32),
                );
                if self.tool == Tool::Select {
                    if self.selection_drag.is_none() {
                        self.begin_selection_drag(point);
                    }
                    self.update_selection_drag(point);
                } else if primary_pressed && over_canvas {
                    self.handle_canvas_down(point, shift);
                } else if self.drawing {
                    if self.tool.is_shape() {
                        self.shape_current = Some(self.constrain(
                            self.shape_start.or(self.angle_vertex).unwrap_or(point),
                            point,
                            shift,
                        ));
                    } else {
                        self.smooth_to(point);
                    }
                }
            }
        } else if self.tool == Tool::Select {
            if self.selection_drag.is_some() {
                self.finish_selection_drag();
            }
        } else if self.drawing {
            if self.tool.is_shape() {
                if let Some(p) = pointer {
                    let point = Pos2::new(
                        ((p.x - rect.min.x) / scale).clamp(0.0, CANVAS_W as f32),
                        ((p.y - rect.min.y) / scale).clamp(0.0, CANVAS_H as f32),
                    );
                    self.shape_current = Some(self.constrain(
                        self.shape_start.or(self.angle_vertex).unwrap_or(point),
                        point,
                        shift,
                    ));
                }
                self.commit_shape(shift);
            } else {
                self.finish_stroke();
            }
        }

        self.paint_stage(ui, rect, scale, pointer, panning);
        self.paint_selection(ui.painter(), rect, scale);
        if self.tool.is_shape()
            && (self.drawing || (self.tool.is_two_step() && self.angle_step == 1))
        {
            self.paint_preview(ui.painter(), rect, scale);
        }
        if let Some(editor) = &self.text_editor {
            // Ghost of the final rasterized text on the paper itself, so the
            // inline editor is not the only indication of the result.
            if !editor.value.is_empty() {
                ui.painter().text(
                    screen(rect, editor.at, scale),
                    Align2::LEFT_TOP,
                    &editor.value,
                    FontId::proportional(self.text_size * scale),
                    style::alpha(self.color(), self.opacity.clamp(0.2, 1.0)),
                );
            }
        }
        if self.text_editor.is_some() {
            // An egui Area runs an invisible sizing pass on the first frame it
            // appears, so a single request_focus when the editor opens is
            // lost. Keep asking until the TextEdit actually owns the focus;
            // otherwise typed letters trigger tool shortcuts instead.
            let id = Id::new("text-editor");
            let focused = ctx.memory(|m| m.focused() == Some(id));
            if !focused {
                ctx.memory_mut(|m| m.request_focus(id));
            }
            self.paint_text_editor(ctx, rect, scale);
        }

        // Smooth the brush ring toward the pointer.
        let dt = self.last_frame.elapsed().as_secs_f32().min(0.08);
        self.last_frame = Instant::now();
        let k = 1.0 - (-14.0 * dt).exp();
        self.cursor_smooth = match (self.cursor_smooth, pointer) {
            (Some(old), Some(p)) => Some(old.lerp(p, k)),
            (None, Some(p)) => Some(p),
            (_, None) => None,
        };
        ui.allocate_space(available);
    }

    fn handle_canvas_down(&mut self, point: Pos2, shift: bool) {
        // Clicking elsewhere while the inline editor is open must not throw
        // the text away: place it first, then let the next click draw.
        if let Some(editor) = self.text_editor.take() {
            self.commit_text(editor.at, editor.value);
            return;
        }
        if !self.drawing {
            match self.tool {
                Tool::Picker => {
                    if let Some(color) = self.painting.sample(point.x, point.y) {
                        let [r, g, b, _] = egui::Rgba::from(color).to_srgba_unmultiplied();
                        self.rgb = [r, g, b];
                        self.notify("Color picked");
                    }
                    return;
                }
                Tool::Text => {
                    self.text_editor = Some(TextEditor {
                        at: self.snap_point(point),
                        value: self.pending_text.take().unwrap_or_default(),
                    });
                    return;
                }
                Tool::Brush | Tool::Eraser => {
                    self.painting.begin_stroke();
                    self.drawing = true;
                    self.prev = Some(point);
                    self.last = Some(point);
                    self.stamp_at(point);
                }
                _ => {
                    self.drawing = true;
                    if self.tool.is_two_step() && self.angle_step == 1 {
                        let vertex = self.angle_vertex.unwrap_or_else(|| self.snap_point(point));
                        self.shape_start = Some(vertex);
                        self.angle_vertex = Some(vertex);
                    } else {
                        self.angle_step = 0;
                        self.angle_vertex = None;
                        self.angle_first = None;
                        self.shape_start = Some(self.snap_point(point));
                    }
                    self.shape_current = Some(self.constrain(
                        self.shape_start.or(self.angle_vertex).unwrap_or(point),
                        point,
                        shift,
                    ));
                }
            }
            return;
        }
        if matches!(self.tool, Tool::Brush | Tool::Eraser) {
            self.smooth_to(point);
        }
    }

    fn paint_stage(
        &self,
        ui: &mut egui::Ui,
        rect: Rect,
        scale: f32,
        pointer: Option<Pos2>,
        panning: bool,
    ) {
        let painter = ui.painter();
        painter.rect_filled(
            rect.expand(7.0),
            Rounding::same(16.0),
            Color32::from_black_alpha(95),
        );
        painter.rect_filled(rect, Rounding::same(3.0), style::paper());

        let fade = ui
            .ctx()
            .animate_bool(Id::new("grid-fade"), self.show_grid || self.show_axes);
        if fade > 0.01 {
            draw_paper(
                painter,
                rect,
                scale,
                self.grid,
                self.paper_template,
                self.show_grid,
                self.show_axes,
                fade,
            );
        }

        if let Some(texture) = &self.texture {
            painter.image(
                texture.id(),
                rect,
                Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                Color32::WHITE,
            );
        }
        painter.rect_stroke(
            rect,
            Rounding::same(3.0),
            Stroke::new(1.0_f32, style::hairline_strong()),
        );

        if let Some(p) = pointer.filter(|p| rect.contains(*p)) {
            if !panning && self.text_editor.is_none() {
                let canvas_point =
                    Pos2::new((p.x - rect.min.x) / scale, (p.y - rect.min.y) / scale);
                let color = if self.tool == Tool::Eraser {
                    style::ink()
                } else {
                    self.color()
                };
                let target = self.cursor_smooth.unwrap_or(p);
                if self.tool != Tool::Select {
                    if self.tool.is_shape() || self.tool == Tool::Text || self.tool == Tool::Picker
                    {
                        let cross = 7.0;
                        painter.line_segment(
                            [
                                target - Vec2::new(cross, 0.0),
                                target + Vec2::new(cross, 0.0),
                            ],
                            Stroke::new(1.0_f32, style::alpha(color, 0.8)),
                        );
                        painter.line_segment(
                            [
                                target - Vec2::new(0.0, cross),
                                target + Vec2::new(0.0, cross),
                            ],
                            Stroke::new(1.0_f32, style::alpha(color, 0.8)),
                        );
                    } else {
                        let radius = (self.brush_size * 0.5 * scale).max(2.0);
                        painter.circle_stroke(
                            target,
                            radius,
                            Stroke::new(1.3_f32, style::alpha(color, 0.9)),
                        );
                        painter.circle_filled(target, 1.5, color);
                    }
                }
                if self.snap {
                    let snapped = screen(rect, self.snap_point(canvas_point), scale);
                    painter.circle_stroke(
                        snapped,
                        3.0,
                        Stroke::new(1.0_f32, style::alpha(style::ink(), 0.7)),
                    );
                }
                ui.ctx().set_cursor_icon(if self.tool == Tool::Select {
                    CursorIcon::Default
                } else {
                    CursorIcon::Crosshair
                });
            }
        }
    }

    fn paint_selection(&self, painter: &Painter, rect: Rect, scale: f32) {
        let region = match &self.selection_drag {
            Some(SelectionDrag::Marquee { start, current }) => rect_between(*start, *current),
            Some(SelectionDrag::Move { last, .. }) => Some(*last),
            None => self.selection,
        };
        let Some(region) = region else {
            return;
        };
        let screen_rect = selection_screen(region, rect, scale);
        let active = self.tool == Tool::Select;
        let color = style::ink();
        painter.rect_filled(
            screen_rect,
            Rounding::same(2.0),
            style::alpha(color, if active { 0.12 } else { 0.06 }),
        );
        dashed_rect(
            painter,
            screen_rect,
            Stroke::new(
                1.2_f32,
                style::alpha(color, if active { 0.95 } else { 0.45 }),
            ),
        );
        if active {
            for corner in [
                screen_rect.min,
                Pos2::new(screen_rect.max.x, screen_rect.min.y),
                screen_rect.max,
                Pos2::new(screen_rect.min.x, screen_rect.max.y),
            ] {
                painter.rect_filled(
                    Rect::from_center_size(corner, Vec2::splat(5.0)),
                    Rounding::same(1.0),
                    color,
                );
            }
            let label = format!("{} × {} px", region.w, region.h);
            let label_at = if screen_rect.min.y > 18.0 {
                screen_rect.min - Vec2::new(0.0, 6.0)
            } else {
                screen_rect.max + Vec2::new(0.0, 6.0)
            };
            painter.text(
                label_at,
                Align2::LEFT_BOTTOM,
                label,
                FontId::proportional(10.0),
                color,
            );
        }
    }

    fn paint_preview(&self, painter: &Painter, rect: Rect, scale: f32) {
        let (Some(start), Some(end)) = (self.shape_start.or(self.angle_vertex), self.shape_current)
        else {
            return;
        };
        let a = screen(rect, start, scale);
        let b = screen(rect, end, scale);
        // Preview the *final* look: same colour, opacity and stroke width as
        // the rasterized shape, just drawn as a live overlay while dragging.
        let color = style::alpha(self.color(), self.opacity.clamp(0.15, 1.0));
        let stroke = Stroke::new((self.brush_size * scale).max(1.2_f32), color);
        let fill = style::alpha(self.color(), self.opacity * 0.12);
        match self.tool {
            Tool::Line | Tool::Arrow => {
                painter.line_segment([a, b], stroke);
                if self.tool == Tool::Arrow {
                    let dir = (b - a).normalized();
                    let n = Vec2::new(-dir.y, dir.x);
                    for side in [-1.0, 1.0] {
                        painter.line_segment([b - dir * 12.0 + n * (side * 5.0), b], stroke);
                    }
                }
            }
            Tool::Rect => {
                let r = Rect::from_two_pos(a, b);
                painter.rect_filled(r, Rounding::same(0.0), fill);
                painter.rect_stroke(r, Rounding::same(0.0), stroke);
            }
            Tool::Ellipse => {
                let center = a.lerp(b, 0.5);
                let radii = Vec2::new((b.x - a.x).abs() * 0.5, (b.y - a.y).abs() * 0.5);
                let points: Vec<Pos2> = (0..=72)
                    .map(|i| {
                        let ang = i as f32 / 72.0 * std::f32::consts::TAU;
                        center + Vec2::new(radii.x * ang.cos(), radii.y * ang.sin())
                    })
                    .collect();
                painter.add(eframe::epaint::Shape::ellipse_filled(center, radii, fill));
                painter.add(eframe::epaint::Shape::line(points, stroke));
            }
            Tool::Measure => {
                painter.line_segment([a, b], stroke);
                let dir = (b - a).normalized();
                let n = Vec2::new(-dir.y, dir.x);
                for end in [a, b] {
                    painter.line_segment([end - n * 6.0, end + n * 6.0], stroke);
                }
                let len = (end - start).length() / self.px_per_unit.max(0.001);
                painter.text(
                    a.lerp(b, 0.5) + n * 16.0,
                    Align2::CENTER_CENTER,
                    format!("{len:.2} m"),
                    FontId::proportional(self.text_size * scale * 0.75),
                    color,
                );
            }
            Tool::Angle => {
                painter.circle_filled(a, 3.0, color);
                if let Some(first) = self.angle_first {
                    if let Some(geometry) = angle_geometry(start, first, end) {
                        painter.line_segment(
                            [
                                a,
                                a + Vec2::angled(geometry.first_angle)
                                    * (geometry.first_ray * scale),
                            ],
                            stroke,
                        );
                        painter.line_segment(
                            [
                                a,
                                a + Vec2::angled(geometry.second_angle)
                                    * (geometry.second_ray * scale),
                            ],
                            stroke,
                        );
                        let points: Vec<Pos2> = (0..=32)
                            .map(|i| {
                                let angle = geometry.start + geometry.sweep * (i as f32 / 32.0);
                                screen(rect, start + Vec2::angled(angle) * geometry.radius, scale)
                            })
                            .collect();
                        painter.add(eframe::epaint::Shape::line(points, stroke));
                        let label = geometry.label();
                        let label_pos = screen(
                            rect,
                            start + Vec2::angled(geometry.mid_angle()) * (geometry.radius + 12.0),
                            scale,
                        );
                        let label_width = self.painting.text_width(&label, self.text_size) * scale;
                        painter.text(
                            Pos2::new(label_pos.x - label_width * 0.5, label_pos.y),
                            Align2::LEFT_TOP,
                            label,
                            FontId::proportional(self.text_size * scale),
                            color,
                        );
                    } else {
                        let ray = first - start;
                        if ray.length_sq() >= 9.0 {
                            painter.line_segment(
                                [
                                    a,
                                    a + ray.normalized() * (ray.length().clamp(28.0, 84.0) * scale),
                                ],
                                stroke,
                            );
                        }
                    }
                } else {
                    painter.line_segment([a, b], stroke);
                }
            }
            _ => {}
        }
    }

    fn paint_text_editor(&mut self, ctx: &egui::Context, rect: Rect, scale: f32) {
        let Some(editor) = self.text_editor.as_mut() else {
            return;
        };
        let at = screen(rect, editor.at, scale);
        // Keep the inline editor inside the window, even in a tiny tiled one.
        let viewport = ctx.screen_rect();
        let at = Pos2::new(
            at.x.clamp(
                viewport.min.x + 4.0,
                (viewport.max.x - 290.0).max(viewport.min.x + 4.0),
            ),
            at.y.clamp(
                viewport.min.y + 4.0,
                (viewport.max.y - 90.0).max(viewport.min.y + 4.0),
            ),
        );
        let mut commit = false;
        let mut cancel = false;
        let area = egui::Area::new(Id::new("text-editor-area"))
            .order(egui::Order::Foreground)
            .current_pos(at);
        area.show(ctx, |ui| {
            Frame {
                inner_margin: Margin::symmetric(10.0, 8.0),
                fill: style::panel_alt(),
                stroke: Stroke::new(1.0_f32, style::ink_soft()),
                rounding: Rounding::same(10.0),
                shadow: egui::Shadow {
                    offset: Vec2::new(0.0, 6.0),
                    blur: 18.0,
                    spread: 0.0,
                    color: Color32::from_black_alpha(120),
                },
                ..Default::default()
            }
            .show(ui, |ui| {
                let response = ui.add(
                    egui::TextEdit::singleline(&mut editor.value)
                        .id(Id::new("text-editor"))
                        .desired_width(260.0)
                        .hint_text("Type a label or formula…"),
                );
                // Enter commits while the field is focused. `lost_focus()`
                // alone was never true on the same frame as the key press,
                // so pressing Enter appeared to do nothing.
                if ui.input(|i| i.key_pressed(Key::Enter))
                    || (response.lost_focus() && !editor.value.trim().is_empty())
                {
                    commit = true;
                }
                if ui.input(|i| i.key_pressed(Key::Escape)) {
                    cancel = true;
                }
                ui.horizontal(|ui| {
                    if ui.button("Place").clicked() {
                        commit = true;
                    }
                    if ui.button("Cancel").clicked() {
                        cancel = true;
                    }
                    ui.label(
                        RichText::new("Enter to place")
                            .size(10.0)
                            .color(style::ink_dim()),
                    );
                });
            });
        });
        let (position, text) = self
            .text_editor
            .as_ref()
            .map(|e| (e.at, e.value.clone()))
            .unzip();
        if cancel {
            self.text_editor = None;
        } else if commit {
            if let Some(position) = position {
                self.commit_text(position, text.unwrap_or_default());
                self.sync_texture(ctx);
                ctx.request_repaint();
            }
        }
    }

    fn status_bar(&self, ctx: &egui::Context) {
        let width = ctx.screen_rect().width();
        egui::TopBottomPanel::bottom("status")
            .exact_height(30.0)
            .frame(Frame {
                inner_margin: Margin::symmetric(if width < 520.0 { 8.0 } else { 16.0 }, 6.0),
                fill: style::panel(),
                stroke: Stroke::new(1.0_f32, style::hairline()),
                ..Default::default()
            })
            .show(ctx, |ui| {
                ui.horizontal_centered(|ui| {
                    ui.label(
                        RichText::new(self.tool.label())
                            .size(11.0)
                            .strong()
                            .color(style::ink()),
                    );
                    if width >= 760.0 {
                        if let Some(rect) = self.selection {
                            ui.label(RichText::new("·").color(style::ink_dim()));
                            ui.label(
                                RichText::new(format!("{}×{} selected", rect.w, rect.h))
                                    .size(10.5)
                                    .color(style::ink()),
                            );
                        }
                    }
                    if width >= 900.0 {
                        ui.label(RichText::new("·").color(style::ink_dim()));
                        let hint = if self.tool == Tool::Angle && self.angle_step == 1 {
                            "Drag the second ray · Esc cancels"
                        } else {
                            tool_hint(self.tool)
                        };
                        ui.label(RichText::new(hint).size(10.5).color(style::ink_dim()));
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if width >= 640.0 {
                            ui.label(
                                RichText::new(format!("{CANVAS_W} × {CANVAS_H}"))
                                    .size(10.5)
                                    .color(style::ink_dim()),
                            );
                            ui.label(RichText::new("·").color(style::ink_dim()));
                        }
                        ui.label(
                            RichText::new(format!("{}%", (self.zoom * 100.0).round() as i32))
                                .size(10.5)
                                .color(style::ink_dim()),
                        );
                        if width >= 780.0 {
                            ui.label(RichText::new("·").color(style::ink_dim()));
                            let coords = self
                                .cursor_smooth
                                .map(|p| format!("{}, {}", p.x.round() as i32, p.y.round() as i32))
                                .unwrap_or_else(|| "—, —".to_owned());
                            ui.label(
                                RichText::new(coords)
                                    .monospace()
                                    .size(10.5)
                                    .color(style::ink_dim()),
                            );
                        }
                    });
                });
            });
    }

    fn reference_contents(&mut self, ui: &mut egui::Ui, close: &mut bool) {
        ui.horizontal(|ui| {
            ui.label(RichText::new("Reference").size(14.0).strong());
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.button("×").on_hover_text("Close").clicked() {
                    *close = false;
                }
            });
        });
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(4.0, 4.0);
            let w = (ui.available_width() - 8.0) / 3.0;
            for (tab, name) in [
                (RefTab::Formulas, "Formulas"),
                (RefTab::Constants, "Constants"),
                (RefTab::Units, "Units"),
            ] {
                let active = self.ref_tab == tab;
                let button = if active {
                    Button::new(RichText::new(name).size(11.0).strong())
                        .fill(style::ink_soft())
                        .min_size(Vec2::new(w, 26.0))
                } else {
                    Button::new(RichText::new(name).size(11.0)).min_size(Vec2::new(w, 26.0))
                };
                if ui.add(button).clicked() {
                    self.ref_tab = tab;
                }
            }
        });
        ui.add_space(8.0);

        match self.ref_tab {
            RefTab::Formulas => self.formulas_tab(ui),
            RefTab::Constants => self.constants_tab(ui),
            RefTab::Units => self.units_tab(ui),
        }
    }

    fn reference_panel(&mut self, ctx: &egui::Context) {
        if !self.ref_open && ctx.animate_bool(Id::new("ref-fade"), false) < 0.01 {
            return;
        }
        let fade = ctx.animate_bool(Id::new("ref-fade"), self.ref_open);
        let mut open = self.ref_open;
        let screen = ctx.screen_rect();

        if screen.width() < 1000.0 {
            // On narrower windows the reference floats above the canvas so the
            // diagram keeps as much space as possible. Never ask for a window
            // larger than the actual viewport (tiled WMs can be very small).
            let width = (screen.width() - 24.0).clamp(200.0, 330.0);
            let height = (screen.height() - 130.0).clamp(260.0, 720.0);
            // Keep the whole floating window inside the viewport, including
            // the gap above it. On short tiled windows 66 px of top offset can
            // otherwise push its bottom edge past the screen.
            let pad = 8.0;
            let top = (screen.height() - 60.0).clamp(pad, 66.0);
            let max = Vec2::new(
                (screen.width() - 2.0 * pad).max(180.0),
                (screen.height() - top - pad).max(220.0),
            );
            let mut visible = true;
            egui::Window::new("Reference")
                .open(&mut open)
                .anchor(Align2::RIGHT_TOP, Vec2::new(-pad, top))
                .default_size(Vec2::new(width.min(max.x), height.min(max.y)))
                .min_size(Vec2::new(180.0, 220.0))
                .max_size(max)
                .resizable(true)
                .collapsible(false)
                .scroll([false, true])
                .show(ctx, |ui| self.reference_contents(ui, &mut visible));
            if !visible {
                open = false;
            }
        } else {
            egui::SidePanel::right("reference")
                .resizable(true)
                .default_width(300.0)
                .min_width(160.0)
                .max_width(480.0)
                .frame(Frame {
                    inner_margin: Margin::symmetric(14.0, 10.0),
                    fill: style::alpha(style::panel(), fade),
                    stroke: Stroke::new(1.0_f32, style::hairline()),
                    ..Default::default()
                })
                .show(ctx, |ui| {
                    egui::ScrollArea::vertical()
                        .auto_shrink([false, true])
                        .show(ui, |ui| self.reference_contents(ui, &mut open));
                });
        }
        self.ref_open = open;
    }

    fn formulas_tab(&mut self, ui: &mut egui::Ui) {
        let query = self.ref_query.to_lowercase();
        ui.add(
            egui::TextEdit::singleline(&mut self.ref_query)
                .id(Id::new("ref-search"))
                .desired_width(ui.available_width())
                .hint_text("Search name, symbol or topic…"),
        );
        ui.add_space(8.0);
        let mut topics: Vec<&str> = Vec::new();
        for formula in reference::FORMULAS {
            let haystack = format!(
                "{} {} {} {}",
                formula.topic, formula.name, formula.expr, formula.note
            )
            .to_lowercase();
            if (query.is_empty() || haystack.contains(&query)) && !topics.contains(&formula.topic) {
                topics.push(formula.topic);
            }
        }
        if topics.is_empty() {
            ui.label(RichText::new("No matching formulas").color(style::ink_dim()));
            return;
        }
        egui::ScrollArea::vertical().show(ui, |ui| {
            for topic in topics {
                ui.label(
                    RichText::new(topic.to_uppercase())
                        .size(10.0)
                        .strong()
                        .color(style::ink()),
                );
                ui.add_space(4.0);
                for formula in reference::FORMULAS.iter().filter(|f| {
                    f.topic == topic
                        && (query.is_empty()
                            || format!("{} {} {} {}", f.topic, f.name, f.expr, f.note)
                                .to_lowercase()
                                .contains(&query))
                }) {
                    let tri = reference::triangle(formula);
                    let mut copy = false;
                    let mut place_text = false;
                    let mut place_triangle = false;
                    Frame {
                        inner_margin: Margin::symmetric(9.0, 7.0),
                        fill: style::panel_alt(),
                        stroke: Stroke::new(1.0_f32, style::hairline()),
                        rounding: Rounding::same(9.0),
                        ..Default::default()
                    }
                    .show(ui, |ui| {
                        ui.label(RichText::new(formula.name).size(12.0).strong());
                        ui.horizontal(|ui| {
                            ui.vertical(|ui| {
                                ui.label(
                                    RichText::new(formula.expr)
                                        .monospace()
                                        .size(12.0)
                                        .color(style::ink()),
                                );
                                ui.label(
                                    RichText::new(formula.note)
                                        .size(10.0)
                                        .color(style::ink_dim()),
                                );
                            });
                            let (rect, response) = ui.allocate_exact_size(
                                Vec2::new(124.0_f32.min(ui.available_width() * 0.52), 82.0),
                                Sense::hover(),
                            );
                            draw_formula_triangle(ui.painter(), rect, tri);
                            response.on_hover_text(if tri.exact {
                                "Three-factor triangle"
                            } else {
                                "Grouped as terms × 1"
                            });
                        });
                        ui.horizontal_wrapped(|ui| {
                            if ui.button("Copy").clicked() {
                                copy = true;
                            }
                            if ui.button("Place text").clicked() {
                                place_text = true;
                            }
                            if ui.button("Place △").clicked() {
                                place_triangle = true;
                            }
                        });
                    });
                    if copy {
                        copy_text(ctx_clone(ui), formula.expr);
                        self.notify("Formula copied");
                    }
                    if place_text {
                        // Place it right away. Earlier this only pre-filled the
                        // inline editor, which made "Place on canvas" look as
                        // if it did nothing when the editor lost focus.
                        let ctx = ctx_clone(ui);
                        let at = Pos2::new(CANVAS_W as f32 * 0.5, CANVAS_H as f32 * 0.3);
                        self.commit_text(at, formula.expr.to_owned());
                        self.sync_texture(&ctx);
                        ctx.request_repaint();
                        self.tool = Tool::Text;
                        self.notify("Formula placed on canvas");
                    }
                    if place_triangle {
                        let ctx = ctx_clone(ui);
                        let at = Pos2::new(CANVAS_W as f32 * 0.5, CANVAS_H as f32 * 0.45);
                        self.painting.formula_triangle(
                            at,
                            420.0,
                            280.0,
                            tri.top,
                            tri.left,
                            tri.right,
                            self.color(),
                        );
                        self.sync_texture(&ctx);
                        ctx.request_repaint();
                        self.tool = Tool::Brush;
                        self.notify("Formula triangle placed on canvas");
                    }
                    ui.add_space(6.0);
                }
                ui.add_space(6.0);
            }
        });
    }

    fn constants_tab(&mut self, ui: &mut egui::Ui) {
        let query = self.ref_query.to_lowercase();
        ui.add(
            egui::TextEdit::singleline(&mut self.ref_query)
                .id(Id::new("ref-search"))
                .desired_width(ui.available_width())
                .hint_text("Search constants…"),
        );
        ui.add_space(8.0);
        egui::ScrollArea::vertical().show(ui, |ui| {
            for constant in reference::CONSTANTS {
                let haystack = format!(
                    "{} {} {} {}",
                    constant.name, constant.symbol, constant.value, constant.unit
                )
                .to_lowercase();
                if !query.is_empty() && !haystack.contains(&query) {
                    continue;
                }
                let mut copy = false;
                Frame {
                    inner_margin: Margin::symmetric(9.0, 7.0),
                    fill: style::panel_alt(),
                    stroke: Stroke::new(1.0_f32, style::hairline()),
                    rounding: Rounding::same(9.0),
                    ..Default::default()
                }
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(constant.symbol)
                                .monospace()
                                .size(14.0)
                                .strong()
                                .color(style::ink()),
                        );
                        ui.label(RichText::new(constant.name).size(11.5));
                    });
                    ui.label(
                        RichText::new(format!("{} {}", constant.value, constant.unit))
                            .monospace()
                            .size(11.0)
                            .color(style::ink_dim()),
                    );
                    if ui.button("Copy value").clicked() {
                        copy = true;
                    }
                });
                if copy {
                    copy_text(
                        ctx_clone(ui),
                        format!("{} = {} {}", constant.symbol, constant.value, constant.unit),
                    );
                    self.notify("Constant copied");
                }
                ui.add_space(6.0);
            }
        });
    }

    fn units_tab(&mut self, ui: &mut egui::Ui) {
        let categories = reference::unit_categories();
        if !categories.contains(&self.ref_category.as_str()) {
            self.ref_category = categories[0].to_owned();
        }
        let from_units: Vec<&reference::Unit> = reference::UNITS
            .iter()
            .filter(|u| u.category == self.ref_category)
            .collect();
        if !from_units.iter().any(|u| u.symbol == self.conv_from) {
            self.conv_from = from_units[0].symbol.to_owned();
        }
        if !from_units.iter().any(|u| u.symbol == self.conv_to) {
            self.conv_to = from_units[1.min(from_units.len() - 1)].symbol.to_owned();
        }

        ui.label(
            RichText::new("Quantity")
                .size(10.0)
                .strong()
                .color(style::ink_dim()),
        );
        let before = self.ref_category.clone();
        egui::ComboBox::from_id_salt("ref-category")
            .selected_text(self.ref_category.clone())
            .width(ui.available_width())
            .show_ui(ui, |ui| {
                for category in &categories {
                    ui.selectable_value(&mut self.ref_category, (*category).to_owned(), *category);
                }
            });
        if before != self.ref_category {
            let units: Vec<_> = reference::UNITS
                .iter()
                .filter(|u| u.category == self.ref_category)
                .collect();
            self.conv_from = units[0].symbol.to_owned();
            self.conv_to = units[1.min(units.len() - 1)].symbol.to_owned();
        }
        ui.add_space(10.0);

        ui.label(
            RichText::new("From → to")
                .size(10.0)
                .strong()
                .color(style::ink_dim()),
        );
        ui.horizontal(|ui| {
            // ComboBox::width is a *minimum* width: without truncation a long
            // symbol ("μm", "km/h", …) makes the button wider than the slot,
            // so the two boxes together overflow the reference window.
            let gap = ui.spacing().item_spacing.x;
            let arrow_w = 14.0;
            let combo_w = ((ui.available_width() - 2.0 * gap - arrow_w) / 2.0).max(48.0);
            ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);
            egui::ComboBox::from_id_salt("conv-from")
                .selected_text(self.conv_from.clone())
                .width(combo_w)
                .show_ui(ui, |ui| {
                    for unit in reference::units_in(&self.ref_category) {
                        ui.selectable_value(
                            &mut self.conv_from,
                            unit.symbol.to_owned(),
                            unit.symbol,
                        )
                        .on_hover_text(unit.name);
                    }
                });
            ui.label(RichText::new("→").color(style::ink_dim()));
            egui::ComboBox::from_id_salt("conv-to")
                .selected_text(self.conv_to.clone())
                .width(combo_w)
                .show_ui(ui, |ui| {
                    for unit in reference::units_in(&self.ref_category) {
                        ui.selectable_value(&mut self.conv_to, unit.symbol.to_owned(), unit.symbol)
                            .on_hover_text(unit.name);
                    }
                });
        });
        ui.add_space(8.0);
        // Lay out right-to-left so the unit label is allocated first and the
        // input field only ever takes the width that is left over.
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.label(
                RichText::new(&self.conv_from)
                    .monospace()
                    .color(style::ink_dim()),
            );
            ui.add(
                egui::TextEdit::singleline(&mut self.conv_value)
                    .id(Id::new("conv-value"))
                    .desired_width(ui.available_width().min(160.0)),
            );
        });
        ui.add_space(12.0);

        let from = reference::UNITS
            .iter()
            .find(|u| u.category == self.ref_category && u.symbol == self.conv_from);
        let to = reference::UNITS
            .iter()
            .find(|u| u.category == self.ref_category && u.symbol == self.conv_to);
        let value = self.conv_value.trim().parse::<f64>().ok();
        let result = match (value, from, to) {
            (Some(v), Some(from), Some(to)) => reference::convert(v, from, to),
            _ => None,
        };
        Frame {
            inner_margin: Margin::symmetric(12.0, 12.0),
            fill: style::panel_alt(),
            stroke: Stroke::new(1.0_f32, style::ink_soft()),
            rounding: Rounding::same(10.0),
            ..Default::default()
        }
        .show(ui, |ui| {
            ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);
            if let Some(result) = result {
                ui.label(
                    RichText::new("RESULT")
                        .size(9.5)
                        .strong()
                        .color(style::ink_dim()),
                );
                ui.label(
                    RichText::new(format!(
                        "{} {}",
                        reference::format_number(result),
                        self.conv_to
                    ))
                    .monospace()
                    .size(20.0)
                    .color(style::ink()),
                );
                if ui.button("Copy result").clicked() {
                    copy_text(
                        ctx_clone(ui),
                        format!("{} {}", reference::format_number(result), self.conv_to),
                    );
                    self.notify("Result copied");
                }
            } else {
                ui.label(RichText::new("Enter a number to convert").color(style::ink_dim()));
            }
        });
        ui.add_space(10.0);
        ui.label(
            RichText::new(
                "SI base units are used as the bridge. Includes SI, imperial, astronomical, electric, magnetic and radiation units.",
            )
            .size(10.0)
            .color(style::ink_dim()),
        );
    }

    fn dialogs(&mut self, ctx: &egui::Context) {
        let screen = ctx.screen_rect();
        let max = Vec2::new(
            (screen.width() - 16.0).max(160.0),
            (screen.height() - 16.0).max(160.0),
        );
        let mut save_clicked = false;
        let mut cancel_clicked = false;
        egui::Window::new("Save diagram")
            .open(&mut self.save_open)
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .max_size(max)
            .resizable(false)
            .collapsible(false)
            .show(ctx, |ui| {
                ui.label("File name:");
                ui.add_space(5.0);
                ui.add(
                    egui::TextEdit::singleline(&mut self.filename)
                        .id(Id::new("save-name"))
                        .desired_width(ui.available_width()),
                );
                ui.add_space(10.0);
                ui.separator();
                ui.add_space(6.0);
                ui.label(
                    RichText::new("EXPORT")
                        .size(10.0)
                        .strong()
                        .color(style::ink_dim()),
                );
                toggle(ui, "Opaque paper background", &mut self.export_bg_paper);
                ui.horizontal(|ui| {
                    ui.label("Scale");
                    for scale in [1_u32, 2, 3, 4] {
                        ui.selectable_value(&mut self.export_scale, scale, format!("{scale}x"));
                    }
                });
                toggle(ui, "Autosave every 15 s", &mut self.autosave);
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    if ui.button("Cancel").clicked() {
                        cancel_clicked = true;
                    }
                    let save =
                        Button::new(RichText::new("Save PNG").strong()).fill(style::ink_soft());
                    if ui.add(save).clicked() {
                        save_clicked = true;
                    }
                });
            });
        if save_clicked {
            self.save();
        }
        if cancel_clicked {
            self.save_open = false;
        }

        egui::Window::new("Shortcuts")
            .open(&mut self.help_open)
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .max_size(max)
            .resizable(false)
            .collapsible(false)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.add(
                        egui::Image::new(egui::include_image!("../assets/logo-ink.png"))
                            .fit_to_exact_size(Vec2::splat(48.0))
                            .tint(style::ink()),
                    );
                    ui.vertical(|ui| {
                        ui.add_space(4.0);
                        ui.label(RichText::new("spaint").strong().size(16.0));
                        ui.label(
                            RichText::new("ink on paper · physics notebook")
                                .size(10.0)
                                .color(style::ink_dim()),
                        );
                    });
                });
                ui.separator();
                ui.add_space(4.0);
                for (key, action) in [
                    ("V", "Select tool (drag a box, then drag inside to move)"),
                    ("B / L / A", "Freehand / line / vector"),
                    ("R / O / N", "Rectangle / ellipse / angle"),
                    ("M / T", "Measure / text"),
                    ("E / I", "Eraser / pick color"),
                    ("Shift + drag", "Constrain to 45°"),
                    ("[ / ]", "Stroke width"),
                    ("Ctrl + A", "Select all"),
                    ("Ctrl + C / X / V", "Copy / cut / paste selection"),
                    ("Delete", "Delete selection"),
                    ("Esc", "Cancel drag or deselect"),
                    ("Ctrl + Z / Y", "Undo / redo"),
                    ("Ctrl + S", "Save PNG"),
                    ("Ctrl + G", "Toggle grid"),
                    ("Ctrl + R", "Toggle reference"),
                    ("Ctrl + Delete", "Clear canvas"),
                    ("Space + drag", "Pan canvas"),
                    ("Ctrl + scroll", "Zoom"),
                ] {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(key).monospace().strong());
                        ui.label(RichText::new(action).color(style::ink_dim()));
                    });
                }
            });

        let now = Instant::now();
        let active = self.toast.as_ref().is_some_and(|(_, until)| now < *until);
        let fade = ctx.animate_bool(Id::new("toast-fade"), active);
        if active || fade > 0.01 {
            egui::Area::new(Id::new("toast"))
                .anchor(
                    Align2::CENTER_BOTTOM,
                    Vec2::new(0.0, -48.0 + (1.0 - fade) * 12.0),
                )
                .show(ctx, |ui| {
                    Frame {
                        inner_margin: Margin::symmetric(16.0, 9.0),
                        fill: style::alpha(style::panel_alt(), fade),
                        rounding: Rounding::same(10.0),
                        stroke: Stroke::new(1.0_f32, style::alpha(style::ink_soft(), fade)),
                        shadow: egui::Shadow {
                            offset: Vec2::new(0.0, 5.0),
                            blur: 16.0,
                            spread: 0.0,
                            color: Color32::from_black_alpha(110),
                        },
                        ..Default::default()
                    }
                    .show(ui, |ui| {
                        if let Some((message, _)) = &self.toast {
                            ui.label(RichText::new(message).size(12.0));
                        }
                    });
                });
        } else if !active && self.toast.is_some() {
            self.toast = None;
        }
        if active {
            ctx.request_repaint();
        }
    }
}

enum Action {
    Undo,
    Redo,
    Save,
    Clear,
    Copy,
    Cut,
    Paste,
    DeleteSelection,
    SelectAll,
    ToggleGrid,
    ToggleReference,
    Help,
    Escape,
    Tool(Tool),
    Size(f32),
}

fn screen(rect: Rect, point: Pos2, scale: f32) -> Pos2 {
    rect.min + point.to_vec2() * scale
}

fn rect_between(a: Pos2, b: Pos2) -> Option<RectPx> {
    let x0 = a.x.min(b.x).floor().clamp(0.0, CANVAS_W as f32) as usize;
    let y0 = a.y.min(b.y).floor().clamp(0.0, CANVAS_H as f32) as usize;
    let x1 = a.x.max(b.x).ceil().clamp(0.0, CANVAS_W as f32) as usize;
    let y1 = a.y.max(b.y).ceil().clamp(0.0, CANVAS_H as f32) as usize;
    (x1 > x0 && y1 > y0).then_some(RectPx {
        x: x0,
        y: y0,
        w: x1 - x0,
        h: y1 - y0,
    })
}

fn dashed_rect(painter: &Painter, rect: Rect, stroke: Stroke) {
    let dash = 7.0_f32;
    let gap = 5.0_f32;
    let edges = [
        (rect.min, Pos2::new(rect.max.x, rect.min.y)),
        (Pos2::new(rect.max.x, rect.min.y), rect.max),
        (rect.max, Pos2::new(rect.min.x, rect.max.y)),
        (Pos2::new(rect.min.x, rect.max.y), rect.min),
    ];
    for (a, b) in edges {
        let delta = b - a;
        let len = delta.length();
        if len < 1.0 {
            continue;
        }
        let dir = delta / len;
        let mut at = 0.0_f32;
        while at < len {
            let end = (at + dash).min(len);
            painter.line_segment([a + dir * at, a + dir * end], stroke);
            at += dash + gap;
        }
    }
}

fn rect_contains(rect: RectPx, p: Pos2) -> bool {
    p.x >= rect.x as f32
        && p.x < (rect.x + rect.w) as f32
        && p.y >= rect.y as f32
        && p.y < (rect.y + rect.h) as f32
}

fn selection_screen(rect: RectPx, stage: Rect, scale: f32) -> Rect {
    Rect::from_min_max(
        screen(stage, Pos2::new(rect.x as f32, rect.y as f32), scale),
        screen(
            stage,
            Pos2::new((rect.x + rect.w) as f32, (rect.y + rect.h) as f32),
            scale,
        ),
    )
}

fn tool_key(tool: Tool) -> &'static str {
    match tool {
        Tool::Select => "V",
        Tool::Brush => "B",
        Tool::Line => "L",
        Tool::Arrow => "A",
        Tool::Rect => "R",
        Tool::Ellipse => "O",
        Tool::Angle => "N",
        Tool::Measure => "M",
        Tool::Text => "T",
        Tool::Eraser => "E",
        Tool::Picker => "I",
    }
}

fn short_tool_label(tool: Tool) -> &'static str {
    match tool {
        Tool::Select => "Select",
        Tool::Brush => "Draw",
        Tool::Line => "Line",
        Tool::Arrow => "Arrow",
        Tool::Rect => "Rect",
        Tool::Ellipse => "Ellipse",
        Tool::Angle => "Angle",
        Tool::Measure => "Measure",
        Tool::Text => "Text",
        Tool::Eraser => "Erase",
        Tool::Picker => "Pick",
    }
}

fn tool_hint(tool: Tool) -> &'static str {
    match tool {
        Tool::Select => "Drag a box to select · drag inside to move · Del to delete",
        Tool::Brush => "Draw a smooth freehand stroke",
        Tool::Line => "Drag to draw a straight line · Shift to constrain",
        Tool::Arrow => "Drag to draw a vector arrow",
        Tool::Rect => "Drag between opposite corners",
        Tool::Ellipse => "Drag to set the bounding box",
        Tool::Angle => "Drag the first ray, then the second",
        Tool::Measure => "Drag between two points for a distance",
        Tool::Text => "Click to place a text or formula label",
        Tool::Eraser => "Drag to erase pixels",
        Tool::Picker => "Click a painted pixel to pick its color",
    }
}

fn copy_text(ctx: egui::Context, text: impl Into<String>) {
    ctx.output_mut(|o| o.copied_text = text.into());
}

fn ctx_clone(ui: &egui::Ui) -> egui::Context {
    ui.ctx().clone()
}

fn toggle(ui: &mut egui::Ui, label: &str, value: &mut bool) {
    let response = ui.add(egui::Checkbox::new(value, label));
    response.on_hover_text("Toggle");
}

fn draw_tool_icon(painter: &Painter, center: Pos2, tool: Tool, color: Color32) {
    let stroke = Stroke::new(1.4_f32, color);
    let c = center;
    match tool {
        Tool::Select => {
            let box_rect = Rect::from_center_size(c, Vec2::splat(12.0));
            painter.rect_filled(box_rect, Rounding::same(1.0), style::alpha(color, 0.12));
            dashed_rect(painter, box_rect, stroke);
            for corner in [
                box_rect.min,
                Pos2::new(box_rect.max.x, box_rect.min.y),
                box_rect.max,
                Pos2::new(box_rect.min.x, box_rect.max.y),
            ] {
                painter.rect_filled(
                    Rect::from_center_size(corner, Vec2::splat(3.0)),
                    Rounding::same(1.0),
                    color,
                );
            }
        }
        Tool::Brush => {
            painter.line_segment([c - Vec2::new(5.0, 5.0), c + Vec2::new(2.0, 2.0)], stroke);
            painter.circle_filled(c + Vec2::new(4.0, 4.0), 2.0, color);
        }
        Tool::Line => {
            painter.line_segment([c - Vec2::new(6.0, 5.0), c + Vec2::new(6.0, -5.0)], stroke);
        }
        Tool::Arrow => {
            let tip = c + Vec2::new(6.0, -5.0);
            painter.line_segment([c - Vec2::new(6.0, 5.0), tip], stroke);
            painter.line_segment([tip - Vec2::new(5.0, 0.0), tip], stroke);
            painter.line_segment([tip - Vec2::new(0.0, 5.0), tip], stroke);
        }
        Tool::Rect => {
            painter.rect_stroke(
                Rect::from_center_size(c, Vec2::splat(12.0)),
                Rounding::same(0.0),
                stroke,
            );
        }
        Tool::Ellipse => {
            let points: Vec<Pos2> = (0..=32)
                .map(|i| {
                    let a = i as f32 / 32.0 * std::f32::consts::TAU;
                    c + Vec2::new(6.5 * a.cos(), 5.0 * a.sin())
                })
                .collect();
            painter.add(eframe::epaint::Shape::line(points, stroke));
        }
        Tool::Angle => {
            painter.line_segment([c + Vec2::new(-6.0, 5.0), c + Vec2::new(6.0, 5.0)], stroke);
            painter.line_segment([c + Vec2::new(-6.0, 5.0), c + Vec2::new(4.0, -5.0)], stroke);
            let origin = c + Vec2::new(-6.0, 5.0);
            let points: Vec<Pos2> = (0..=10)
                .map(|i| origin + Vec2::angled(-0.8 + 0.8 * (i as f32 / 10.0)) * 6.0)
                .collect();
            painter.add(eframe::epaint::Shape::line(points, stroke));
        }
        Tool::Measure => {
            painter.line_segment([c - Vec2::new(6.0, 0.0), c + Vec2::new(6.0, 0.0)], stroke);
            for x in [-6.0, 6.0] {
                painter.line_segment([c + Vec2::new(x, -3.0), c + Vec2::new(x, 3.0)], stroke);
            }
        }
        Tool::Text => {
            painter.text(
                c,
                Align2::CENTER_CENTER,
                "T",
                FontId::proportional(14.0),
                color,
            );
        }
        Tool::Eraser => {
            let r = Rect::from_center_size(c, Vec2::new(12.0, 7.0));
            painter.rect_filled(r, Rounding::same(2.0), style::alpha(color, 0.25));
            painter.rect_stroke(r, Rounding::same(2.0), stroke);
        }
        Tool::Picker => {
            painter.line_segment([c - Vec2::new(4.0, 6.0), c + Vec2::new(4.0, -3.0)], stroke);
            painter.line_segment([c + Vec2::new(4.0, -3.0), c + Vec2::new(6.0, -5.0)], stroke);
            painter.circle_stroke(c - Vec2::new(5.0, 7.0), 2.5, stroke);
        }
    }
}

/// Compact reference preview of a formula triangle. It mirrors the geometry
/// used by `Painting::formula_triangle` so the panel preview and the canvas
/// stroke read the same way.
fn draw_formula_triangle(painter: &Painter, rect: Rect, tri: reference::FormulaTriangle<'_>) {
    let center = rect.center();
    let w = rect.width().min(124.0);
    let h = rect.height().min(82.0);
    let apex_y = center.y - h * 0.5;
    let base_y = center.y + h * 0.5;
    let divider_y = center.y - h * 0.08;
    let divider_half = w * 0.5 * ((divider_y - apex_y) / (base_y - apex_y));

    let apex = Pos2::new(center.x, apex_y);
    let left = Pos2::new(center.x - w * 0.5, base_y);
    let right = Pos2::new(center.x + w * 0.5, base_y);
    let ink = style::ink();
    let line = Stroke::new(1.8_f32, style::alpha(ink, 0.92));
    painter.line_segment([apex, left], line);
    painter.line_segment([left, right], line);
    painter.line_segment([right, apex], line);
    painter.line_segment(
        [
            Pos2::new(center.x - divider_half, divider_y),
            Pos2::new(center.x + divider_half, divider_y),
        ],
        line,
    );
    painter.line_segment(
        [Pos2::new(center.x, divider_y), Pos2::new(center.x, base_y)],
        line,
    );

    let longest = tri
        .top
        .chars()
        .count()
        .max(tri.left.chars().count())
        .max(tri.right.chars().count())
        .max(1);
    let size = (h * 0.18)
        .clamp(9.0, 14.0)
        .min(w * 0.52 / (longest as f32 * 0.62))
        .max(7.5);
    let top_y = apex_y + (divider_y - apex_y) * 0.5 - size * 0.55;
    let bottom_y = divider_y + (base_y - divider_y) * 0.5 - size * 0.55;
    painter.text(
        Pos2::new(center.x, top_y),
        Align2::CENTER_TOP,
        tri.top,
        FontId::monospace(size),
        ink,
    );
    painter.text(
        Pos2::new(center.x - w * 0.22, bottom_y),
        Align2::CENTER_TOP,
        tri.left,
        FontId::monospace(size),
        ink,
    );
    painter.text(
        Pos2::new(center.x + w * 0.22, bottom_y),
        Align2::CENTER_TOP,
        tri.right,
        FontId::monospace(size),
        ink,
    );
}

fn draw_paper(
    painter: &Painter,
    rect: Rect,
    scale: f32,
    spacing: f32,
    template: PaperTemplate,
    grid: bool,
    axes: bool,
    fade: f32,
) {
    if grid {
        let minor = style::alpha(style::grid_minor(), fade);
        let major = style::alpha(style::grid_major(), fade);
        let mut multiplier = 1.0_f32;
        while spacing * multiplier * scale < 14.0 {
            multiplier *= 2.0;
        }
        match template {
            PaperTemplate::Plain => {}
            PaperTemplate::Square | PaperTemplate::Log => {
                let step = if template == PaperTemplate::Log {
                    let mut x = spacing * multiplier;
                    let mut coords = vec![x];
                    while x < CANVAS_W as f32 * 0.5 {
                        x *= 1.25;
                        coords.push(x);
                    }
                    coords
                } else {
                    (0..)
                        .map(|i| i as f32 * spacing * multiplier)
                        .take_while(|x| *x <= CANVAS_W as f32)
                        .collect()
                };
                for (i, x) in step.iter().enumerate() {
                    if *x <= 0.0 {
                        continue;
                    }
                    let color = if i % 5 == 0 { major } else { minor };
                    let px = rect.min.x + x * scale;
                    painter.line_segment(
                        [Pos2::new(px, rect.min.y), Pos2::new(px, rect.max.y)],
                        Stroke::new(1.0_f32, color),
                    );
                }
                for (i, y) in step.iter().enumerate() {
                    if *y <= 0.0 {
                        continue;
                    }
                    let color = if i % 5 == 0 { major } else { minor };
                    let py = rect.min.y + y * scale;
                    if py <= rect.max.y {
                        painter.line_segment(
                            [Pos2::new(rect.min.x, py), Pos2::new(rect.max.x, py)],
                            Stroke::new(1.0_f32, color),
                        );
                    }
                }
            }
            PaperTemplate::Dot => {
                let step = spacing * multiplier;
                let mut i = 0;
                loop {
                    let x = i as f32 * step;
                    if x > CANVAS_W as f32 {
                        break;
                    }
                    let mut j = 0;
                    loop {
                        let y = j as f32 * step;
                        if y > CANVAS_H as f32 {
                            break;
                        }
                        let color = if i % 5 == 0 && j % 5 == 0 {
                            major
                        } else {
                            minor
                        };
                        painter.circle_filled(screen(rect, Pos2::new(x, y), scale), 1.4, color);
                        j += 1;
                    }
                    i += 1;
                }
            }
            PaperTemplate::Polar => {
                let center = rect.center();
                let max_r = rect.width().min(rect.height()) * 0.5;
                let mut i = 0;
                loop {
                    let r = i as f32 * spacing * scale;
                    if r > max_r {
                        break;
                    }
                    let color = if i % 5 == 0 { major } else { minor };
                    painter.circle_stroke(center, r, Stroke::new(1.0_f32, color));
                    i += 1;
                }
                for spoke in 0..12 {
                    let angle = spoke as f32 * std::f32::consts::TAU / 12.0;
                    let dir = Vec2::angled(angle) * max_r;
                    painter.line_segment([center - dir, center + dir], Stroke::new(1.0_f32, minor));
                }
            }
        }
    }
    if axes {
        let center = rect.center();
        let axis = style::alpha(style::axis(), fade);
        painter.line_segment(
            [
                Pos2::new(rect.min.x, center.y),
                Pos2::new(rect.max.x, center.y),
            ],
            Stroke::new(1.2_f32, axis),
        );
        painter.line_segment(
            [
                Pos2::new(center.x, rect.min.y),
                Pos2::new(center.x, rect.max.y),
            ],
            Stroke::new(1.2_f32, axis),
        );
        let label = style::alpha(style::ink_dim(), fade);
        painter.text(
            center + Vec2::new(9.0, 12.0),
            Align2::LEFT_TOP,
            "0",
            FontId::proportional(10.0),
            label,
        );
        painter.text(
            rect.max - Vec2::new(12.0, 18.0),
            Align2::RIGHT_TOP,
            "+x",
            FontId::proportional(10.0),
            label,
        );
        painter.text(
            rect.min + Vec2::new(11.0, 12.0),
            Align2::LEFT_TOP,
            "+y",
            FontId::proportional(10.0),
            label,
        );
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        style::sync(ctx);
        self.run_autosave(ctx);
        // Last-resort safety net for very small / heavily bordered windows:
        // scale the whole UI down until at least the design size fits. The
        // physical window size does not depend on the zoom, so this settles
        // after one frame instead of oscillating.
        let rect = ctx.screen_rect();
        let pixels_per_point = ctx.pixels_per_point();
        let native = ctx.native_pixels_per_point().unwrap_or(pixels_per_point);
        let physical = Vec2::new(
            rect.width() * pixels_per_point / native,
            rect.height() * pixels_per_point / native,
        );
        let target_zoom = if physical.x >= 900.0 && physical.y >= 600.0 {
            1.0
        } else {
            (physical.x / 900.0).min(physical.y / 600.0).clamp(0.5, 1.0)
        };
        if (ctx.zoom_factor() - target_zoom).abs() > 0.01 {
            ctx.set_zoom_factor(target_zoom);
        }

        if !self.layout_ready {
            self.layout_ready = true;
            // On a small/tiled window the reference library would cover the
            // whole diagram, so start with it closed.
            if ctx.screen_rect().width() < 980.0 {
                self.ref_open = false;
            }
        }
        self.handle_keys(ctx);
        self.sync_texture(ctx);

        // All panels must be added before the CentralPanel, otherwise egui
        // gives the right panel a zero-width (off-screen) rect. This is the
        // usual cause of "the ends of the GUI stick out and are not visible"
        // in tiled window managers.
        self.top_bar(ctx);
        self.status_bar(ctx);
        egui::SidePanel::left("sidebar")
            .resizable(true)
            .default_width(216.0)
            .min_width(150.0)
            .max_width(320.0)
            .frame(Frame {
                // Without an inner margin the tool buttons start at x=0 of the
                // window and the compositor border covers their left edge.
                inner_margin: Margin::symmetric(12.0, 10.0),
                fill: style::panel(),
                stroke: Stroke::new(1.0_f32, style::hairline()),
                ..Default::default()
            })
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| self.sidebar(ui));
            });
        self.reference_panel(ctx);

        egui::CentralPanel::default()
            .frame(Frame {
                fill: style::stage(),
                ..Default::default()
            })
            .show(ctx, |ui| self.canvas(ctx, ui));

        self.dialogs(ctx);
    }
}

#[cfg(test)]
mod canvas_ui_tests {
    use super::*;
    use egui::{Event, Modifiers, PointerButton, RawInput};

    fn frame(ctx: &egui::Context, app: &mut App, events: Vec<Event>) {
        let input = RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1000.0, 700.0))),
            events,
            ..Default::default()
        };
        let _ = ctx.run(input, |ctx| {
            app.handle_keys(ctx);
            app.sync_texture(ctx);
            app.top_bar(ctx);
            app.status_bar(ctx);
            egui::SidePanel::left("sidebar").show(ctx, |ui| app.sidebar(ui));
            app.reference_panel(ctx);
            egui::CentralPanel::default().show(ctx, |ui| app.canvas(ctx, ui));
            app.dialogs(ctx);
        });
    }

    fn press(p: Pos2) -> Event {
        Event::PointerButton {
            pos: p,
            button: PointerButton::Primary,
            pressed: true,
            modifiers: Modifiers::NONE,
        }
    }

    fn release(p: Pos2) -> Event {
        Event::PointerButton {
            pos: p,
            button: PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::NONE,
        }
    }

    #[test]
    fn text_editor_survives_a_held_click_and_commits_on_enter() {
        let ctx = egui::Context::default();
        let mut app = App::new();
        app.tool = Tool::Text;
        app.pending_text = Some("E=mc²".into());
        let p = Pos2::new(500.0, 350.0);
        frame(&ctx, &mut app, vec![Event::PointerMoved(p)]);
        frame(&ctx, &mut app, vec![press(p)]);
        assert!(app.text_editor.is_some(), "editor must open on press");
        // The button stays down for several frames in real usage.
        frame(&ctx, &mut app, vec![Event::PointerMoved(p)]);
        frame(&ctx, &mut app, vec![Event::PointerMoved(p)]);
        assert!(
            app.text_editor.is_some(),
            "editor must not close while the button is held"
        );
        frame(&ctx, &mut app, vec![release(p)]);
        assert!(app.text_editor.is_some(), "editor must stay after release");
        frame(&ctx, &mut app, vec![Event::Text("X".into())]);
        assert_eq!(
            app.text_editor.as_ref().unwrap().value,
            "E=mc²X",
            "typing must reach the focused TextEdit"
        );
        assert_eq!(
            app.tool,
            Tool::Text,
            "typing must not trigger tool shortcuts"
        );
        frame(
            &ctx,
            &mut app,
            vec![Event::Key {
                key: Key::Enter,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers::NONE,
            }],
        );
        assert!(app.text_editor.is_none(), "Enter must place the text");
        assert!(app.pending_text.is_none());
        assert!(app.painting.image.pixels.iter().any(|px| px.a() > 0));
    }

    #[test]
    fn rectangle_preview_follows_the_pointer_before_release() {
        let ctx = egui::Context::default();
        let mut app = App::new();
        app.tool = Tool::Rect;
        let a = Pos2::new(400.0, 250.0);
        let b = Pos2::new(560.0, 380.0);
        frame(&ctx, &mut app, vec![Event::PointerMoved(a)]);
        frame(&ctx, &mut app, vec![press(a)]);
        assert!(app.drawing);
        let start = app.shape_current;
        frame(&ctx, &mut app, vec![Event::PointerMoved(b)]);
        assert!(
            app.drawing && app.shape_current.is_some() && app.shape_current != start,
            "preview must follow the pointer while dragging"
        );
        frame(&ctx, &mut app, vec![release(b)]);
        assert!(!app.drawing);
        assert!(app.painting.image.pixels.iter().any(|px| px.a() > 0));
    }

    #[test]
    fn angle_keeps_the_first_ray_and_commits_on_the_second_drag() {
        let ctx = egui::Context::default();
        let mut app = App::new();
        app.tool = Tool::Angle;
        let vertex = Pos2::new(400.0, 250.0);
        let first = Pos2::new(500.0, 250.0);
        let second = Pos2::new(400.0, 350.0);

        frame(&ctx, &mut app, vec![Event::PointerMoved(vertex)]);
        frame(&ctx, &mut app, vec![press(vertex)]);
        frame(&ctx, &mut app, vec![Event::PointerMoved(first)]);
        frame(&ctx, &mut app, vec![release(first)]);

        assert_eq!(app.angle_step, 1, "first drag must store the first ray");
        assert!(app.angle_first.is_some());
        assert!(app.angle_vertex.is_some());
        assert!(
            app.painting.image.pixels.iter().all(|px| px.a() == 0),
            "first drag must not paint the angle yet"
        );

        frame(&ctx, &mut app, vec![Event::PointerMoved(vertex)]);
        frame(&ctx, &mut app, vec![press(vertex)]);
        frame(&ctx, &mut app, vec![Event::PointerMoved(second)]);
        frame(&ctx, &mut app, vec![release(second)]);

        assert!(!app.drawing);
        assert_eq!(app.angle_step, 0);
        assert!(app.angle_first.is_none());
        assert!(app.angle_vertex.is_none());
        assert!(
            app.painting.image.pixels.iter().any(|px| px.a() > 0),
            "second drag must commit the rays, arc and degree label"
        );
    }
}
