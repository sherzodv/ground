mod features;
mod protocol;
mod server;
mod util;
mod workspace;

use std::fs;
use std::io;
use std::process::Command;

const PID_FILE: &str = ".ground/lsp.pid";

pub fn start() -> io::Result<()> {
    fs::create_dir_all(".ground")?;
    fs::write(PID_FILE, std::process::id().to_string())?;
    let res = server::run();
    let _ = fs::remove_file(PID_FILE);
    res
}

pub fn stop() -> io::Result<()> {
    let pid = match fs::read_to_string(PID_FILE) {
        Ok(s) => s.trim().to_string(),
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e),
    };
    let _ = Command::new("kill").arg(&pid).status();
    let _ = fs::remove_file(PID_FILE);
    Ok(())
}
