//! Opt-in synthetic-fixture probe. Not human typing, a low-power laptop, or a
//! release latency certificate. Refuses an existing directory; no manuscripts.
use grimoire_core::{
    accounting::{Ledger, Mode, Moment, Role, Source},
    editor::Editor,
    measure::{self, Counts, Mutation, Origin},
};
use serde_json::json;
use std::{fs, path::PathBuf, time::Instant};
fn main() -> anyhow::Result<()> {
    let root = PathBuf::from(
        std::env::args()
            .nth(1)
            .expect("supply a NEW disposable probe directory"),
    );
    fs::create_dir(&root)?;
    let result = (|| -> anyhow::Result<serde_json::Value> {
        let mut cases = vec![];
        for bytes in [10_000, 50_000, 200_000, 260_000, 300_000] {
            let text = "word ".repeat(bytes / 5);
            let mut samples = vec![];
            let mut gaps = 0;
            for _ in 0..30 {
                let t = Instant::now();
                let c = measure::replacement(&text, text.len() - 1..text.len() - 1, "x");
                samples.push(t.elapsed().as_micros() as u64);
                gaps += c.gaps;
            }
            samples.sort_unstable();
            cases.push(json!({"source_bytes":bytes,"samples":30,"median_us":samples[15],"p95_us":samples[28],"word_gaps":gaps}));
        }
        let mut e = Editor::from_text(&"word ".repeat(10000));
        e.measure_operations(true);
        e.scene_end();
        let start = Instant::now();
        for _ in 0..30 {
            e.insert('x');
            e.take_mutations();
        }
        let editor_us = start.elapsed().as_micros() as u64;
        let mut ledger = Ledger::load(&root);
        let now = Moment {
            mono_ms: 0,
            wall: chrono::DateTime::parse_from_rfc3339("2026-10-25T12:00:00+00:00")?,
            zone: Some("UTC".into()),
        };
        for i in 0..1024 {
            ledger.observe(
                &now,
                Source {
                    path: format!("manuscript/synthetic-{i}.md").into(),
                    role: Role::Manuscript,
                },
                Mode::Drafting,
                Mutation {
                    origin: Origin::Typing,
                    counts: Counts {
                        added: 1,
                        inserted_chars: 4,
                        operations: 1,
                        ..Counts::default()
                    },
                },
                vec![],
            )?;
        }
        let t = Instant::now();
        ledger.flush()?;
        let flush_us = t.elapsed().as_micros() as u64;
        let t = Instant::now();
        let loaded = Ledger::load(&root);
        let load_us = t.elapsed().as_micros() as u64;
        assert!(loaded.complete && loaded.known.len() == 1024);
        Ok(
            json!({"kind":"synthetic fixture microbenchmark, shared sandbox x86-64; not physical or end-to-end input/power qualification","range_measurement":cases,"editor_50kb_30_insertions_us":editor_us,"ledger_1024_synthetic_groups_flush_us":flush_us,"ledger_1024_synthetic_groups_load_us":load_us,"all_measurement_calls_and_editor_events_executed":true}),
        )
    })();
    fs::remove_dir_all(&root)?;
    println!("{}", serde_json::to_string_pretty(&result?)?);
    Ok(())
}
