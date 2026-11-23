use std::path::PathBuf;
use std::process::ExitCode;

use grimoire_brain::benchmark::{Profile, run};

fn main() -> ExitCode {
    match execute() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("brain-bench: {error}");
            ExitCode::FAILURE
        }
    }
}

fn execute() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut seed = None;
    let mut profile = None;
    let mut output = None;
    let mut revision = None;
    let mut args = std::env::args_os().skip(1);
    while let Some(flag) = args.next() {
        if flag == "--help" {
            println!(
                "brain-bench --seed NUMBER --profile smoke|aged --output FILE [--revision COMMIT]"
            );
            return Ok(());
        }
        let value = args.next().ok_or("Every option needs a value")?;
        match flag.to_str() {
            Some("--seed") => seed = Some(value.to_str().ok_or("Invalid seed")?.parse()?),
            Some("--profile") if value == "smoke" => profile = Some(Profile::Smoke),
            Some("--profile") if value == "aged" => profile = Some(Profile::Aged),
            Some("--profile") => return Err("Choose the smoke or aged profile".into()),
            Some("--output") => output = Some(PathBuf::from(value)),
            Some("--revision") => {
                revision = Some(value.into_string().map_err(|_| "Invalid revision")?)
            }
            _ => return Err(format!("Unknown option {}", flag.to_string_lossy()).into()),
        }
    }
    let output = output.ok_or("Missing --output")?;
    let within_limits = run(
        seed.ok_or("Missing --seed")?,
        profile.ok_or("Missing --profile")?,
        revision.as_deref().unwrap_or("unspecified"),
        &output,
    )?;
    println!("Results written to {}", output.display());
    if !within_limits {
        println!("One or more measurements exceeded the Stage 0 limits.");
    }
    Ok(())
}
