//! Bounded process execution and protected stdin for the Linux adapter.
use super::Invocation;
use anyhow::{Context, Result, bail};
use std::{
    io::{Read, Write},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

/// Bounded text of a failed helper: stderr first, then the `message` of a JSON
/// reply on stdout, then the bare exit status.
fn failure_detail(stdout: &[u8], stderr: &[u8], status: std::process::ExitStatus) -> String {
    let stderr = String::from_utf8_lossy(stderr);
    let stderr = stderr.trim();
    if !stderr.is_empty() {
        return stderr.to_string();
    }
    if let Ok(serde_json::Value::Object(object)) = serde_json::from_slice(stdout)
        && let Some(message) = object.get("message").and_then(serde_json::Value::as_str)
        && !message.is_empty()
        && message.len() <= 1024
    {
        return message.chars().filter(|c| !c.is_control()).collect();
    }
    format!("exited with {status}")
}
pub trait Runner: Send {
    fn run(&mut self, cmd: &Invocation) -> Result<String>;
    fn run_with_input(&mut self, cmd: &Invocation, input: &[u8]) -> Result<String> {
        if !input.is_empty() {
            bail!("this runner does not support protected input");
        }
        self.run(cmd)
    }
}
#[derive(Default)]
pub struct ProcessRunner;
impl Runner for ProcessRunner {
    fn run(&mut self, cmd: &Invocation) -> Result<String> {
        self.run_with_input(cmd, &[])
    }
    fn run_with_input(&mut self, cmd: &Invocation, input: &[u8]) -> Result<String> {
        let mut child = Command::new(cmd.program)
            .args(&cmd.args)
            .env("LC_ALL", "C")
            .env("PATH", crate::paths::SAFE_PATH)
            .stdin(if input.is_empty() {
                Stdio::null()
            } else {
                Stdio::piped()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .with_context(|| format!("starting {} (is its package installed?)", cmd.program))?;
        if !input.is_empty()
            && let Some(mut stdin) = child.stdin.take()
            && let Err(err) = stdin.write_all(input).and_then(|()| stdin.flush())
            && err.kind() != std::io::ErrorKind::BrokenPipe
        {
            let _ = child.kill();
            let _ = child.wait();
            bail!("writing protected input to {} failed", cmd.program);
        }
        // Drain pipes while waiting, with a bounded retained buffer.
        fn drain(mut pipe: impl Read) -> std::io::Result<Vec<u8>> {
            let mut kept = Vec::new();
            let mut b = [0; 4096];
            loop {
                let n = pipe.read(&mut b)?;
                if n == 0 {
                    return Ok(kept);
                }
                let room = 65_537usize.saturating_sub(kept.len());
                kept.extend_from_slice(&b[..n.min(room)]);
            }
        }
        let (Some(out), Some(err)) = (child.stdout.take(), child.stderr.take()) else {
            let _ = child.kill();
            let _ = child.wait();
            bail!("could not capture output from {}", cmd.program);
        };
        let out = std::thread::spawn(move || drain(out));
        let err = std::thread::spawn(move || drain(err));
        let start = Instant::now();
        let status = loop {
            if let Some(status) = child.try_wait()? {
                break Some(status);
            }
            if start.elapsed() > cmd.timeout {
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
            std::thread::sleep(Duration::from_millis(20));
        };
        let Some(status) = status else {
            // Do not join the pipe readers: a helper child that outlives the
            // killed launcher would hold the pipes open and hang the worker.
            // The detached readers end when the pipes close.
            bail!("{} timed out; no success assumed", cmd.program);
        };
        // An exited launcher can leave a descendant holding a pipe open.
        // The same deadline applies to draining it as to waiting for the child.
        while !out.is_finished() || !err.is_finished() {
            if start.elapsed() > cmd.timeout {
                bail!(
                    "{} timed out while reading its reply; no success assumed",
                    cmd.program
                );
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        let stdout = out
            .join()
            .map_err(|_| anyhow::anyhow!("stdout reader failed"))??;
        let stderr = err
            .join()
            .map_err(|_| anyhow::anyhow!("stderr reader failed"))??;
        if stdout.len() > 65_536 || stderr.len() > 65_536 {
            bail!(
                "{} output exceeds limit; no partial result accepted",
                cmd.program
            );
        }
        if !status.success() {
            bail!(
                "{}: {}",
                cmd.program,
                failure_detail(&stdout, &stderr, status)
            );
        }
        Ok(String::from_utf8(stdout)
            .context("system reply is not UTF-8")?
            .trim_end_matches(['\r', '\n'])
            .to_string())
    }
}
