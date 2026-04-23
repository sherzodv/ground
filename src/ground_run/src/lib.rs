use std::io::{BufRead, BufReader};
use std::process::Stdio;
use std::sync::mpsc::{self, Receiver};
use std::thread;

#[derive(Clone, Copy)]
pub enum Source {
    Stdout,
    Stderr,
}

pub trait OutputParser: Send + 'static {
    type Event: Send;
    fn parse(&mut self, line: &str, source: Source) -> Option<Self::Event>;
}

#[derive(Debug)]
pub struct ExitStatus {
    pub code: Option<i32>,
    pub success: bool,
}

pub enum RunEvent<E> {
    Spawned,
    Raw(String), // raw stdout line, always emitted before the parsed Line
    Line(E),
    Stderr(String),     // raw stderr line the parser returned None for
    Exited(ExitStatus), // always last
}

#[derive(Debug)]
pub enum RunError {
    NotFound(String),
    SpawnFailed(String),
}

impl std::fmt::Display for RunError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RunError::NotFound(s) => write!(f, "command not found: {s}"),
            RunError::SpawnFailed(s) => write!(f, "spawn failed: {s}"),
        }
    }
}

pub fn spawn<P: OutputParser>(
    cmd: &mut std::process::Command,
    parser: P,
) -> Result<Receiver<RunEvent<P::Event>>, RunError> {
    let mut child = cmd
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                RunError::NotFound(e.to_string())
            } else {
                RunError::SpawnFailed(e.to_string())
            }
        })?;

    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();

    let (tx_raw, rx_raw) = mpsc::channel::<(Source, String)>();
    let (tx_ev, rx_ev) = mpsc::channel::<RunEvent<P::Event>>();

    // stdout reader
    let tx1 = tx_raw.clone();
    thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if tx1.send((Source::Stdout, line)).is_err() {
                break;
            }
        }
    });

    // stderr reader
    let tx2 = tx_raw;
    thread::spawn(move || {
        for line in BufReader::new(stderr).lines().map_while(Result::ok) {
            if tx2.send((Source::Stderr, line)).is_err() {
                break;
            }
        }
    });

    // parser thread — owns child, drives the outer channel
    thread::spawn(move || {
        let mut parser = parser;
        let _ = tx_ev.send(RunEvent::Spawned);

        for (source, line) in rx_raw {
            let is_stderr = matches!(source, Source::Stderr);
            if !is_stderr {
                let _ = tx_ev.send(RunEvent::Raw(line.clone()));
            }
            match parser.parse(&line, source) {
                Some(ev) => {
                    let _ = tx_ev.send(RunEvent::Line(ev));
                }
                None if is_stderr => {
                    let _ = tx_ev.send(RunEvent::Stderr(line));
                }
                None => {}
            }
        }

        let status = child
            .wait()
            .map(|s| ExitStatus {
                code: s.code(),
                success: s.success(),
            })
            .unwrap_or(ExitStatus {
                code: None,
                success: false,
            });

        let _ = tx_ev.send(RunEvent::Exited(status));
    });

    Ok(rx_ev)
}
