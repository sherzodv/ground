# Current work: move rendering into the compiler

## Problem

`ground_be_terra` currently owns three concerns that don't belong together:

1. Flattening `AsmDef` → Tera context (`def_to_ctx`, `vpc_def_to_ctx`).
2. Picking a template entry by hardcoded Rust `match` on `type_name` / `kind`.
3. Calling `ground_gen::render` and shelling out to Terraform.

Only (3) is backend work. (1) and (2) are the missing **render phase** of the
compiler. `ground_be_terra` should shrink to ops-only: take rendered files, run
`terraform`, surface output.

## Pipeline

```
parse → resolve → asm (lower) → render
```

`render` is a separate compiler phase, not part of `compile()`. The LSP path
(`analyze`) skips it.

```rust
pub fn compile(req: CompileReq) -> CompileRes;                  // unchanged
pub fn render(res: &CompileRes,
              target: &RenderTarget,
              templates: &[TemplateUnit]) -> RenderRes;         // NEW
```

## Template selection

Convention, not configuration:

- Each `plan` lives in a pack.
- The compiler looks in that pack for `main.<backend>.<type>.tera`.
- That file is the **manifest entry** — it renders to JSON with a
  `files: [{file, template}]` array that fans out to helper templates in the
  same pack closure (same shape `ground_gen::render` already handles).
- `<backend>` and `<type>` are **opaque filename tokens**. The compiler never
  enumerates or validates them against a registry. Adding a backend = dropping
  a file.

## Target configuration

Single target per project for now (multi-target can become a list later).

Settings file `.ground/settings.json`:

```json
{
  "target": "tf:json",
  "src": "src",
  "templates": "templates"
}
```

- `target` — `"<backend>:<type>"` string, parsed by splitting on `:`.
- `src` — root for `.grd` / `.ts` discovery (default: project root or `src`).
- `templates` — optional; when set, templates live under a parallel tree
  mirroring `src`'s pack structure. When unset, `.tera` files are discovered
  alongside `.grd` / `.ts` in the same root.

CLI:

```
ground init                     # scaffold .ground/, no target
ground config target tf:json    # set/change target
ground config target            # print current target
ground gen                      # render using settings target
ground gen --target k8s:yaml    # per-invocation override
```

`init` no longer takes a target — keeps it idempotent and safe to re-run.
Changing the target is a separate, discoverable command.

## Templates are loosely coupled to packs

Templates are pack-addressable resources but are NOT tied to `.grd` files.
They live in their own list on the compile request:

```rust
pub struct CompileReq {
    pub units: Vec<Unit>,
    pub templates: Vec<TemplateUnit>,
}

pub struct TemplateUnit {
    pub path: Vec<String>,   // pack path (same rules as Unit.path)
    pub file: String,        // e.g. "main.tf.json.tera", "_helpers.tera"
    pub content: String,
}
```

Manifest lookup: `path == plan_pack && file == "main.{backend}.{type}.tera"`.
Helper templates referenced from the manifest resolve within the plan pack's
`use`-closure, same scoping as defs.

## Rendering engine boundary

The compiler NEVER uses `tera::Tera` directly. All rendering goes through
`ground_gen::render(&RenderReq, &ctx)`. If we swap Tera later, only
`ground_gen` changes.

`render()` flow:

1. Find plan's pack; pick `main.{backend}.{out_type}.tera`.
2. Collect in-scope `TemplateUnit`s → `Vec<TeraUnit>`.
3. Build context from the planned `AsmDef` via pure `asm_value_to_json` —
   no backend-specific massaging.
4. Call `ground_gen::render`.
5. Wrap each `JsonUnit` as a `RenderUnit`.

```rust
pub struct RenderTarget {
    pub backend: String,
    pub out_type: String,
}

pub struct RenderUnit {
    pub backend: String,
    pub out_type: String,
    pub plan: String,
    pub file: String,
    pub content: String,
}

pub struct RenderRes {
    pub units: Vec<RenderUnit>,
    pub errors: Vec<CompileError>,
}
```

## Consequences for `ground_be_terra`

- Stops importing `ground_gen`.
- Templates under `src/ground_be_terra/src/templates/` move into the stdlib
  / test packs where they semantically belong.
- `def_to_ctx` and `vpc_def_to_ctx` disappear. Any derivation they perform
  (zones, NAT keys, bucket names, region expansion) moves into Ground defs +
  TS mappers in `std/aws/tf`, or into Tera macros. The CLAUDE heuristic
  already requires this — templates receive fully resolved vendor entities.
- **Operates on a written project on disk**, not on `Vec<RenderUnit>` in
  memory. The CLI writes render output to `.ground/<backend>/<plan>/...`
  first; `ground_be_terra` reads that tree and runs `terraform`. Keeps the
  backend crate free of render/IO plumbing and lets the written tree be
  inspected, diffed, or hand-edited between `gen` and `apply`.
- Final surface: `ground_be_terra::plan(project_dir)` /
  `ground_be_terra::apply(project_dir)` — path in, ops out. No templates,
  no Tera, no context building, no `RenderUnit`.

## File writes

File writes live in the `ground` crate (CLI). `ground_compile` and
`ground_gen` are pure — they return data. The CLI composes on-disk paths
(`.ground/<backend>/<plan>/<file>`) from `RenderUnit` fields and writes the
tree. `ground_be_terra` then consumes the tree by path.

Flow for `ground gen`:

1. CLI reads `.ground/settings.json` → `RenderTarget`.
2. CLI walks `src` (+ optional `templates` root) → `Vec<Unit>` +
   `Vec<TemplateUnit>`.
3. `ground_compile::compile(...)` → `CompileRes`.
4. `ground_compile::render(&res, &target, &templates)` → `Vec<RenderUnit>`.
5. CLI writes each `RenderUnit` to `.ground/<backend>/<plan>/<file>`.

`ground apply` then calls `ground_be_terra::apply(".ground/tf/<plan>")`.

## Template root normalization

When `templates` is configured as a separate root from `src`, the CLI
discovers `.tera` files under that root and must assign each a
**pack path that matches the corresponding `.grd` pack path** — otherwise
manifest lookup (`path == plan_pack`) fails.

Rule: strip the configured `templates` root prefix before computing the
pack path, the same way the `src` root prefix is stripped for `.grd` / `.ts`
discovery. `templates/app/main.tf.json.tera` with `templates = "templates"`
→ `TemplateUnit { path: ["app"], file: "main.tf.json.tera" }`, which
matches `app.grd` under `src/app.grd` with `path: ["app"]`.

Same normalization applies when `templates` is unset (shared root with
`src`): strip the `src` root prefix. The compiler sees identical pack
paths regardless of whether templates live inline or in a parallel tree.

## Errors the new phase introduces

1. Missing manifest: `no "main.tf.json.tera" found in pack "<plan-pack>"`.
2. Manifest parse fail (reuse `GenError::Manifest`, surface with location).
3. Helper template referenced by manifest not found in pack closure.

## Implementation order

1. Add `TemplateUnit`, `RenderTarget`, `RenderUnit`, `RenderRes` to
   `ground_compile`. Extend `CompileReq` with `templates`.
2. Implement `ground_compile::render()` routing through `ground_gen`.
3. Settings + CLI: `ground config target`, `--target` flag, settings.json
   `target` / `templates` fields.
4. Golden coverage: extend `ground_test` fixtures to exercise render with
   `main.tf.json.tera` in the plan pack.
5. Relocate `ground_be_terra/src/templates/*.tera` into stdlib/test packs.
6. Shrink `ground_be_terra` to ops-only; drop `def_to_ctx` / `vpc_def_to_ctx`.
   Port their derivation into Ground + TS (larger, separate pass).

# Other notes

- remove existing templates from ground_be_terra
- one render call per list of plans in the pack
- yes asm should provide a list of plan roots

