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
            println!(
                "Layout schema {} validated; {} workspaces. Tutorial schema {} validated. Pins schema {} validated; {} references (source availability checked in UI). No files changed.",
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
