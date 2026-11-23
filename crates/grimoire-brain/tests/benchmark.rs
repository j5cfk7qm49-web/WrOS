#![cfg(all(feature = "qualification", target_os = "linux"))]

use std::fs;
use std::process::Command;

use serde_json::Value;

#[test]
fn benchmark_writes_versioned_results() {
    let folder = tempfile::tempdir().unwrap();
    let output = folder.path().join("result.json");
    let run = Command::new(env!("CARGO_BIN_EXE_brain-bench"))
        .args([
            "--seed",
            "25001",
            "--profile",
            "smoke",
            "--revision",
            "fixture",
            "--output",
        ])
        .arg(&output)
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let record: Value = serde_json::from_slice(&fs::read(&output).unwrap()).unwrap();
    assert_eq!(record["schema"], 1);
    assert_eq!(record["seed"], 25_001);
    assert_eq!(record["revision"], "fixture");
    assert_eq!(record["profile"], "smoke");
    assert!(record["workload"]["visible_nodes"].as_u64().unwrap() > 0);
    assert!(record["workload"]["review_events"].as_u64().unwrap() > 0);
    assert!(record["background_batches"].as_u64().unwrap() > 0);
    assert!(record["initial_disk"]["store_bytes"].as_u64().unwrap() > 0);
    assert!(record["cold_open_peak_rss_bytes"].as_u64().unwrap() > 0);
    assert!(record["checkpoint_start_rss_bytes"].as_u64().unwrap() > 0);
    assert!(
        record["storage_only_checkpoint_peak_rss_bytes"]
            .as_u64()
            .unwrap()
            > 0
    );
    let metrics = &record["metrics"];
    for (name, limit) in [
        ("cold_open_p95_ms", 3000.0),
        ("edit_to_text_p95_ms", 50.0),
        ("fts_p95_ms", 150.0),
        ("checkpoint_peak_rss_bytes", 536870912.0),
        ("journal_bytes_per_edit", 65536.0),
    ] {
        assert_eq!(metrics[name]["limit"].as_f64(), Some(limit), "{name}");
        let observed = metrics[name]["observed"].as_f64().unwrap();
        assert_eq!(
            metrics[name]["passed"].as_bool(),
            Some(observed <= limit),
            "{name}"
        );
    }
    let temporary = &metrics["checkpoint_temporary_bytes"];
    assert!(temporary["observed"].as_u64().is_some());
    assert_eq!(
        temporary["limit"].as_u64().unwrap(),
        record["checkpoint_size_bytes"].as_u64().unwrap() + 64 * 1024 * 1024,
    );
    assert!(record["machine"]["os"].as_str().unwrap().contains("linux"));
}
