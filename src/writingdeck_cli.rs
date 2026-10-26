//! Non-interactive appliance diagnostics. No mutation unless explicitly creating
//! a brand-new tutorial directory. Status is read-only, not a system setup.
use anyhow::{Result, bail};
use std::path::PathBuf;
use writingdeck_sys::{Backend, Reply, Request};

pub fn command(mut args: impl Iterator<Item = String>) -> Result<()> {
    match args.next().as_deref() {
        Some("status") => {
            let simulated = match args.next().as_deref() {
                None => false,
                Some("--simulate") => true,
                Some(other) => bail!("unknown status option {other}"),
            };
            if args.next().is_some() {
                bail!("too many status arguments");
            }
            let mut backend: Box<dyn Backend> = if simulated {
                Box::new(writingdeck_sys::Simulated::laptop())
            } else {
                Box::new(writingdeck_sys::linux::Linux::default())
            };
            let Reply::Snapshot(snapshot) = backend.request(Request::Refresh)? else {
                bail!("invalid status response");
            };
            println!("{}", serde_json::to_string_pretty(&snapshot)?);
        }
        Some("tutorial-demo") => {
            let root = args.next().map(PathBuf::from).ok_or_else(|| {
                anyhow::anyhow!("supply a NEW tutorial folder; existing directories are refused")
            })?;
            if args.next().is_some() {
                bail!("too many tutorial arguments");
            }
            grimoire_core::tutorial::create_demo(&root)?;
            println!(
                "Safe demo created at {}\nOpen it with: grimoire {}\nThen F12 > Interactive tutorial.",
                root.display(),
                root.display()
            );
        }
        Some("prose") => prose_command(args)?,
        Some("lexicon") => {
            let query = args
                .next()
                .ok_or_else(|| anyhow::anyhow!("supply a quoted word/phrase"))?;
            let root = match args.next().as_deref() {
                None => std::env::var_os("WRITINGDECK_WORDNET_DIR")
                    .map(PathBuf::from)
                    .unwrap_or_else(|| PathBuf::from(grimoire_core::lexicon::DEFAULT_ROOT)),
                Some("--data") => args
                    .next()
                    .map(PathBuf::from)
                    .ok_or_else(|| anyhow::anyhow!("supply the local WordNet 3.0 folder"))?,
                Some(other) => bail!("unknown dictionary option {other}"),
            };
            if args.next().is_some() {
                bail!("too many dictionary arguments");
            }
            let lookup = grimoire_core::lexicon::Dictionary::open(&root)?.lookup(&query)?;
            println!("{}", serde_json::to_string_pretty(&lookup)?);
        }
        Some("check-config") => {
            let root = args
                .next()
                .map(PathBuf::from)
                .ok_or_else(|| anyhow::anyhow!("supply the manuscript folder"))?;
            if !root.join("manuscript").is_dir() {
                bail!("not a Grimoire project (no manuscript directory)");
            }
            if args.next().is_some() {
                bail!("too many configuration arguments");
            }
            let layout = grimoire_core::workspaces::Store::load(&root)?;
            let tutorial = grimoire_core::tutorial::Store::load(&root)?;
            let pins = grimoire_core::pins::Store::load(&root)?;
            let journal = grimoire_core::journal::validate_existing(&root)?;
            let workflow = grimoire_core::workflow::Store::load(&root)?;
            let _productivity = grimoire_core::statefile::Store::load(
                &root,
                std::path::Path::new(".grimoire/writingdeck/productivity.json"),
                grimoire_core::productivity::Settings::default(),
            )?;
            let ledger = grimoire_core::accounting::Ledger::load(&root);
            if !ledger.complete {
                bail!(
                    "operation ledger unavailable/incomplete: {}; originals retained",
                    ledger.warning
                );
            }
            let challenge_days = grimoire_core::challenges::validate_existing(&root)?;
            let (spark_days, spark_assignments) = grimoire_core::spark::validate_existing(&root)?;
            let _checks = grimoire_core::diagnostics::preferences::NovelPreferences::load(&root)?;
            let _personal_checks = std::env::var_os("HOME")
                .map(PathBuf::from)
                .map(|home| {
                    grimoire_core::diagnostics::preferences::PersonalPreferences::load(&home)
                })
                .transpose()?;
            let groups = ledger.known.len();
            let scratch_count = workflow.state.notes.len();
            println!(
                "Layout schema {} validated; {} workspaces. Tutorial schema {} validated. Pins schema {} validated; {} references (source availability checked in UI). {journal} journal date files validated (other filenames not interpreted). Workflow schema 1 validated; {scratch_count} scratchpad references (sources checked in UI). Productivity schema 1 validated; {groups} recorded operation groups; {challenge_days} challenge date files validated (other filenames not interpreted). Spark schema 1 validated; {spark_days} fixed daily / {spark_assignments} optional assignment files. Diagnostic schema 1 validated (including personal profile when HOME is set); no analyzer started. No files changed.",
                layout.state.version,
                layout.state.workspaces.len(),
                tutorial.progress.version,
                pins.state.version,
                pins.state.pins.len()
            );
        }
        Some("version") => println!(
            "WritingDeck OS 0.1.0-dev · Grimoire {} · upstream base f3ca1e8",
            env!("CARGO_PKG_VERSION")
        ),
        None | Some("help" | "--help" | "-h") => println!(
            "WritingDeck OS development foundation\n\n  grimoire writingdeck status [--simulate]  read-only JSON status\n  grimoire writingdeck tutorial-demo DIR   create a NEW safe demo\n  grimoire writingdeck lexicon WORD [--data DIR]  local WordNet 3.0 senses/antonyms (read-only)\n  grimoire writingdeck check-config DIR    validate without rewriting\n  grimoire writingdeck version             application/base identification\n\nMain UI: F12 home, F10 outline, Ctrl-D focus.\nOS build and installation: docs/INSTALLATION.md\nCurrent limitations: docs/FEATURE_MATRIX.md"
        ),
        Some(other) => bail!("unknown WritingDeck command {other}; try grimoire writingdeck help"),
    }
    Ok(())
}

/// Explicit read-only command: never discovers a novel, runs a shell, saves a
/// result/profile, or enables any model without the separate named flag.
fn prose_command(mut args: impl Iterator<Item = String>) -> Result<()> {
    use grimoire_core::{
        diagnostics::{Profile, Snapshot, SourceRole, SourceUnit},
        prose, prose_tools,
    };
    use std::{
        fs::File,
        io::Read,
        sync::{Arc, atomic::AtomicBool},
    };
    let path = args
        .next()
        .map(PathBuf::from)
        .ok_or_else(|| anyhow::anyhow!("supply one UTF-8 Markdown file"))?;
    let mut engine = "native".to_owned();
    let mut permit_model = false;
    let mut options = prose::Options::default();
    options.analysis.profile = Profile::Revision;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--engine" => {
                engine = args
                    .next()
                    .ok_or_else(|| anyhow::anyhow!("supply native, proselint or nabokov"))?
            }
            "--allow-model" => permit_model = true,
            "--include-dialogue-style" => options.analysis.relaxed_dialogue = false,
            "--include-footnotes" => options.analysis.include_footnotes = true,
            "--preset" => {
                options.preset = match args.next().as_deref() {
                    Some("general") => prose::FictionPreset::General,
                    Some("accessible") => prose::FictionPreset::Accessible,
                    Some("experimental") => prose::FictionPreset::Experimental,
                    _ => bail!("unknown/missing fiction preset"),
                }
            }
            _ => bail!("unknown prose option {arg}"),
        }
    }
    if !matches!(engine.as_str(), "native" | "proselint" | "nabokov") {
        bail!("unsupported prose engine; only the pinned adapter contracts are accepted")
    }
    if engine == "nabokov" && !permit_model {
        bail!(
            "Nabokov uses a trained local spaCy model. Explicit --allow-model permits ONE run on this file; nothing downloaded or enabled live."
        )
    }
    let file = File::open(&path)?;
    if !file.metadata()?.is_file() {
        bail!("prose input must be a regular file")
    }
    let mut text = String::new();
    file.take((grimoire_core::diagnostics::MAX_SOURCE_BYTES + 1) as u64)
        .read_to_string(&mut text)?;
    let document = path
        .file_name()
        .ok_or_else(|| anyhow::anyhow!("missing source filename"))?;
    let source = Snapshot::new(
        std::path::Path::new(document),
        SourceUnit::MarkdownFile,
        SourceRole::Manuscript,
        1,
        text,
    )?;
    let (status, findings, metrics, omitted, elapsed, peak) = if engine == "native" {
        let report = prose::analyze(source.clone(), options)?;
        (
            format!("{:?}", report.status).to_lowercase(),
            report.findings,
            Some(report.metrics),
            report.omitted,
            report.elapsed_ms,
            0,
        )
    } else {
        let which = if engine == "nabokov" {
            prose_tools::Engine::Nabokov
        } else {
            prose_tools::Engine::Proselint
        };
        let permission =
            permit_model.then(|| prose_tools::ModelPermit::allow_once(&source, &options));
        let report = prose_tools::analyze(
            source.clone(),
            &options,
            which,
            &prose_tools::Runtime::default(),
            permission.as_ref(),
            Arc::new(AtomicBool::new(false)),
        )?;
        (
            serde_json::to_value(report.status)?
                .as_str()
                .unwrap_or("failed")
                .to_owned(),
            report.findings,
            None,
            report.omitted,
            report.elapsed_ms,
            report.peak_rss_kib,
        )
    };
    let rows:Vec<_>=findings.iter().map(|d|serde_json::json!({"engine":d.rule.engine,"version":d.rule.version,"rule":d.rule.rule,"category":d.category,"severity":d.severity,"context":d.context,"utf8_bytes":d.span,"scalar_start":source.point(d.span.start),"scalar_end":source.point(d.span.end),"expected":d.expected,"message":d.message,"alternatives":d.replacements.iter().map(|r|serde_json::json!({"text":r.text,"source_safe":r.source_safe})).collect::<Vec<_>>()})).collect();
    println!(
        "{}",
        serde_json::to_string_pretty(
            &serde_json::json!({"schema":1,"engine":engine,"source_unit":"markdown-file","source_sha256":source.stamp.sha256,"status":status,"metrics":metrics,"findings":rows,"omitted":omitted,"elapsed_ms":elapsed,"child_peak_rss_kib":peak,"interpretation":"Local advisory patterns only. Analysis words are not ledger word units; readability is approximate English, not literary quality or authorship evidence."})
        )?
    );
    if status != "complete" {
        bail!("prose engine did not complete; result is not a clean check: {status}")
    }
    Ok(())
}
