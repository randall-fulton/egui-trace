use std::{collections::BTreeMap, net::SocketAddr, sync::Arc};

use axum::{extract::State, routing::post, Router, Server};
use axum_extra::protobuf::Protobuf;
use tokio::sync::mpsc;
use tracing::debug;

use crate::proto::opentelemetry::proto::{
    collector::trace::v1::{ExportTraceServiceRequest, ExportTraceServiceResponse},
    common::v1::{any_value, AnyValue, KeyValue},
};

struct CollectorState {
    tx: mpsc::Sender<Vec<crate::Span>>,
}

pub(crate) async fn run(
    tx: mpsc::Sender<Vec<crate::Span>>,
    addr: SocketAddr,
) -> Result<(), String> {
    let app = Router::new()
        .route("/v1/traces", post(export_trace))
        .with_state(Arc::new(CollectorState { tx }));

    debug!("listening on {addr}");

    Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .map_err(|e| e.to_string())
}

async fn export_trace(
    State(state): State<Arc<CollectorState>>,
    Protobuf(payload): Protobuf<ExportTraceServiceRequest>,
) -> Protobuf<ExportTraceServiceResponse> {
    // TODO: add more to metadata
    let spans = payload
        .resource_spans
        .into_iter()
        .flat_map(|resource_span| {
            let metadata = map_attributes(resource_span.resource.unwrap_or_default().attributes);
            resource_span
                .scope_spans
                .into_iter()
                .map(|s| (s, metadata.clone()))
                .collect::<Vec<(_, _)>>()
        })
        .flat_map(|(scope_span, mut metadata)| {
            let mut scope_metadata =
                map_attributes(scope_span.scope.clone().unwrap_or_default().attributes);
            metadata.append(&mut scope_metadata);
            scope_span
                .spans
                .into_iter()
                .map(|span| (span, metadata.clone()))
                .collect::<Vec<_>>()
        })
        .map(|(raw, metadata)| {
            let start = chrono::NaiveDateTime::from_timestamp_opt(
                (raw.start_time_unix_nano / 1_000_000_000) as i64,
                (raw.start_time_unix_nano % 1_000_000_000) as u32,
            )
            .expect("valid unix nano start time");
            let end = chrono::NaiveDateTime::from_timestamp_opt(
                (raw.end_time_unix_nano / 1_000_000_000) as i64,
                (raw.end_time_unix_nano % 1_000_000_000) as u32,
            )
            .expect("valid unix nano end time");
            crate::Span {
                id: format!(
                    "{:x}",
                    u64::from_be_bytes(raw.span_id.clone().try_into().expect("span_id of 8 bytes"),)
                ),
                name: raw.name.clone(),
                start: chrono::DateTime::<chrono::Utc>::from_utc(start, chrono::Utc),
                duration_micros: end.timestamp_micros() - start.timestamp_micros(),
                trace_id: format!(
                    "{:x}",
                    u128::from_be_bytes(
                        raw.trace_id
                            .clone()
                            .try_into()
                            .expect("trace_id of 16 bytes"),
                    )
                ),
                parent_id: if !raw.parent_span_id.is_empty() {
                    Some(format!(
                        "{:x}",
                        u64::from_be_bytes(
                            raw.parent_span_id
                                .clone()
                                .try_into()
                                .expect("parent_span_id of 8 bytes"),
                        )
                    ))
                } else {
                    None
                },
                attributes: map_attributes(raw.attributes),
                metadata,
                ..Default::default()
            }
        })
        .collect::<Vec<crate::Span>>();
    _ = state.tx.send(spans).await;

    let response = ExportTraceServiceResponse::default();
    Protobuf(response)
}

fn any_value_to_string(av: &AnyValue) -> String {
    match &av.value {
        Some(any_value::Value::StringValue(val)) => val.clone(),
        Some(any_value::Value::BoolValue(val)) => format!("{val}"),
        Some(any_value::Value::IntValue(val)) => format!("{val}"),
        Some(any_value::Value::DoubleValue(val)) => format!("{val}"),
        Some(any_value::Value::ArrayValue(val)) => {
            format!(
                "[{}]",
                val.values
                    .iter()
                    .map(any_value_to_string)
                    .collect::<Vec<String>>()
                    .join(", ")
            )
        }
        Some(any_value::Value::KvlistValue(val)) => format!(
            "{{{}}}",
            val.values
                .iter()
                .map(|KeyValue { key, value }| format!(
                    "{key}: {}",
                    value
                        .as_ref()
                        .map(any_value_to_string)
                        .unwrap_or("null".into())
                ))
                .collect::<Vec<String>>()
                .join(", ")
        ),
        Some(any_value::Value::BytesValue(val)) => {
            format!(
                "[{}]",
                val.iter()
                    .map(|v| v.to_string())
                    .collect::<Vec<String>>()
                    .join(", ")
            )
        }
        _ => "".into(),
    }
}

#[inline]
fn map_attributes(attributes: Vec<KeyValue>) -> BTreeMap<String, String> {
    attributes
        .iter()
        .map(|KeyValue { key, value }| {
            let value = value.as_ref().map(any_value_to_string).unwrap_or_default();
            (key.clone(), value)
        })
        .collect()
}
