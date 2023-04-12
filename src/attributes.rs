use eframe::{
    egui::{Grid, Layout, ScrollArea},
    emath::Align,
};

use crate::trace::Span;

pub(crate) struct Attributes {
    span: Span,
}

impl Attributes {
    pub(crate) fn new(span: Span) -> Self {
        Self { span }
    }
}

impl crate::Panel for Attributes {
    fn draw(&mut self, ui: &mut eframe::egui::Ui) -> Option<crate::Action> {
        ui.heading(&self.span.name);
        ui.separator();

        ScrollArea::vertical().show(ui, |ui| {
            if !self.span.attributes.is_empty() {
                ui.heading("Attributes");
                Grid::new("span_attributes").num_columns(2).show(ui, |ui| {
                    self.span.attributes.iter().for_each(|(key, value)| {
                        ui.label(format!("{key}:"));
                        ui.with_layout(Layout::right_to_left(Align::TOP), |ui| {
                            ui.label(if !value.is_empty() { value } else { "-" });
                        });
                        ui.end_row();
                    });
                });
                ui.add_space(10.0);
            }

            if !self.span.metadata.is_empty() {
                ui.heading("Metadata");
                Grid::new("span_metadata").num_columns(2).show(ui, |ui| {
                    self.span.metadata.iter().for_each(|(key, value)| {
                        ui.label(format!("{key}:"));
                        ui.with_layout(Layout::right_to_left(Align::TOP), |ui| {
                            ui.label(if !value.is_empty() { value } else { "-" });
                        });
                        ui.end_row();
                    });
                });
            }
        });
        None
    }
}
