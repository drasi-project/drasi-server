use anyhow::{ensure, Context, Result};
use drasi_core::models::SourceChange;
use drasi_lib::computation::v1::{
    ChangeOperation, EnvelopeCodec, GraphChangeCodec, QueryChangeCodec,
};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    fs::File,
    io::{BufRead, BufReader, BufWriter, Write},
    num::NonZeroUsize,
};

fn main() -> Result<()> {
    let path = std::env::args()
        .nth(1)
        .context("usage: inspect-trace <recording.ndjson>")?;
    let mut codec = EnvelopeCodec::new(NonZeroUsize::new(8 * 1024 * 1024).unwrap());
    codec.register_schema(GraphChangeCodec::schema())?;
    codec.register_schema(QueryChangeCodec::schema())?;
    let mut output = BufWriter::new(std::io::stdout().lock());
    let mut sequences = BTreeMap::new();
    for (index, line) in BufReader::new(File::open(path)?).lines().enumerate() {
        let record: Value = serde_json::from_str(&line?)?;
        let envelope = codec.decode(&serde_json::to_vec(&record["frame"])?)?;
        let stream = envelope.system().stream().as_str().to_owned();
        let sequence = envelope.system().sequence();
        ensure!(
            sequences
                .get(&stream)
                .is_none_or(|previous| sequence > *previous),
            "stream sequence did not advance at line {}",
            index + 1
        );
        sequences.insert(stream.clone(), sequence);
        let mut changes = Vec::new();
        if record["port"] == "graph" {
            for change in GraphChangeCodec::decode_changes(&envelope)? {
                changes.push(match change {
                    SourceChange::Insert { element } | SourceChange::Update { element } => json!({
                        "metadata":element.get_metadata(),
                        "properties":Value::Object(element.get_properties().into()),
                    }),
                    SourceChange::Delete { metadata } => json!({"deleted":metadata}),
                    SourceChange::Future { .. } => {
                        anyhow::bail!("unexpected native future emission")
                    }
                });
            }
        } else {
            ensure!(record["port"] == "query", "unexpected diagnostic port");
            for operation in envelope.changes().operations() {
                let (row, deleted) = match operation {
                    ChangeOperation::Added { after, .. }
                    | ChangeOperation::Updated { after, .. } => (after, false),
                    ChangeOperation::Deleted {
                        before: Some(before),
                        ..
                    } => (before, true),
                    ChangeOperation::Deleted { identity, .. } => {
                        changes.push(json!({"deleted_identity":format!("{identity:?}")}));
                        continue;
                    }
                };
                let row = QueryChangeCodec::decode_row(row)?;
                changes.push(
                    json!({"query":row.query_id,"signature":row.signature.to_string(),
                    "deleted":deleted,"values":QueryChangeCodec::row_values_to_json(&row.values)}),
                );
            }
        }
        serde_json::to_writer(
            &mut output,
            &json!({
                "line":index+1,"observed_at_ms":record["observed_at_ms"],"stream":stream,
                "sequence":sequence.to_string(),"lineage":record["frame"]["lineage"],"changes":changes,
            }),
        )?;
        writeln!(&mut output)?;
    }
    output.flush()?;
    Ok(())
}
