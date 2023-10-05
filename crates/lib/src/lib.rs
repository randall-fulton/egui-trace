use std::{collections::{BTreeMap, HashMap}, path::Path, io::Read};

pub mod collector;
pub mod otel;

pub mod proto {
    include!(concat!(env!("OUT_DIR"), "/_includes.rs"));
}

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

    let rest = rest.into_iter().fold(HashMap::new(), |mut m, span| {
        m.entry(span.trace_id.clone())
            .or_insert_with(Vec::new)
            .push(span);
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

#[derive(Debug, Clone)]
pub struct Trace {
    pub id: String,
    pub spans: Vec<Span>,

    /// Map from parent span to children
    #[allow(dead_code)]
    connections: HashMap<String, Vec<usize>>,
}

impl Trace {
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
                    .map(|child_id| spans.get(child_id).cloned().expect("id to exist in spans"))
                    .collect::<Vec<_>>();
                children.sort_by_key(|child| child.start);
                for mut child in children.into_iter() {
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
        let connections = descendants.values().fold(HashMap::new(), |mut m, span| {
            m.entry(span.parent_id.clone().unwrap())
                .or_insert_with(Vec::new)
                .push(span.id.clone());
            m
        });

        // build in render order
        let descendants =
            build_tree_vec(&root.id, &connections, &descendants, vec![root.clone()], 0);
        // use descendant index in lookup
        let connections =
            descendants
                .iter()
                .enumerate()
                .fold(HashMap::new(), |mut acc, (i, span)| {
                    acc.entry(span.id.clone()).or_insert_with(Vec::new).push(i);
                    acc
                });

        Trace {
            id: root.trace_id,
            spans: descendants,
            connections,
        }
    }
}
