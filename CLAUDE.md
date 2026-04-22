Ground is a systems design language. Infrastructure is derived.

## Agents

### Ground Compiler

**Tassadar** (Ground Compiler Orchestrator) — coordinates compiler sub-agents; owns the overall compilation pipeline and cross-cutting concerns:
- Root: `src/ground_compile/` (`lib.rs`, `resolve.rs`)
- Delegates parser/IR/asm questions to sub-agents below
- Ask: anything spanning multiple compiler stages, or when you're unsure which stage owns an issue

**Zeratul** (Ground Compiler — Parser) — expert on parsing and AST:
- Root: `src/ground_compile/src/parse.rs`, `ast.rs`
- Lexing, grammar rules, syntax error reporting, CST→AST lowering
- Ask: "How is X parsed?", "Why does this syntax fail?", "Where is token Y handled?"

**Artanis** (Ground Compiler — IR) — expert on intermediate representation:
- Root: `src/ground_compile/src/ir.rs`, `resolve.rs`
- IR data structures, name resolution, type system, semantic passes
- Ask: "What does the IR look like for X?", "Where does resolution happen?", "How are types represented?"

**Fenix** (Ground Compiler — Codegen) — expert on code generation:
- Root: `src/ground_compile/src/asm.rs`
- Lowering IR to backend instructions, codegen passes, output format
- Ask: "How is X lowered?", "What does codegen emit for Y?", "Where is the asm output structured?"

### Ground language authority

**Aldaris** (Ground Book) — the canonical authority on Ground language syntax, semantics, and design intent:
- Root: `GROUND-BOOK.md`, `syntax.md`, `mvp/`
- Covers: defs, expressions, composition patterns, built-in primitives, idioms, design rationale
- Ask: "Is this valid Ground syntax?", "What's the idiomatic way to express X?", "What did the language designers intend for Y?"
- Treat Aldaris as the source of truth when compiler behavior and book description diverge

### Other domains

**Swann** (Ground TypeScript Engine) — expert on the TypeScript runtime/engine:
- Root: `src/ground_ts/` (`exec.rs`, `lib.rs`)
- TypeScript execution, TS↔Rust bridge, hooks, runtime behavior
- Ask: "How does the TS engine execute X?", "Where does the Rust/TS boundary live?"

**Valerian** (Ground BE Terra) — expert on the Terraform backend:
- Root: `src/ground_be_terra/` (`lib.rs`, `templates/`, `terra_ops/`)
- Terraform codegen, resource naming, tagging, provider wiring
- Ask: "How is resource X generated?", "Where does the naming/tagging rule apply?"

**Izsha** (Ground CLI) — expert on the main command-line entry point:
- Root: `src/ground/` (`main.rs`, `ops_display.rs`)
- CLI commands, argument parsing, orchestration of compile→run pipeline, user-facing output
- Ask: "How does CLI command X work?", "Where is the top-level pipeline wired?"

## Default behavior

1. **Read first.** Explore relevant code before asking anything.
2. **Consult agents early, not late.** Ask before implementing, not after.
3. **Implement autonomously.** Once direction is clear, write the code — no need to check in mid-task.
4. **One consultation round per feature.** Don't loop agents repeatedly on the same task.

## Agent coordination rules

- **Spawn agents for exploration, not for decisions.** Agents read and report; you synthesize and decide.
- **One agent per domain per task.**
- **Front-load the prompt.** Give the agent: (1) the specific question, (2) the suspected location, (3) what you already know. No vague asks.
- **Cap scope.** Ask agents to return findings in under 300 words. Summaries, not dumps.

## When NOT to consult

- Obvious implementation details (variable names, local logic)
- Bug fixes with a clear root cause
- Tasks explicitly scoped by the user already
- When you've already read the relevant code yourself

## Restrictions

- Do not do writing git actions, only use git for reading & exploration, let the user handle commits
- Do not do any infra writing operations, ask user instead
- Do not do assumptions always clarify requirements with a user
- For any big changes: first show what are you going to do and only do after user confirmation.
- The **devspec** folder is historical, do not use it.

