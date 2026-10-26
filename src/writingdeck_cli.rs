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
            "WritingDeck OS development foundation\n\n  grimoire writingdeck status [--simulate]  read-only JSON status\n  grimoire writingdeck tutorial-demo DIR   create a NEW safe demo\n  grimoire writingdeck check-config DIR    validate without rewriting\n  grimoire writingdeck version             application/base identification\n\nMain UI: F12 home, F10 outline, Ctrl-D focus.\nOS build and installation: docs/INSTALLATION.md\nCurrent limitations: docs/FEATURE_MATRIX.md"
        ),
        Some(other) => bail!("unknown WritingDeck command {other}; try grimoire writingdeck help"),
    }
    Ok(())
}
