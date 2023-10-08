use std::{collections::BTreeMap, net::SocketAddr, sync::Arc};

use axum::{extract::State, routing::post, Router, Server};
use axum_extra::protobuf::Protobuf;
use tokio::sync::mpsc;
use tracing::{debug, error};

use crate::proto::opentelemetry::proto::{
    collector::trace::v1::{
        ExportTracePartialSuccess, ExportTraceServiceRequest, ExportTraceServiceResponse,
    },
    common::v1::{any_value, AnyValue, KeyValue},
};

struct CollectorState {
    tx: mpsc::Sender<Vec<crate::Span>>,
}

/// # Errors
/// If the server encounters an error
pub async fn run(tx: mpsc::Sender<Vec<crate::Span>>, addr: SocketAddr) -> Result<(), String> {
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
    let scope_spans = payload
        .resource_spans
        .into_iter()
        .flat_map(|resource_span| {
            let metadata = map_attributes(&resource_span.resource.unwrap_or_default().attributes);
            resource_span
                .scope_spans
                .into_iter()
                .map(|s| (s, metadata.clone()))
                .collect::<Vec<(_, _)>>()
        });
    let raw_spans = scope_spans
        .flat_map(|(scope_span, metadata)| {
            let scope_metadata =
                map_attributes(&scope_span.scope.clone().unwrap_or_default().attributes);
            scope_span
                .spans
                .into_iter()
                .map(|span| (span, metadata.clone(), scope_metadata.clone()))
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();

    let mut spans = Vec::new();
    let mut error_message = String::new();
    let mut rejected_spans = 0i64;
    for (raw_span, res_meta, scope_meta) in raw_spans.into_iter() {
        let attributes = map_attributes(&raw_span.attributes);
        let span = crate::Span::new(raw_span, attributes, res_meta, scope_meta);
        match span {
            Ok(span) => spans.push(span),
            Err(msg) => {
                error!("{msg}");
                error_message.push_str(&format!("{msg}\n"));
                rejected_spans += 1;
            }
        }
    }

    _ = state.tx.send(spans).await;

    let response = ExportTraceServiceResponse {
        partial_success: Some(ExportTracePartialSuccess {
            rejected_spans,
            error_message,
        }),
    };
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
                    value.as_ref().map_or("null".into(), any_value_to_string)
                ))
                .collect::<Vec<String>>()
                .join(", ")
        ),
        Some(any_value::Value::BytesValue(val)) => {
            format!(
                "[{}]",
                val.iter()
                    .map(std::string::ToString::to_string)
                    .collect::<Vec<String>>()
                    .join(", ")
            )
        }
        _ => String::new(),
    }
}

#[inline]
fn map_attributes(attributes: &[KeyValue]) -> BTreeMap<String, String> {
    attributes
        .iter()
        .map(|KeyValue { key, value }| {
            let value = value.as_ref().map(any_value_to_string).unwrap_or_default();
            (key.clone(), value)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    mod export_trace {
        use crate::proto::opentelemetry::proto::{
            common::v1::InstrumentationScope,
            resource::v1::Resource,
            trace::v1::{ResourceSpans, ScopeSpans, Span},
        };

        use super::super::*;
        use tokio;

        #[tokio::test]
        async fn empty_request() {
            let (tx, _rx) = mpsc::channel(1);
            let state = Arc::new(CollectorState { tx });
            let payload = ExportTraceServiceRequest {
                resource_spans: vec![],
            };
            let Protobuf(res) = export_trace(State(state), Protobuf(payload)).await;
            let success = res.partial_success.unwrap_or_default();
            assert_eq!(success.rejected_spans, 0);
        }

        #[tokio::test]
        async fn single_span_no_metadata() -> Result<(), String> {
            let (tx, mut rx) = mpsc::channel(1);
            let state = Arc::new(CollectorState { tx });
            let payload = ExportTraceServiceRequest {
                resource_spans: vec![ResourceSpans {
                    scope_spans: vec![ScopeSpans {
                        spans: vec![Span {
                            trace_id: [0; 16].to_vec(),
                            span_id: [0; 8].to_vec(),
                            name: "Test".to_string(),
                            start_time_unix_nano: 0,
                            end_time_unix_nano: 1_000_000,
                            ..Span::default()
                        }],
                        ..ScopeSpans::default()
                    }],
                    ..ResourceSpans::default()
                }],
            };
            let Protobuf(res) = export_trace(State(state), Protobuf(payload)).await;
            let success = res.partial_success.unwrap_or_default();
            assert_eq!(success.rejected_spans, 0);

            let spans = rx.try_recv().map_err(|_| "span not available on channel")?;
            assert_eq!(spans.len(), 1);
            assert_eq!(&spans[0].name, "Test");

            Ok(())
        }

        #[tokio::test]
        async fn single_span_with_metadata() -> Result<(), String> {
            let (tx, mut rx) = mpsc::channel(1);
            let state = Arc::new(CollectorState { tx });
            let payload = ExportTraceServiceRequest {
                resource_spans: vec![ResourceSpans {
                    resource: Some(Resource {
                        attributes: vec![KeyValue {
                            key: "library".to_string(),
                            value: Some(AnyValue {
                                value: Some(any_value::Value::StringValue(
                                    "egui-trace".to_string(),
                                )),
                            }),
                        }],
                        ..Resource::default()
                    }),
                    scope_spans: vec![ScopeSpans {
                        scope: Some(InstrumentationScope {
                            name: "collector".to_string(),
                            version: "v0.0.1".to_string(),
                            attributes: vec![KeyValue {
                                key: "method".to_string(),
                                value: Some(AnyValue {
                                    value: Some(any_value::Value::StringValue(
                                        "generated".to_string(),
                                    )),
                                }),
                            }],
                            ..InstrumentationScope::default()
                        }),
                        spans: vec![Span {
                            trace_id: [0; 16].to_vec(),
                            span_id: [0; 8].to_vec(),
                            name: "Test".to_string(),
                            start_time_unix_nano: 0,
                            end_time_unix_nano: 1_000_000,
                            attributes: vec![KeyValue {
                                key: "cache.hit".to_string(),
                                value: Some(AnyValue {
                                    value: Some(any_value::Value::BoolValue(true)),
                                }),
                            }],
                            ..Span::default()
                        }],
                        ..ScopeSpans::default()
                    }],
                    ..ResourceSpans::default()
                }],
            };
            let Protobuf(res) = export_trace(State(state), Protobuf(payload)).await;
            let success = res.partial_success.unwrap_or_default();
            assert_eq!(success.rejected_spans, 0);

            let spans = rx.try_recv().map_err(|_| "span not available on channel")?;
            assert_eq!(spans.len(), 1);

            let span = &spans[0];
            assert_eq!(&span.name, "Test");
            assert_eq!(span.metadata.len(), 2);
            assert_eq!(
                span.metadata
                    .get("library")
                    .ok_or("resource attribute not in metadata")?,
                &"egui-trace".to_string()
            );
            assert_eq!(
                span.metadata
                    .get("method")
                    .ok_or("instrumentation scope attribute not in metadata")?,
                &"generated".to_string()
            );
            assert_eq!(span.attributes.len(), 1);
            assert_eq!(
                span.attributes
                    .get("cache.hit")
                    .ok_or("missing span attribute")?,
                &"true".to_string()
            );

            Ok(())
        }
    }
}
