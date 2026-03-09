# RFC 0002 — Generic async command runner with layered output parsing

## Problem

`terra_ops` runs terraform synchronously and discards its output. Ground will
need to run other external tools (docker, kubectl, …) with the same needs:
spawn a process, stream output in real time, parse lines into typed events,
surface user-friendly messages to the caller.

Build it once, generically.

---

## Goals

- Run any external command non-blocking.
- Parse stdout/stderr line-by-line through a pluggable parser.
- Runner emits its own lifecycle events (spawned, exited, …).
- Dedicated enrichment layer to compose user-friendly display messages.
- No async runtime — `std::thread` + `std::sync::mpsc`.

---

## New crate: `ground_run`

Sits between `ground_core` and all backends. No knowledge of terraform or any
tool. Zero dependencies beyond `std`.

```
ground_core
    │
    ▼
ground_run          ← new
    │
    ▼
ground_be_terra
    │
    ▼
ground  (CLI)
```

---

## Layer 1 — Runner (`ground_run`)

### `OutputParser` trait

```rust
pub enum Source { Stdout, Stderr }

pub trait OutputParser: Send + 'static {
    type Event: Send;
    fn parse(&mut self, line: &str, source: Source) -> Option<Self::Event>;
}
```

### `RunEvent<E>` — runner lifecycle + parsed events

The runner emits its own lifecycle messages alongside parsed events. Callers
get a single unified stream.

```rust
pub enum RunEvent<E> {
    Spawned,             // process started successfully
    Line(E),             // parsed event from OutputParser
    Stderr(String),      // raw stderr line when parser returns None for it
    Exited(ExitStatus),  // process exited; always the last event
}

pub struct ExitStatus { pub code: Option<i32>, pub success: bool }
```

### `spawn`

```rust
pub fn spawn<P: OutputParser>(
    cmd:    &mut std::process::Command,
    parser: P,
) -> Result<Receiver<RunEvent<P::Event>>, RunError>
```

Internals:
1. Start process with `stdout: piped, stderr: piped`. Emit `Spawned`.
2. One reader thread per stream → shared internal `mpsc`.
3. Parser thread: calls `parser.parse()`, forwards `Line(event)` or
   `Stderr(raw)` to caller's `Receiver`.
4. When both reader threads close, emit `Exited` and terminate.

```rust
pub enum RunError {
    NotFound(String),
    SpawnFailed(String),
}
```

---

## Layer 2 — Tool parser (`ground_be_terra::terra_ops::parser`)

### TfEvent

Terraform with `-json` flag emits NDJSON to stdout (supported since 0.15,
minimum required version). `terraform init` has no JSON mode — parse its
stdout with best-effort regex patterns.

```rust
pub enum TfEvent {
    // init (best-effort regex)
    InitProviderDownload { name: String, version: String },
    InitComplete,

    // plan / apply (-json NDJSON)
    ResourcePlanned  { address: String, action: Action },
    ResourceApplying { address: String },
    ResourceDone     { address: String, elapsed_secs: u32 },
    ResourceErrored  { address: String, message: String },
    ChangeSummary    { add: u32, change: u32, remove: u32 },
    Diagnostic       { severity: String, summary: String },
    ApiCall          { method: String, url: String },  // TF_LOG=DEBUG only
    Unknown          { raw: String },
}
```

`TfParser` implements `OutputParser<Event = TfEvent>`:
- stdout: try JSON parse first; fall back to regex for init lines.
- stderr: emit as `Diagnostic` (terraform writes warnings there); if
  `TF_LOG=DEBUG` is set, also emit `ApiCall` lines extracted from HTTP log
  entries.

---

## Layer 3 — Ground events (`ground_be_terra::terra_ops`)

Provider-agnostic. `terra_ops` maps `TfEvent → OpsEvent`.

```rust
pub enum OpsEvent {
    // init
    ProviderReady   { name: String, version: String },
    InitDone,

    // plan
    ResourceQueued  { name: String, action: Action },
    PlanReady       { summary: PlanSummary },

    // apply
    ResourceApplying { name: String, action: Action },
    ResourceDone     { name: String, action: Action, elapsed_secs: u32 },
    ResourceFailed   { name: String, reason: String },
    ApplyDone,

    Warning         { message: String },
}
```

Public API — returns immediately, caller drives the loop:

```rust
pub fn init(dir: &Path)  -> Result<Receiver<RunEvent<OpsEvent>>, OpsError>;
pub fn plan(dir: &Path)  -> Result<Receiver<RunEvent<OpsEvent>>, OpsEvent>;
pub fn apply(dir: &Path) -> Result<Receiver<RunEvent<OpsEvent>>, OpsError>;
```

`plan` is two-phase: stream `ResourceQueued` events while terraform runs, then
run `terraform show -json .tfplan` after exit to produce `PlanReady`. This
keeps live progress without screen-scraping the plan summary.

---

## Layer 4 — Enrichment (`ground::ops_display`)

This layer owns all user-facing copy. It translates `RunEvent<OpsEvent>` into
`DisplayEvent`s — the only place messages are composed. Keeping enrichment
separate means it can add context (stack name, elapsed wall time, counts) and
be tested independently of the runner or terraform.

```rust
pub struct DisplayEvent {
    pub message: String,
    pub level:   DisplayLevel,   // Info | Warn | Error | Progress
    pub detail:  Option<String>, // secondary line, shown indented
}

pub enum DisplayLevel { Info, Warn, Error, Progress }

pub trait EventEnricher {
    fn enrich(&mut self, event: RunEvent<OpsEvent>) -> Option<DisplayEvent>;
}
```

`EventEnricher` is stateful (`&mut self`) so it can accumulate context:
elapsed timers, resource counts, current stack name, etc.

Example enrichments:

| OpsEvent | DisplayEvent.message |
|----------|----------------------|
| `RunEvent::Spawned` | `"running terraform plan"` |
| `ResourceQueued { name, Create }` | `"+ aws_ecs_service.svc_api"` |
| `ResourceDone { name, Create, 12 }` | `"✓ aws_ecs_service.svc_api  (12s)"` |
| `ResourceFailed { name, reason }` | `"✗ aws_ecs_service.svc_api"`, detail: reason |
| `PlanReady { summary }` | `"plan: 3 to add, 0 to change, 0 to destroy"` |
| `RunEvent::Exited(ok)` | `"done"` / `"failed"` |

The CLI loop:

```rust
let rx       = terra_ops::plan(dir)?;
let enricher = TerraEnricher::new(stack_name);

for event in rx {
    if let Some(d) = enricher.enrich(event) {
        render(&d);   // CLI formats and prints
    }
}
```

---

## TF_LOG (opt-in)

```rust
pub struct RunOptions { pub tf_log: Option<TfLogLevel> }
```

When set, injects `TF_LOG=<level>` into the child env. The enricher can choose
to surface `ApiCall` events as debug lines or drop them.

---

## Module map

| Layer | Crate | Module |
|-------|-------|--------|
| Runner, `OutputParser`, `RunEvent` | `ground_run` | `lib` |
| `TfParser`, `TfEvent` | `ground_be_terra` | `terra_ops::parser` |
| `OpsEvent`, `init/plan/apply` | `ground_be_terra` | `terra_ops` |
| `EventEnricher`, `DisplayEvent` | `ground` | `ops_display` |
| Render loop | `ground` | `main` |
