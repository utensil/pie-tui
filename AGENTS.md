# AGENTS.md — pie-tui

Rust TUI targeting feature parity with `@earendil-works/pi-tui` (reference pin tracked in
`tools/surface-manifest.json`). Public repo (MIT). `pie` names the pi-compat TUI only
(e = enhanced); future harness-core/plugin work takes harness-derived names, never pie.

## Repo standards

- **Public-clean.** MIT. No personal paths (`/Users/<user>`), tokens, or identity details in
  code, tests, fixtures, or history.
- **Neutral packages.** No crate hardcodes a model, provider, theme, or extension list —
  themes and configuration enter as data at the edges.
- **Boundary (build-enforced via `cargo run -p xt -- boundary`, also run in tests).**
  Layer ranks: core 0 < term 1 < components 2 < app 3 < adapters 4 < xtask 5.
  Depend only strictly downward; adapters never referenced from below; pie-core sources stay
  pure (no sibling references); no host absolute paths anywhere; xtask stays dependency-free.

## Commit discipline

- **git only.** Author AND committer must be the repository owner's GitHub identity
  (`utensil <utensilcandel@gmail.com>`; beware ambient `GIT_AUTHOR_NAME`/`GIT_COMMITTER_NAME`
  overrides — set both explicitly when committing).
- Conventional commits (`feat:`, `fix:`, `refactor:`, `docs:`, `chore:`), title ends `[AGENT]`,
  one logical change per commit, stage only files of that change.
- Target every git command explicitly (remote/branch/path); push with explicit refspec
  (`git push origin dev-rust`); after every push read back the remote tip and require equality.
- Verify before committing: `cargo fmt --check --all`, `cargo clippy --workspace --all-targets`
  (warning-free), `cargo test --workspace`, `cargo run -p xt -- boundary`.

## Verification ladder

1. fmt + clippy + unit/property tests + boundary gate (every commit).
2. TS-oracle golden vectors: fixtures harvested from the pinned pi-tui build (M1+).
3. Differential frame snapshots vs the reference under identical scripted input (M2+).
4. tmux session smoke (`tools/tmux-smoke.sh`): real terminal, capture-pane assertions (M2+,
   before any milestone landing that touches rendering).
- Every parity claim lands as a row in `docs/parity.md` linking its verifying test/receipt.
- New gates get mutated on purpose at authoring time (break the guarded thing; watch it fail).
