use std::sync::{Arc, Mutex};

use eframe::egui::Grid;
use egui_extras::{Column as EguiColumn, TableBuilder};
use lib::Trace;

#[derive(Debug, Default, PartialEq)]
enum Column {
    Id,
    Name,
    Duration,
    #[default]
    Start,
}

#[derive(Debug, Default, PartialEq)]
enum Direction {
    #[default]
    Ascending,
    Descending,
}

#[derive(Debug, Default)]
pub(crate) struct State {
    search: String,
    sort_column: Column,
    sort_direction: Direction,
}

pub(crate) struct TraceList {
    state: State,
    traces: Arc<Mutex<Vec<Trace>>>,
}

impl TraceList {
    pub(crate) fn new(traces: Arc<Mutex<Vec<Trace>>>) -> Self {
        Self {
            state: Default::default(),
            traces,
        }
    }
}

impl crate::Panel for TraceList {
    fn draw(&mut self, ui: &mut eframe::egui::Ui) -> Option<crate::Action> {
        // index in original slice is kept to use ensure the correct trace
        // is reported as selected, since caller doesn't know we are
        // filtering
        let traces = self.traces.lock().unwrap();
        let mut visible_traces = traces
            .iter()
            .enumerate()
            .filter(|(_, trace)| {
                let search = self.state.search.as_str();
                trace.spans[0].name.starts_with(search) || trace.id.starts_with(search)
            })
            .collect::<Vec<(usize, &Trace)>>();
        match self.state.sort_column {
            Column::Id => visible_traces.sort_by_key(|(_, trace)| &trace.id),
            Column::Name => visible_traces.sort_by_key(|(_, trace)| &trace.spans[0].name),
            Column::Duration => {
                visible_traces.sort_by_key(|(_, trace)| trace.spans[0].duration_micros)
            }
            Column::Start => visible_traces.sort_by_key(|(_, trace)| trace.spans[0].start),
        }
        if self.state.sort_direction == Direction::Descending {
            visible_traces.reverse();
        }

        ui.collapsing("Filters", |ui| {
            Grid::new("list_filters").num_columns(2).show(ui, |ui| {
                ui.label("Search");
                ui.text_edit_singleline(&mut self.state.search);
                ui.end_row();

                ui.label("Sort");
                ui.horizontal(|ui| {
                    ui.radio_value(&mut self.state.sort_column, Column::Id, "Trace ID");
                    ui.radio_value(&mut self.state.sort_column, Column::Name, "Name");
                    ui.radio_value(&mut self.state.sort_column, Column::Duration, "Duration");
                    ui.radio_value(&mut self.state.sort_column, Column::Start, "Start Time");
                });
                ui.end_row();

                ui.label("");
                ui.horizontal(|ui| {
                    ui.radio_value(&mut self.state.sort_direction, Direction::Ascending, "asc");
                    ui.radio_value(
                        &mut self.state.sort_direction,
                        Direction::Descending,
                        "desc",
                    );
                });
                ui.end_row();
            });
        });
        ui.add_space(5.0);

        let mut action = None;
        TableBuilder::new(ui)
            .column(EguiColumn::auto().at_least(250.0))
            .column(EguiColumn::auto().at_least(150.0))
            .column(EguiColumn::auto().at_least(100.0))
            .column(EguiColumn::remainder())
            .striped(true)
            .header(20.0, |mut header| {
                header.col(|ui| {
                    ui.heading("Trace ID");
                });
                header.col(|ui| {
                    ui.heading("Name");
                });
                header.col(|ui| {
                    ui.heading("Duration");
                });
                header.col(|ui| {
                    ui.heading("Start");
                });
            })
            .body(|mut body| {
                for (i, trace) in &visible_traces {
                    body.row(20.0, |mut row| {
                        row.col(|ui| {
                            if ui.link(&trace.id).clicked() {
                                action = Some(crate::Action::OpenTraceDetails(*i));
                            }
                        });
                        row.col(|ui| {
                            ui.label(&trace.spans[0].name);
                        });
                        row.col(|ui| {
                            ui.label(format!("{}ms", trace.spans[0].duration_micros / 1000));
                        });
                        row.col(|ui| {
                            ui.label(format!(
                                "{}",
                                trace.spans[0].start.format("%b %e, %H:%M:%S%.3f")
                            ));
                        });
                    });
                }
            });
        action
    }
}
