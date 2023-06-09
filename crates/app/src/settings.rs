use eframe::egui::{ComboBox, Grid, Visuals};

// TODO: add custom colors to edit appearance screen
// TODO: persist changes to appearance

/// User settings for application.
#[derive(Debug, Default)]
pub(crate) struct Settings {
    mode: Mode,
}

/// Panel to display persistent user settings.
#[derive(Debug)]
pub(crate) struct Panel<'a>(pub(crate) &'a mut Settings);

impl<'a> crate::Panel for Panel<'a> {
    fn draw(&mut self, ui: &mut eframe::egui::Ui) -> Option<crate::Action> {
        ui.label("This panel is a work-in-progress. State isn't saved across app starts.");
        ui.add_space(15.0);
        Grid::new("settings").num_columns(2).show(ui, |ui| {
            ui.label("Theme");
            ComboBox::from_id_source("settings_theme")
                .selected_text(format!("{:?}", self.0.mode))
                .show_ui(ui, |ui| {
                    if ui
                        .selectable_value(&mut self.0.mode, Mode::Dark, "Dark")
                        .changed()
                    {
                        ui.ctx().set_visuals(Visuals::dark());
                    };
                    if ui
                        .selectable_value(&mut self.0.mode, Mode::Light, "Light")
                        .changed()
                    {
                        ui.ctx().set_visuals(Visuals::light());
                    }
                    ui.selectable_value(&mut self.0.mode, Mode::System, "System");
                });
            ui.end_row();
        });
        None
    }
}

/// Theme mode for entire application. Use [`System`] to default to
/// system preference.
#[derive(Debug, Default, PartialEq)]
pub(crate) enum Mode {
    Dark,
    Light,
    #[default]
    System,
}
