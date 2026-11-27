//! Exchange newline-delimited JSON commands and replies with mpv.

use super::Mpv;
use anyhow::{Result, anyhow};
use serde_json::{Value, json};

#[cfg(unix)]
use std::{
    io::{BufRead, BufReader, Write},
    os::unix::net::UnixStream,
    time::{Duration, Instant},
};

impl Mpv {
    #[cfg(unix)]
    fn reply(&mut self, args: &[Value]) -> Result<Value> {
        if !self.connect_existing()? {
            self.start()?;
        }
        self.request_id = self.request_id.wrapping_add(1);
        let stream = self
            .stream
            .as_mut()
            .ok_or_else(|| anyhow!("mpv connection unavailable"))?;
        let result = exchange(stream, args, self.request_id);
        if result.is_err() {
            self.stream = None;
        }
        result
    }

    #[cfg(not(unix))]
    fn reply(&mut self, _: &[Value]) -> Result<Value> {
        Err(anyhow!("music files need mpv on Linux"))
    }

    pub(in crate::music) fn command(&mut self, args: &[Value]) -> Result<Value> {
        reply_data(self.reply(args)?)
    }

    pub(in crate::music) fn property(&mut self, name: &str) -> Result<Option<Value>> {
        let reply = self.reply(&[json!("get_property"), json!(name)])?;
        match reply["error"].as_str() {
            Some("property unavailable" | "property not found") => Ok(None),
            _ => {
                let data = reply_data(reply)?;
                Ok((!data.is_null()).then_some(data))
            }
        }
    }
}

#[cfg(unix)]
fn exchange(stream: &mut BufReader<UnixStream>, args: &[Value], id: i64) -> Result<Value> {
    let socket = stream.get_mut();
    serde_json::to_writer(&mut *socket, &json!({ "command": args, "request_id": id }))?;
    socket.write_all(b"\n")?;
    let deadline = Instant::now() + Duration::from_secs(1);
    let mut line = String::new();
    loop {
        line.clear();
        if stream.read_line(&mut line)? == 0 {
            return Err(anyhow!("mpv closed its socket"));
        }
        let reply: Value = serde_json::from_str(&line)?;
        if reply.get("event").is_none() && reply["request_id"].as_i64() == Some(id) {
            return Ok(reply);
        }
        if Instant::now() >= deadline {
            return Err(anyhow!("timed out waiting for mpv"));
        }
    }
}

fn reply_data(mut reply: Value) -> Result<Value> {
    match reply["error"].as_str() {
        Some("success") => Ok(reply.get_mut("data").map(Value::take).unwrap_or_default()),
        Some(error) => Err(anyhow!("mpv: {error}")),
        None => Err(anyhow!("mpv reply is missing an error status")),
    }
}
