//! OpenTelemetry-specific span import/export logic.

use std::collections::BTreeMap;

use serde::Deserialize;

/// Span as represented in tracing stream
#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub(crate) struct Span {
    pub name: String,
    #[serde(rename = "SpanContext")]
    pub context: SpanContext,
    pub parent: SpanContext,
    #[serde(rename = "StartTime")]
    pub start: chrono::DateTime<chrono::Utc>,
    #[serde(rename = "EndTime")]
    pub end: chrono::DateTime<chrono::Utc>,

    attributes: Option<Vec<SpanAttribute>>,
    status: Status,
    resource: Vec<Resource>,
    #[serde(rename = "InstrumentationLibrary")]
    library: Library,
}

impl From<Span> for crate::Span {
    fn from(value: Span) -> Self {
        let parent_id = if value.is_root() {
            None
        } else {
            Some(value.parent.span_id)
        };
        let attributes: BTreeMap<_, _> = value
            .attributes
            .map(|attributes| {
                attributes
                    .into_iter()
                    .map(|SpanAttribute { key, value }| match value {
                        SpanAttributeValue::Int64 { value } => (key, value.to_string()),
                    })
                    .collect()
            })
            .unwrap_or_default();
        let mut metadata: BTreeMap<_, _> = value
            .resource
            .into_iter()
            .map(|Resource { key, value }| match value {
                ResourceValue::String { value } => (key, value),
            })
            .collect();
        metadata.insert(
            "status.code".into(),
            if value.status.code == "Unset" {
                "-".into()
            } else {
                value.status.code
            },
        );
        metadata.insert("status.description".into(), value.status.description);
        metadata.insert("library.name".into(), value.library.name);
        metadata.insert("library.version".into(), value.library.version);
        metadata.insert("library.schema_url".into(), value.library.schema_url);
        Self {
            id: value.context.span_id.clone(),
            name: value.name.clone(),
            start: value.start,
            duration_micros: (value.end - value.start)
                .num_microseconds()
                .unwrap_or_default(),
            trace_id: value.context.trace_id,
            parent_id,
            attributes,
            metadata,
            ..Default::default()
        }
    }
}

impl Span {
    /// Is current `RawSpan` the root of a trace
    pub fn is_root(&self) -> bool {
        self.parent.trace_id.chars().all(|c| c == '0')
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Status {
    code: String,
    description: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Resource {
    key: String,
    value: ResourceValue,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "UPPERCASE", tag = "Type")]
enum ResourceValue {
    #[serde(rename_all = "PascalCase")]
    String { value: String },
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Library {
    name: String,
    version: String,
    #[serde(rename = "SchemaURL")]
    schema_url: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub(crate) struct SpanContext {
    #[serde(rename = "TraceID")]
    pub trace_id: String,
    #[serde(rename = "SpanID")]
    pub span_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct SpanAttribute {
    key: String,
    value: SpanAttributeValue,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "UPPERCASE", tag = "Type")]
enum SpanAttributeValue {
    #[serde(rename_all = "PascalCase")]
    Int64 { value: i64 },
}
