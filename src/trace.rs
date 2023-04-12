use std::collections::{BTreeMap, HashMap};

#[derive(Debug, Default, Clone)]
pub(crate) struct Span {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) start: chrono::DateTime<chrono::Utc>,

    /// Microsecond relative offset from beginning of root span.
    pub(crate) offset_micros: i64,

    /// Microsecond duration of span.
    pub(crate) duration_micros: i64,

    /// Depth within [`Trace`].
    pub(crate) level: usize,

    pub(crate) trace_id: String,
    pub(crate) parent_id: Option<String>, // None == root span
    pub(crate) attributes: BTreeMap<String, String>,
    pub(crate) metadata: BTreeMap<String, String>,
}

#[derive(Debug, Clone)]
pub(crate) struct Trace {
    pub(crate) id: String,
    pub(crate) spans: Vec<Span>,

    /// Map from parent span to children
    #[allow(dead_code)]
    connections: HashMap<String, Vec<usize>>,
}

impl Trace {
    pub(crate) fn new(root: Span, descendants: Vec<Span>) -> Self {
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
