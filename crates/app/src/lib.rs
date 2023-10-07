mod attributes;
pub mod collector;
pub mod list;
pub mod settings;
pub mod waterfall;

use egui_dock::Tree;
use lib::{build_traces, parse_file, Span, Trace};
use tokio::sync::mpsc;

use std::{
    path::Path,
    sync::{Arc, Mutex},
    time::Duration,
};

use eframe::egui::{self, menu, InputState, Layout};

use tracing::error;

/// Floating window that can be collapsed or dismissed.
trait Panel {
    /// Draw contents of [`Panel`]. Surrounding
    /// [`egui::containers::Window`] will be drawn before calling this
    /// function.
    fn draw(&mut self, ui: &mut egui::Ui) -> Option<Action>;

    /// Request a repaint after the returned [`Duration`]. The
    /// shortest duration requested from the set of all active panels
    /// will be used.
    fn refresh_after(&self) -> Option<Duration> {
        None
    }
}

#[derive(Debug)]
enum Action {
    /// Open attributes tab for [`crate::Span`] at index. Parent
    /// [`crate::Trace`] is implied by context.
    OpenSpanAttributes(usize),
    /// Open trace details tab for [`crate::Trace`] at index.
    OpenTraceDetails(usize),
}

#[derive(Debug, Clone)]
enum Tab {
    Appearance,
    Collector,
    SpanAttributes(usize, usize),
    TraceDetails(usize),
    TraceList,
}

impl PartialEq for Tab {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            // only allow a single attributes panel to be open
            (Self::SpanAttributes(_, _), Self::SpanAttributes(_, _)) => true,
            (Self::TraceDetails(l0), Self::TraceDetails(r0)) => l0 == r0,
            _ => core::mem::discriminant(self) == core::mem::discriminant(other),
        }
    }
}

struct TabViewer {
    settings: settings::Settings,
    traces: Arc<Mutex<Vec<Trace>>>,

    collector: collector::Collector,
    list: list::TraceList,

    /// [`Tab`]s to be added/updated after previous frame.
    pub(crate) last_frame_tabs: Vec<Tab>,
}

impl TabViewer {
    fn new(traces: Arc<Mutex<Vec<Trace>>>) -> Self {
        Self {
            settings: crate::settings::Settings::default(),
            traces: traces.clone(),
            collector: collector::Collector::new(traces.clone()),
            list: list::TraceList::new(traces),
            last_frame_tabs: Vec::new(),
        }
    }
}

impl egui_dock::TabViewer for TabViewer {
    type Tab = Tab;

    fn ui(&mut self, ui: &mut egui::Ui, tab: &mut Self::Tab) {
        let (trace_idx, action) = match tab {
            Tab::Appearance => (None, settings::Panel(&mut self.settings).draw(ui)),
            Tab::Collector => (None, self.collector.draw(ui)),
            Tab::SpanAttributes(trace_idx, span_idx) => {
                if let Some(trace) = self.traces.lock().unwrap().get(*trace_idx).cloned() {
                    let span = trace.spans[*span_idx].clone();
                    (Some(*trace_idx), attributes::Attributes::new(span).draw(ui))
                } else {
                    (None, None)
                }
            }
            Tab::TraceList => (None, self.list.draw(ui)),
            Tab::TraceDetails(idx) => {
                if let Some(trace) = self.traces.lock().unwrap().get(*idx).cloned() {
                    (Some(*idx), waterfall::Waterfall::new(trace).draw(ui))
                } else {
                    (None, None)
                }
            }
        };
        if let Some(action) = action {
            let tab = match action {
                Action::OpenSpanAttributes(span_idx) => {
                    if let Some(trace_idx) = trace_idx {
                        Some(Tab::SpanAttributes(trace_idx, span_idx))
                    } else {
                        error!("attempt to open span without trace index");
                        None
                    }
                }
                Action::OpenTraceDetails(trace_idx) => Some(Tab::TraceDetails(trace_idx)),
            };

            if let Some(tab) = tab {
                self.last_frame_tabs.push(tab);
            }
        }
    }

    fn title(&mut self, tab: &mut Self::Tab) -> egui::WidgetText {
        let title: String = match tab {
            Tab::Appearance => "Appearance".into(),
            Tab::Collector => "Collector".into(),
            Tab::SpanAttributes(trace_idx, span_idx) => format!(
                "Span: {}",
                self.traces
                    .lock()
                    .unwrap()
                    .get(*trace_idx)
                    .and_then(|trace| trace.spans.get(*span_idx))
                    .map_or("<unknown>".to_string(), |span| span.id.clone())
            ),
            Tab::TraceList => "Traces".into(),
            Tab::TraceDetails(idx) => format!(
                "Trace: {}",
                self.traces
                    .lock()
                    .unwrap()
                    .get(*idx)
                    .map_or("<unknown>".to_string(), |trace| trace.id.clone())
            ),
        };
        title.into()
    }
}

pub struct App {
    /// User-actionable error message from most recent operation.
    error: Option<String>, // TODO: display this to users
    traces: Arc<Mutex<Vec<Trace>>>,

    viewer: TabViewer,
    tree: Tree<Tab>,
}

impl Default for App {
    fn default() -> Self {
        let traces: Arc<Mutex<Vec<Trace>>> = Arc::default();
        Self {
            error: Option::default(),
            traces: traces.clone(),
            viewer: TabViewer::new(traces),
            tree: Tree::default(),
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &eframe::egui::Context, frame: &mut eframe::Frame) {
        self.menu_bar(ctx, frame);

        egui::CentralPanel::default().show(ctx, |ui| {
            if self.tree.is_empty() {
                App::landing(ctx, frame);

                if !self.traces.lock().unwrap().is_empty() {
                    self.tree.push_to_focused_leaf(Tab::TraceList);
                }
            } else {
                let style = egui_dock::Style::from_egui(ui.style().as_ref());
                egui_dock::DockArea::new(&mut self.tree)
                    .style(style)
                    .show_inside(ui, &mut self.viewer);
                self.viewer
                    .last_frame_tabs
                    .drain(0..self.viewer.last_frame_tabs.len())
                    .collect::<Vec<Tab>>()
                    .into_iter()
                    .for_each(|tab| self.add_tab(tab));
            }
        });

        ctx.input(|i| {
            self.handle_input(i);
        });
    }
}

impl App {
    fn menu_bar(&mut self, ctx: &eframe::egui::Context, frame: &mut eframe::Frame) {
        egui::TopBottomPanel::top("menu_bar").show(ctx, |ui| {
            menu::bar(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("Open").clicked() {
                        ui.close_menu();
                        self.error = self.pick_file().map_err(String::from).err();
                    }
                    if ui.button("Exit").clicked() {
                        frame.close();
                    }
                });

                ui.menu_button("Edit", |ui| {
                    if ui.button("Appearance").clicked() {
                        ui.close_menu();
                        self.add_tab(Tab::Appearance);
                    }
                });

                ui.menu_button("View", |ui| {
                    if ui.button("Collector").clicked() {
                        ui.close_menu();
                        self.add_tab(Tab::Collector);
                    }
                    if ui.button("Traces").clicked() {
                        ui.close_menu();
                        self.add_tab(Tab::TraceList);
                    }
                });
            });
        });
    }

    fn landing(ctx: &eframe::egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.with_layout(
                Layout::centered_and_justified(egui::Direction::TopDown),
                |ui| {
                    ui.heading("Open a trace file:\nFile > Open\nCtrl + O\nDrag and drop");
                },
            );
        });
    }
}

impl App {
    /// Add [`Tab`] to the active [`egui_dock::Tree`]. Depending on
    /// provided tab, method of opening will vary. For example,
    /// [`Tab::SpanAttributes`] is always opened in a right-split.
    fn add_tab(&mut self, tab: Tab) {
        match tab {
            Tab::SpanAttributes(trace_idx, span_idx) => {
                if let Some((node_idx, tab_idx)) = self.tree.find_tab(&tab) {
                    self.tree.set_focused_node(node_idx);
                    self.tree.set_active_tab(node_idx, tab_idx);
                    if let Some((
                        _rect,
                        Tab::SpanAttributes(existing_trace_idx, existing_span_idx),
                    )) = self.tree.find_active_focused()
                    {
                        *existing_trace_idx = trace_idx;
                        *existing_span_idx = span_idx;
                    } else {
                        error!("found span attributes tab that can't be destructured");
                    }
                } else if let Some((active_node_idx, _)) = self
                    .tree
                    .find_active_focused()
                    .map(|(_, tab)| tab)
                    .cloned()
                    .and_then(|active_tab| self.tree.find_tab(&active_tab))
                {
                    self.tree.split_right(active_node_idx, 0.8, vec![tab]);
                } else {
                    error!("attempted to open span attributes without a focused node");
                }
            }
            _ => {
                if let Some((node_idx, tab_idx)) = self.tree.find_tab(&tab) {
                    self.tree.set_focused_node(node_idx);
                    self.tree.set_active_tab(node_idx, tab_idx);
                } else {
                    self.tree.push_to_focused_leaf(tab);
                }
            }
        }
    }

    /// Process user actions. User-actionable errors are set in [`Self::error`].
    fn handle_input(&mut self, i: &InputState) {
        if i.key_down(egui::Key::O) && i.modifiers.ctrl {
            self.error = self.pick_file().map_err(String::from).err();
        }

        let dropped = &i.raw.dropped_files;
        if !dropped.is_empty() {
            for file in dropped {
                if let Some(file_path) = &file.path {
                    self.error = self
                        .load_traces_from_file(file_path)
                        .map_err(String::from)
                        .err();
                    if self.error.is_some() {
                        break;
                    }
                }
            }
        }
    }

    fn load_traces_from_file(&mut self, file_path: &Path) -> Result<(), String> {
        let mut parsed_traces = parse_file(file_path).and_then(build_traces)?;
        let mut traces = self.traces.lock().unwrap();
        traces.append(&mut parsed_traces);

        Ok(())
    }

    fn pick_file(&mut self) -> Result<(), String> {
        if let Some(file_path) = rfd::FileDialog::new()
            // .set_directory(DEFAULT_DIRECTORY)
            .pick_file()
        {
            self.load_traces_from_file(&file_path)?;
        }
        Ok(())
    }
}

/// Recalculate `traces` whenever new message arrives on `rx`. Only
/// traces that were updated in the message _should_ be recalculated
/// (not true right now).
async fn collect_spans_and_recalculate(
    mut rx: mpsc::Receiver<Vec<Span>>,
    traces: Arc<Mutex<Vec<Trace>>>,
) {
    while let Some(mut spans) = rx.recv().await {
        let mut traces = traces.lock().unwrap();
        let mut all_spans = traces
            .iter()
            .flat_map(|trace| trace.spans.clone())
            .collect::<Vec<_>>();
        all_spans.append(&mut spans);

        let res = build_traces(all_spans);
        match res {
            Ok(res) => (*traces) = res,
            Err(msg) => error!("rebuilding traces on collector ingestions: {msg}"),
        }
    }
}
