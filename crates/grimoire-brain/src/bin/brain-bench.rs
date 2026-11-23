use std::path::PathBuf;
use std::process::ExitCode;

use grimoire_brain::benchmark::{
    Profile, checkpoint_worker, cold_open_worker, prepare_worker, run,
};

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
    let mut args = std::env::args_os().skip(1).peekable();
    match args.peek().and_then(|argument| argument.to_str()) {
        Some("--prepare-fixture") => {
            args.next();
            let profile = Profile::parse(&args.next().ok_or("Missing fixture profile")?)?;
            let seed = args.next().ok_or("Missing fixture seed")?;
            let root = PathBuf::from(args.next().ok_or("Missing fixture folder")?);
            return prepare_worker(
                seed.to_str().ok_or("Invalid fixture seed")?.parse()?,
                profile,
                &root,
            );
        }
        Some("--cold-open") => {
            args.next();
            return cold_open_worker(&PathBuf::from(args.next().ok_or("Missing fixture folder")?));
        }
        Some("--storage-checkpoint") => {
            args.next();
            return checkpoint_worker(&PathBuf::from(args.next().ok_or("Missing fixture folder")?));
        }
        _ => {}
    }
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
            Some("--profile") => profile = Some(Profile::parse(&value)?),
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
