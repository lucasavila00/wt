# AGENTS.md

Behavioral guidelines to reduce common LLM coding mistakes. Merge with project-specific instructions as needed.

**Tradeoff:** These guidelines bias toward caution over speed. For trivial tasks, use judgment.

## 1. Think Before Coding

**Don't assume. Don't hide confusion. Surface tradeoffs.**

Before implementing:
- State your assumptions explicitly. If uncertain, ask.
- If multiple interpretations exist, present them - don't pick silently.
- If a simpler approach exists, say so. Push back when warranted.
- If something is unclear, stop. Name what's confusing. Ask.

## 2. Simplicity First

**Minimum code that solves the problem. Nothing speculative.**

- No features beyond what was asked.
- No abstractions for single-use code.
- No "flexibility" or "configurability" that wasn't requested.
- No error handling for impossible scenarios.
- If you write 200 lines and it could be 50, rewrite it.

Ask yourself: "Would a senior engineer say this is overcomplicated?" If yes, simplify.

## 3. Surgical Changes

**Touch only what you must. Clean up only your own mess.**

When editing existing code:
- Don't "improve" adjacent code, comments, or formatting.
- Don't refactor things that aren't broken.
- Match existing style, even if you'd do it differently.
- If you notice unrelated dead code, mention it - don't delete it.

When your changes create orphans:
- Remove imports/variables/functions that YOUR changes made unused.
- Don't remove pre-existing dead code unless asked.

The test: Every changed line should trace directly to the user's request.

## 4. Goal-Driven Execution

**Define success criteria. Loop until verified.**

Transform tasks into verifiable goals:
- "Add validation" → "Write tests for invalid inputs, then make them pass"
- "Fix the bug" → "Write a test that reproduces it, then make it pass"
- "Refactor X" → "Ensure tests pass before and after"

For multi-step tasks, state a brief plan:
```
1. [Step] → verify: [check]
2. [Step] → verify: [check]
3. [Step] → verify: [check]
```

Strong success criteria let you loop independently. Weak criteria ("make it work") require constant clarification.

---

**These guidelines are working if:** fewer unnecessary changes in diffs, fewer rewrites due to overcomplication, and clarifying questions come before implementation rather than after mistakes.

# Agents

- Read `README.md` and `docs/internals/architecture.md` first.
- If Rust tooling is missing, install stable Rust as the normal user with
  rustup, not from apt:
  `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y`,
  then source `$HOME/.cargo/env` and run
  `rustup component add rustfmt clippy`. Use `sudo apt-get` only for missing
  system prerequisites such as `curl`.
- In a fresh Ubuntu guest, install the repository system prerequisites with
  `sudo apt-get update && sudo apt-get install -y shellcheck libvirt-dev tmux`.
  Use Node.js and npm only through NVM: source `$HOME/.nvm/nvm.sh`, then run
  `nvm install` and `nvm use` using the repository's `.nvmrc` (Node `24.19.0`
  with npm `11.17.0`), and run `npm ci` from the repository root. Do not
  install Node.js or npm with apt. If Cargo compiled `virt-sys`
  before `libvirt-dev` was installed, run `cargo clean -p virt-sys` once so its
  native link metadata is regenerated.
- Current system: Ubuntu 24.04 amd64 servers, local and OpenSSH client contexts, libvirt/KVM, Git access, and SSH access to guests.
- Guest SSH and OpenSSH transport to `wt-server` are in scope; runtime environment overrides and emulation fallback are not.
- Keep `wt-server` slim. Host setup belongs in `wt-server-installer`. Real-system tests belong in `wt-end-to-end-tests`.
- Every golden image includes development tools. KVM E2E reuses the local image
  cache after its first build; do not restore a separate slim E2E image.
- Use Rust for typed validation, state, and lifecycle decisions. Whole-flow POSIX
  shell assets are allowed for guest and server installation procedures.
- Use terminal colors according to their UI meaning: blue for navigation
  highlights such as active markers and selected-card borders, yellow for states
  that need attention, red for errors, and green for successful or healthy
  states. Keep essential meaning in text or symbols; color only reinforces it.
  Text, list, and form selections may reverse terminal defaults for theme-safe
  contrast as established by ADR 0038.
- Match verification to the files changed. Documentation-only changes need no Rust
  checks. For shell scripts, run `bash -n` on the changed scripts and targeted
  behavior checks; run ShellCheck when available.
- For Rust changes, run `cargo fmt --all --check` plus tests and Clippy for the
  affected crates. Use workspace-wide Rust checks only for cross-crate changes or
  when explicitly requested.
- Run `make ci` for the complete pre-merge check. It matches the regular CI job:
  repository static analysis and the workspace test suite. It intentionally
  excludes the separate scheduler-contention stress job and ignored real-system
  KVM tests.
- Prefer Insta snapshots for stable user-visible text, diagnostics, generated
  configuration, scripts and service units, serialized text, and multiline
  output. Snapshot the complete normalized value instead of checking fragments
  with `contains`. Favor inline snapshots for compact expected output. Keep large
  generated artifacts—especially full SSH inventories and long rendered
  configurations—in external `.snap` files when embedding them would dominate
  the test source. Apply that choice consistently based on output size and
  readability, not test location. Keep direct assertions for secrets, status
  codes, parsed semantics, filesystem state, and other behavioral invariants.
  Review generated snapshots and ensure no `.snap.new` or `.pending-snap` files
  remain before finishing.
- Do not run real-system KVM tests for unrelated changes. Run them only when the
  affected behavior requires them or when explicitly requested.
