use std::{
    collections::{BTreeMap, HashMap},
    io::Read,
    path::Path,
};

pub mod collector;
pub mod otel;

pub mod proto {
    include!(concat!(env!("OUT_DIR"), "/_includes.rs"));
}

use tracing::error;

use crate::proto::opentelemetry::proto::trace::v1::Span as RawSpan;

pub fn parse_file(file_path: &Path) -> Result<Vec<Span>, String> {
    let mut contents = String::new();
    std::fs::File::open(file_path)
        .and_then(|mut f| f.read_to_string(&mut contents))
        .map_err(|e| e.to_string())?;
    Ok(contents
        .lines()
        .enumerate()
        .map(|(line, contents)| {
            serde_json::from_str(contents).map_err(|e| {
                dbg!(&contents);
                format!("unable to parse line {line}: {e}", line = line + 1)
            })
        })
        .collect::<Result<Vec<otel::Span>, _>>()?
        .into_iter()
        .map(Span::from)
        .collect())
}

pub fn build_traces(spans: Vec<Span>) -> Result<Vec<Trace>, String> {
    let (roots, rest): (Vec<Span>, Vec<Span>) =
        spans.into_iter().partition(|s| s.parent_id.is_none());

    let rest: HashMap<String, Vec<Span>> = rest.into_iter().fold(HashMap::new(), |mut m, span| {
        m.entry(span.trace_id.clone()).or_default().push(span);
        m
    });

    let traces = roots
        .into_iter()
        .map(|root| {
            let descendants = rest.get(&root.trace_id).cloned().unwrap_or_default();
            Trace::new(root, descendants)
        })
        .collect();
    Ok(traces)
}

#[derive(Debug, Default, Clone)]
pub struct Span {
    pub id: String,
    pub name: String,
    pub start: chrono::DateTime<chrono::Utc>,

    /// Microsecond relative offset from beginning of root span.
    pub offset_micros: i64,

    /// Microsecond duration of span.
    pub duration_micros: i64,

    /// Depth within [`Trace`].
    pub level: usize,

    pub trace_id: String,
    pub parent_id: Option<String>, // None == root span
    pub attributes: BTreeMap<String, String>,
    pub metadata: BTreeMap<String, String>,
}

impl Span {
    pub(crate) fn new(
        raw: RawSpan,
        attributes: BTreeMap<String, String>,
        resource_attributes: BTreeMap<String, String>,
        instrument_attributes: BTreeMap<String, String>,
    ) -> Result<Self, String> {
        #[allow(clippy::cast_possible_wrap)]
        let datetime_from_nanos = |nanos: u64| {
            chrono::NaiveDateTime::from_timestamp_opt(
                (nanos / 1_000_000_000) as i64,
                (nanos % 1_000_000_000) as u32,
            )
        };
        let start = datetime_from_nanos(raw.start_time_unix_nano)
            .ok_or(format!("invalid start time {}", raw.start_time_unix_nano))?;
        let end = datetime_from_nanos(raw.end_time_unix_nano)
            .ok_or(format!("invalid end time {}", raw.end_time_unix_nano))?;

        let id = format!(
            "{:x}",
            u64::from_be_bytes(
                raw.span_id
                    .clone()
                    .try_into()
                    .map_err(|_| "span_id of 8 bytes")?
            )
        );
        let trace_id = format!(
            "{:x}",
            u128::from_be_bytes(
                raw.trace_id
                    .clone()
                    .try_into()
                    .map_err(|_| "trace_id of 16 bytes")?
            )
        );
        let parent_id = if raw.parent_span_id.is_empty() {
            None
        } else {
            Some(format!(
                "{:x}",
                u64::from_be_bytes(
                    raw.parent_span_id
                        .clone()
                        .try_into()
                        .map_err(|_| "parent_span_id of 8 bytes")?
                )
            ))
        };

        let mut metadata = resource_attributes;
        metadata.extend(instrument_attributes);

        Ok(Self {
            id,
            name: raw.name.clone(),
            start: chrono::DateTime::<chrono::Utc>::from_naive_utc_and_offset(start, chrono::Utc),
            duration_micros: end.timestamp_micros() - start.timestamp_micros(),
            trace_id,
            parent_id,
            attributes,
            metadata,
            ..Default::default()
        })
    }
}

#[derive(Debug, Clone)]
pub struct Trace {
    pub id: String,
    pub spans: Vec<Span>,

    /// Map from parent span to children
    #[allow(dead_code)]
    connections: HashMap<String, Vec<usize>>,
}

impl Trace {
    #[must_use]
    pub fn new(root: Span, descendants: Vec<Span>) -> Self {
        /// Build `Vec<Span>` in pre-order (for simpler rendering)
        fn build_tree_vec(
            id: &String,
            connections: &HashMap<String, Vec<String>>,
            spans: &HashMap<String, Span>,
            mut acc: Vec<Span>,
            level: usize,
        ) -> Vec<Span> {
            if let Some(children) = connections.get(id) {
                let mut more_spans = Vec::new();
                let mut children = children
                    .iter()
                    .filter_map(|child_id| match spans.get(child_id).cloned() {
                        Some(child) => Some(child),
                        None => {
                            error!("child {child_id} not found for parent {id}");
                            None
                        }
                    })
                    .collect::<Vec<_>>();
                children.sort_by_key(|child| child.start);

                for mut child in children {
                    let id = child.id.clone();
                    child.level = level + 1;
                    more_spans.push(child);
                    more_spans = build_tree_vec(&id, connections, spans, more_spans, level + 1);
                }
                acc.append(&mut more_spans);
            }
            acc
        }

        // NOTE: All of this can almost certainly be simplified. I
        // took what was here before and morphed it into a new
        // approach, without thinking about how I can get to the new
        // final goal more simply. This HashMap->HashMap->Vec->HashMap
        // nonsense is especially suspect.
        let descendants = descendants
            .into_iter()
            .map(|mut span| {
                span.offset_micros = (span.start - root.start)
                    .num_microseconds()
                    .unwrap_or_default();
                (span.id.clone(), span)
            })
            .collect::<HashMap<_, _>>();
        let connections: HashMap<String, Vec<String>> =
            descendants.values().fold(HashMap::new(), |mut m, span| {
                if let Some(parent_id) = span.parent_id.clone() {
                    m.entry(parent_id).or_default().push(span.id.clone());
                } else {
                    error!("attempted to access non-existent parent of {}", span.id);
                }
                m
            });

        // build in render order
        let descendants =
            build_tree_vec(&root.id, &connections, &descendants, vec![root.clone()], 0);
        // use descendant index in lookup
        let connections: HashMap<String, Vec<usize>> =
            descendants
                .iter()
                .enumerate()
                .fold(HashMap::new(), |mut acc, (i, span)| {
                    acc.entry(span.id.clone()).or_default().push(i);
                    acc
                });

        Trace {
            id: root.trace_id,
            spans: descendants,
            connections,
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn build_traces() -> Result<(), String> {
        let spans = vec![
            crate::Span {
                trace_id: "one".to_string(),
                id: "one_root".to_string(),
                ..crate::Span::default()
            },
            crate::Span {
                trace_id: "one".to_string(),
                parent_id: Some("one_root".to_string()),
                id: "one_child".to_string(),
                ..crate::Span::default()
            },
            crate::Span {
                trace_id: "two".to_string(),
                ..crate::Span::default()
            },
        ];
        let traces = super::build_traces(spans)?;
        assert_eq!(traces.len(), 2);
        assert_eq!(traces[0].id, "one".to_string());
        assert_eq!(
            traces[0]
                .spans
                .iter()
                .map(|s| s.id.clone())
                .collect::<Vec<_>>(),
            vec!["one_root".to_string(), "one_child".to_string()]
        );

        assert_eq!(traces[1].id, "two".to_string());
        Ok(())
    }
}
