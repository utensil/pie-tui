# pie-tui

`pie-tui` is a Rust port targeting behavioral and public-API
compatibility with [`@earendil-works/pi-tui`](https://github.com/earendil-works/pi).
The public package contract is pinned to version 0.84.2; differential fixtures and an exhaustive
API ledger keep compatibility claims tied to evidence.

> **Status:** M0–M6 are complete for the authenticated pi-tui 0.84.2 Tier-0
> production-behavior envelope. The private package keeps all 69 canonical 0.84.2 runtime exports and
> 133 baseline type names, then adopts the single authenticated 0.84.4
> `setCapabilityOverrides` overlay for an actual 70-export / 134-name facade; production ProcessTerminal and
> native-planned Main/Alt screen facades pass semantic-oracle, mutation, and real-tmux
> lifecycle gates. A clean `dsh-pi-tui-mono` override at `c59fd5d` passes all 40
> front-door tests plus a real packed `InteractiveMode` render/input/resize/teardown
> lifecycle on macOS and Linux, arm64 and x64.

Canonical API ledger: 88/133 ported or verified, 40/133 behavior-verified, 106 compiled mappings; M3 target 80, gap 0.

The detailed evidence and known gaps live in the [parity ledger](docs/parity.md).
The [work queue](docs/roadmap.md) is the source of truth for milestone state.

## Milestones

| Milestone | State | Scope |
|---|---|---|
| M0 | Complete | Workspace scaffold, dependency-boundary gate, CI, and reference surface manifest. |
| M1a–M1c | Complete | Reference-harvested text, key, ANSI wrapping, truncation, slicing, grapheme, and OSC 8 behavior. |
| M2 | Complete for renderer scope | Terminal trait and recorder, byte-differential main-screen renderer, synchronized output, and live tmux smoke. The full TUI runtime was deferred to M5 and is now closed by the package acceptance gates. |
| M3 | Complete | Component waves, fuzzy matching, keybindings, staged direct-import contract, and the exhaustive canonical API ledger; reviewed M5 foundation mappings raised the receipt above its 80-symbol target. |
| M4.0 | Complete | Sealed pure frame/diff state, transactional terminal byte planning/execution, and app-owned main-screen control with the legacy renderer contract preserved. |
| M4 | Complete | Editor/Input state and lifecycle, kill ring and undo, marked-backed Markdown, and an exact raw-UTF-16 LaTeX compatibility boundary. Canonical Editor/Input/Markdown interfaces remain honestly partial where M5 owns host/runtime seams. |
| M5 | Complete | Full 0.84.2 Node-API baseline, production ProcessTerminal and canonical Main/Alt facades, authenticated drift receipt, clean package and current pi-dsh consumer checks, plus teardown-oriented real-tmux coverage. The later bounded addition is explicit: baseline 69/133 + one adopted overlay = actual 70/134. |
| M6 | Complete | Authenticated full-tree 0.84.2 contract, terminal/TuiBase semantics, native-planned Main/Alt transactions, decisive mutations, a real packed-consumer `InteractiveMode` lifecycle, and exact-SHA macOS/Linux arm64/x64 CI. |

## Available now

The workspace is intentionally layered:

- `pie-core` contains pure ANSI-aware text measurement and wrapping, key parsing and
  matching, fuzzy matching, keybinding state, editor state/undo/word-navigation
  models, the exact raw-UTF-16 LaTeX renderer, screen composition, the sealed
  `LogicalFrame`/`FrameDiff` model, `StdinBuffer`, terminal-color parsers, and
  deterministic terminal-image codecs/dimension/layout helpers.
- `pie-term` provides the terminal interface, an in-memory `TestRecorder`, keyboard
  protocol helpers, transactional ANSI render plans, the byte-differential
  main-screen renderer compatibility wrapper, capability detection/cache, and a
  backend-injected `ProcessTerminal` lifecycle state machine.
- `pie-components` provides the current Rust component wave: `Text`,
  `TruncatedText`, `Spacer`, `Box`, `VStack`, `HStack`, `Loader`, `SelectList`,
  `SettingsList`, `Editor`, `Input`, marked-backed `Markdown`, `Image`, vertical
  `ScrollView`, cached layout primitives, autocomplete, cancellation, and container
  primitives. Some public shapes and lifecycle details remain partial; see the
  parity ledger before relying on drop-in equivalence.
- `pie-app` owns `MainScreenController`, the shared deterministic
  `TuiBaseController` lifecycle with an injected `TuiControllerHost`, reviewed
  injected Rust `TuiMainScreen` and `TuiAltScreen` controllers, the deterministic
  golden runner, and real-terminal demos for the proven renderer and screen paths.
  `TuiBaseController` remains screen-agnostic; the Node adapter supplies the
  process-stream/event-loop ownership and canonical JavaScript screen classes.
- `pie-napi` contains both the compiled Rust compatibility namespace used by the
  API contract tests and the private dual ESM/CommonJS `pie-tui-native` Node-API
  package. The package preserves the reviewed ordered 69/69 runtime and
  133-name TypeScript baseline, then adds only the authenticated 0.84.4
  `setCapabilityOverrides` overlay for an actual 70-export / 134-name facade, with native-backed behavioral state and canonical
  JavaScript identity seams, and reproducible native artifacts. It is a clean
  alias for the recorded consumer contract, not a claim beyond the parity ledger.

## Try it

The repository pins Rust 1.98 in `rust-toolchain.toml`.

```sh
cargo test --workspace
cargo run -p pie-app --bin pie-demo
```

In the demo, use `space` or `j` to increment, `k` to decrement, and `q` to quit.
It is a renderer/input smoke driver, not the finished pi-tui runtime.

## Verification

Run the normal local gate before committing:

```sh
cargo fmt --check --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo run -p xt -- boundary
node tools/check-coverage.mjs
```

The private Node-API package has its own full oracle, artifact, runtime,
TypeScript, drift, pack-consumer, and mutation gate. It requires authenticated
0.84.1, 0.84.2, and 0.84.4 tarballs plus the pinned 0.84.2 installed `dist`
described in `adapters/pie-napi/README.md`:

```sh
npm --prefix adapters/pie-napi run verify
```

The M6 acceptance scripts exercise the production Node host and the current
pi-dsh front door, including a real packed `InteractiveMode` lifecycle:

```sh
tools/tmux-napi-smoke.sh
bash tools/check-current-dsh-consumer.sh
```

The shared TuiBase controller has a pinned oracle and a separate decisive
mutation gate:

```sh
PI_TUI_DIST=/path/to/pinned-0.84.1/dist node tools/golden/gen-golden-tui-controller.mjs --check
PI_TUI_DIST=/path/to/pinned-0.84.1/dist tools/check-tui-controller-mutations.sh
```

The injected Rust Main/Alt controllers have their own pinned oracle, focused
regression receipts, fresh-consumer gate, and real-terminal smoke:

```sh
PI_TUI_DIST=/path/to/pinned-0.84.1/dist node tools/golden/gen-golden-main-alt-controller.mjs --check
PI_TUI_DIST=/path/to/pinned-0.84.1/dist tools/check-main-alt-oracle-mutations.sh
tools/check-main-alt-product-mutations.sh
bash tools/check-fresh-pie-app-consumer.sh
cargo build -p pie-app --bin pie-screen-demo
tools/tmux-main-alt-smoke.sh
```

For the real-terminal renderer smoke (when `tmux` is installed):

```sh
cargo build -p pie-app
tools/tmux-smoke.sh
```

The ordinary coverage command validates the canonical manifest, exhaustive ledger,
compiled Rust contract, staged imports, and this README's summary numbers. The
historic M3 target remains an independently checked receipt:

```sh
node tools/check-coverage.mjs --milestone M3
```

At this checkpoint that command passes at 88/80. The ordinary gate independently
protects the same reviewed 88-symbol Rust-mapping floor. M6 completion is
independently established by the authenticated semantic/mutation corpus, full live
consumer lifecycle, and exact-SHA four-target CI.

## Current limits

- Direct Rust Editor/Input default word navigation retains a measured 1,439-case
  Thai-adjacent ICU77/ICU4X residual outside the JavaScript host-Intl seam.
- Rust `Markdown` cannot represent canonical output strings containing unpaired
  UTF-16 units or JavaScript Array identity; the raw LaTeX compatibility core does
  preserve the exact unit domain for the future binding.
- Direct Rust `ProcessTerminal`, `ScrollView`, and `Image` continue to use
  injected host facts or backends; the M5 JavaScript facade owns the process,
  timer, object/Array identity, and terminal deletion seams.
- `calculateImageRows` defaults omitted cell dimensions to the canonical literal
  9x18 values. Mutable global cell-dimension access is still needed by
  `renderImage`; the private selected package now covers random image-ID and
  global cell-dimension operations through the canonical package facade.
- The integrated `TuiBaseController` remains a deterministic injected-host Rust
  foundation. The adapter now supplies production host/event-loop wiring,
  canonical screen classes, and current-consumer execution.
- The private `pie-tui-native` package preserves the authenticated 0.84.2 baseline 69/133, plus one adopted 0.84.4 overlay, for an actual 70/134 runtime/type facade. Its drop-in claim remains bounded to that recorded Tier-0 corpus.
- The supported Node floor remains 24.4.1. Node 22.19.0 passes the runtime and
  pack gates but diverges in the authenticated Alt-screen search oracle, so it is
  tested evidence rather than an advertised compatibility claim.
- Current keybinding compatibility does not expose the canonical manager return
  shape and full method/config identity semantics.
- Locale-dependent collation and dictionary word segmentation outside the pinned
  differential corpus remain documented compatibility debt.
- All 14 downstream compatibility imports now have compiled mappings and the former
  four-name M5 deferral allowlist is empty. Selected private-package runtime and
  clean-consumer and full canonical-package execution are proved.

Do not infer parity from a compiled Rust mapping alone. The disposition and evidence
for every canonical symbol are recorded in `tools/surface-coverage.json` and
summarized in [docs/parity.md](docs/parity.md).

## Milestone README contract

README reconciliation is required for every milestone checkpoint. In the same
milestone change:

1. Update the status callout and milestone table to distinguish implemented,
   verified, partial, and planned work.
2. Reconcile the available features, commands, and current limits with the code and
   [roadmap](docs/roadmap.md).
3. Reconcile the canonical metrics sentence with `tools/surface-coverage.json`.
   `node tools/check-coverage.mjs` derives those values from every ledger row and
   fails if the README numbers are stale.
4. Keep the active milestone visibly in progress until its own runtime and package
   receipts pass, even when an earlier numeric target is already green.
