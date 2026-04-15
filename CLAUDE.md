Ground is an architecture definition language. Infrastructure is derived.

## Agents

**Tassadar** (Ground Compiler Expert) — delegate all exploration and questions about ground syntax:
- Root: `ground_compile/` crate
- Ground language syntax, grammar, parsing, IR, type system
- Compiler passes, codegen pipeline, error reporting
- Ask: "How does X work in ground_compile?" → spawn Tassadar to explore and report back

## Current focus

Our main focus currently is the Ground Book. It was written to sketch the foundational concepts and usage patterns of the Ground language. There is some real infra defined in the `mvp` folder in the Ground language. Both are not yet strict, because we don't have implementation yet in compiler and generation layers. Now our main focus is implementing things fixing the Ground Book & mvp along the way.

## Default behavior

1. **Read first.** Explore relevant code before asking anything.
2. **Consult agents early, not late.** Ask before implementing, not after.
3. **Implement autonomously.** Once direction is clear, write the code — no need to check in mid-task.
4. **One consultation round per feature.** Don't loop zealots repeatedly on the same task.

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

## Process

The **devspec** folder is historical, do not treat it as a source of truth. RFCs in it do not reflect the current state but rather design choices of the past.

The RFC process can be requested by user:

  - Feature is designed in a corresponding devspec/000x-rfc-feature.md: reqs, approach, architecture, tech reqs, libs etc.
  - Be concise and technical, no story telling
  - Discuss and iterate with user on the rfc
  - After rfc is confirmed as finished by the user, create a corresponding devspec/000x-pln-feature.md with implementation plan
  - Iterate with user on the implementation plan
  - After user confirms the plan proceed with the implementation

