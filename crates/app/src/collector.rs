use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use eframe::egui::{self, Grid};
use lib::Trace;
use tokio::{sync::mpsc, task::JoinHandle};
use tracing::error;

use crate::Panel;
use lib::collector::run;

#[derive(Debug, Default)]
pub(crate) struct Collector {
    refresh_duration: Duration,

    host: String,
    port: String,
    task: Option<JoinHandle<Result<(), String>>>,

    /// Traces owned by [`App`]. Rebuilt when collector server ingests
    /// a new batch of spans.
    traces: Arc<Mutex<Vec<Trace>>>,
}

impl Panel for Collector {
    fn draw(&mut self, ui: &mut egui::Ui) -> Option<crate::Action> {
        ui.label("Start a background OTel collector to ingest span data over HTTP.");
        ui.label(
            "This functions identically to the standard OTel collector and \
		      simplifies ingesting data with minimal changes to instrumented \
		      application.",
        );
        ui.add_space(15.0);

        Grid::new("collector_panel_fields")
            .num_columns(2)
            .show(ui, |ui| {
                ui.label("Host");
                ui.text_edit_singleline(&mut self.host);
                ui.end_row();

                ui.label("Port");
                ui.text_edit_singleline(&mut self.port);
                ui.end_row();
            });
        ui.horizontal(|ui| {
            if self.task.is_none() && ui.button("Start").clicked() {
                if let Err(err) = self.start_collector() {
                    // TODO: certain errors might be user-actionable (e.g. unavailable port)
                    error!(err, "starting collector");
                }
            } else if self.task.is_some() && ui.button("Stop").clicked() {
                if let Err(err) = self.stop_collector() {
                    error!(err, "starting collector");
                }
            }
        });
        None
    }

    fn refresh_after(&self) -> Option<Duration> {
        self.task.as_ref().map(|_| self.refresh_duration)
    }
}

impl Collector {
    pub(crate) fn new(traces: Arc<Mutex<Vec<Trace>>>) -> Self {
        Self {
            refresh_duration: Duration::from_millis(250),
            host: "localhost".into(),
            port: "3000".into(),
            task: None,
            traces,
        }
    }

    /// Start `OTel` collector endpoint.
    fn start_collector(&mut self) -> Result<(), String> {
        use std::net::SocketAddr;

        if self.task.is_some() {
            return Err("collector already active".into());
        }

        let port = self
            .port
            .parse::<u16>()
            .map_err(|_| "port must be a valid u16".to_string())?;

        // TODO: validate host and port within form and display errors
        let host: [u8; 4] = if self.host == "localhost" {
            "127.0.0.1".to_string()
        } else {
            self.host.clone()
        }
        .split('.')
        .map(|byte| byte.parse::<u8>().map_err(|e| e.to_string()))
        .collect::<Result<Vec<u8>, String>>()?
        .try_into()
        .map_err(|_| "host must match IP format 'XXX.XXX.XXX.XXX'".to_string())?;

        let addr = SocketAddr::from((host, port));

        let (tx, rx) = mpsc::channel(1);
        self.task = Some(tokio::spawn(async move { run(tx, addr).await }));

        let traces = self.traces.clone();
        tokio::spawn(async move {
            crate::collect_spans_and_recalculate(rx, traces).await;
        });
        Ok(())
    }

    /// Stop active otel collector endpoint.
    fn stop_collector(&mut self) -> Result<(), String> {
        if let Some(task) = self.task.as_mut() {
            // TODO: is a hard-abort the best way to kill an async task?
            task.abort();
            self.task = None;
            Ok(())
        } else {
            Err("no active collector".into())
        }
    }
}
