use eframe::egui::*;

use crate::trace::Trace;

pub(crate) struct Waterfall {
    trace: Trace,
}

impl Waterfall {
    pub(crate) fn new(trace: Trace) -> Self {
        Self { trace }
    }
}

impl crate::Panel for Waterfall {
    fn draw(&mut self, ui: &mut eframe::egui::Ui) -> Option<crate::Action> {
        // TODO: pre-calculate colors
        let colors: Vec<Color32> = vec![
            // Color32::from_rgb(0x07, 0x10, 0x13), // Rich black; ideally the BG color
            Color32::from_rgb(0x0B, 0x6E, 0x4F), // Dartmouth Green
            Color32::from_rgb(0xF2, 0x54, 0x5B), // Indian Red
            Color32::from_rgb(0x64, 0x5E, 0x9D), // Ultra Violet
            Color32::from_rgb(0x2D, 0xC2, 0xBD), // Robin Egg Blue
        ];

        // TODO: expand/collapse
        ui.heading(format!("Trace: {}", self.trace.id.clone()));

        let mut action = None;
        ScrollArea::vertical().show(ui, |ui| {
            Grid::new("trace_waterfall")
                .num_columns(3)
                .spacing((10.0, -7.0))
                .striped(true)
                .show(ui, |ui| {
                    let root = &self.trace.spans[0];
                    self.trace
                        .spans
                        .iter()
                        .map(|span| {
                            let width = span.duration_micros as f32 / root.duration_micros as f32;
                            let offset = span.offset_micros as f32 / root.duration_micros as f32;
                            let duration_ms = span.duration_micros as f32 / 1000.0;
                            (span, width, offset, duration_ms)
                        })
                        .enumerate()
                        .zip(colors.iter().cycle())
                        .for_each(|((i, (span, width, offset, duration_ms)), color)| {
                            Frame::group(&Style::default()) // with group, bar preview destroys alignment
                                .stroke(Stroke::NONE)
                                .show(ui, |ui| {
                                    ui.add(
                                        Bar::new(
                                            BarMode::Fixed,
                                            5.0,
                                            15.0 * span.level as f32,
                                            20.0,
                                            *color,
                                        )
                                        .round_radius(2.0),
                                    );
                                    if ui.link(&span.name).clicked() {
                                        action = Some(crate::Action::OpenSpanAttributes(i))
                                    }
                                });
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                ui.label(format!("{} ms", duration_ms));
                            });
                            ui.add(
                                Bar::new(BarMode::Relative, width, offset, 20.0, *color)
                                    .min_width(2.0)
                                    .round_radius(2.0),
                            );
                            ui.end_row();
                        });
                });
        });
        action
    }
}

/// Render modes for [`Bar`]
#[derive(Debug, Default, PartialEq)]
enum BarMode {
    /// Use width and offset as literal values.
    #[default]
    Fixed,
    /// Use width and offset as percentages of available space.
    Relative,
}

/// Colored bar [`egui::Widget`]
#[derive(Debug, Default)]
struct Bar {
    /// Determines how width and height should be interpreted when
    /// rendered. See [`Self::width`] and [`Self::offset`] specifics.
    mode: BarMode,

    /// Width of the rendered bar.
    ///
    /// When `mode == BarMode::Fixed`, represents exact pixel value.
    ///
    /// When `mode == BarMode::Relative`, represents a percentage of
    /// available space. (Must be in range 0.0..=1.0)
    width: f32,

    /// Lower bound for width of bar. Applies regardless of [`BarMode`].
    min_width: f32,

    /// Horizontal offset of the rendered bar.
    ///
    /// When `mode == BarMode::Fixed`, represents exact pixel value.
    ///
    /// When `mode == BarMode::Relative`, represents a percentage of
    /// available space. (Must be in range 0.0..=1.0)
    offset: f32,

    /// Height of bar in pixels.
    height: f32,

    /// Background color of bar.
    color: eframe::egui::Color32,

    /// Radius of corner rounding. Set to zero to disable rounding.
    round_radius: f32,
}

impl Widget for Bar {
    fn ui(self, ui: &mut Ui) -> Response {
        let (claimed_width, offset, rendered_width) = match self.mode {
            BarMode::Relative => {
                let claimed_width = ui.available_width();
                (
                    claimed_width,
                    claimed_width * self.offset,
                    claimed_width * self.width,
                )
            }
            BarMode::Fixed => (self.width + self.offset, self.offset, self.width),
        };

        let (mut rect, response) =
            ui.allocate_exact_size(Vec2::new(claimed_width, self.height), Sense::hover());

        rect.min.x += offset;
        rect.max.x = rect.min.x + rendered_width;

        if rect.max.x - rect.min.x < self.min_width {
            rect.max.x = rect.min.x + self.min_width;
        }

        if ui.is_rect_visible(rect) {
            ui.painter()
                .rect_filled(rect, Rounding::same(self.round_radius), self.color);
        }
        response
    }
}

impl Bar {
    fn new(mode: BarMode, width: f32, offset: f32, height: f32, color: Color32) -> Self {
        if mode == BarMode::Relative {
            // TODO: determine best way to validate/clamp width/offset in release builds without crashing
            debug_assert!(
                (0.0..=1.0).contains(&width),
                "relative width {width} was not in range [0.0, 1.0]"
            );
            debug_assert!(
                (0.0..=1.0).contains(&offset),
                "relative offset {offset} was not in range [0.0, 1.0]"
            );
        }

        Bar {
            mode,
            width,
            offset,
            height,
            color,
            ..Default::default()
        }
    }

    fn min_width(mut self, min_width: f32) -> Self {
        self.min_width = min_width;
        self
    }

    fn round_radius(mut self, radius: f32) -> Self {
        self.round_radius = radius;
        self
    }
}
