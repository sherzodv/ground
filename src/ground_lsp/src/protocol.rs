use std::io::{self, BufRead, Write};

use serde_json::{json, Value};

pub fn read_message(reader: &mut dyn BufRead) -> io::Result<Option<Value>> {
    let mut content_length = None;
    let mut line = String::new();
    loop {
        line.clear();
        if reader.read_line(&mut line)? == 0 {
            return Ok(None);
        }
        let trimmed = line.trim_end();
        if trimmed.is_empty() {
            break;
        }
        if let Some(v) = trimmed.strip_prefix("Content-Length:") {
            content_length = v.trim().parse::<usize>().ok();
        }
    }
    let len = match content_length {
        Some(v) => v,
        None => return Ok(None),
    };
    let mut buf = vec![0u8; len];
    reader.read_exact(&mut buf)?;
    let value: Value = serde_json::from_slice(&buf)?;
    Ok(Some(value))
}

pub fn send_response(out: &mut dyn Write, id: Option<Value>, result: Value) -> io::Result<()> {
    let payload = json!({ "jsonrpc": "2.0", "id": id.unwrap_or(Value::Null), "result": result });
    send_payload(out, &payload)
}

pub fn send_notification(out: &mut dyn Write, method: &str, params: Value) -> io::Result<()> {
    let payload = json!({ "jsonrpc": "2.0", "method": method, "params": params });
    send_payload(out, &payload)
}

fn send_payload(out: &mut dyn Write, payload: &Value) -> io::Result<()> {
    let body = serde_json::to_vec(payload)?;
    write!(out, "Content-Length: {}\r\n\r\n", body.len())?;
    out.write_all(&body)?;
    out.flush()
}
