# `pie-tui-native`

Milestone M6 closes the authenticated Tier-0 production paths of the private,
repo-owned Node-API compatibility package. It
preserves the complete 69-export runtime namespace and exact 133-name baseline
of the authenticated pi-tui 0.84.2 package, including canonical component,
terminal, and Main/Alt screen classes. It adopts only the authenticated 0.84.4
`setCapabilityOverrides` addition, yielding an actual 70-export / 134-name facade.
It is an installable package alias for the recorded Tier-0 consumer contract;
it is not the upstream package and does not claim compatibility outside the
documented parity ledger and verification corpus.

The Rust core owns text, input/editor, Markdown, loader, StdinBuffer, layout,
key, terminal, and rendering behavior. Thin JavaScript facades retain the
upstream object graph, callback, timer, process-stream, and module-namespace
semantics that cannot cross Node-API by value. The public Rust `pi_tui`
compatibility namespace remains separate from the private ABI in `src/native.rs`.

## Boundary notes

- `renderLatex` and the Kitty/iTerm2 payloads cross Node-API as UTF-16 code
  units, preserving lone surrogates, embedded NULs, and astral pairs. Kitty
  chunks at the reference's 4096 JavaScript-code-unit boundary, including a
  surrogate pair split across chunks. `renderLatex` maps its unsupported-result
  native `null` to public `undefined`.
- Other string inputs entering a Rust `String` are explicitly bounded to
  well-formed UTF-16. Unpaired surrogates throw `RangeError` instead of entering
  a lossy conversion.
- Image IDs, Kitty columns/rows, and utility column counts are guarded as
  unsigned 32-bit integers (`0` through `0xffffffff`). ID `0` is valid:
  `encodeKitty` omits `i=` for it while `deleteKittyImage` emits `i=0`.
  The reference JavaScript interpolates some wider and coercive values; this
  package deliberately makes no compatibility claim outside the documented u32
  domain for these functions. `calculateImageRows` stays adapter-local
  JavaScript so its `Number` coercion and literal 9x18 default remain exact.
- iTerm2 decoded sizes use `Buffer.byteLength(payload, "base64")` in JavaScript;
  the lower Rust approximation remains outside the selected facade.
- Padded truncation preflights JavaScript's maximum string length and raises a
  catchable `RangeError` before a native allocation can abort the process.
- Cell dimensions and temporary `setCapabilities` values retain caller object
  identity. Persistent `setCapabilityOverrides` input is defensively cloned;
  ESM and CommonJS share the same runtime slots.
- Capability detection snapshots every consumed terminal-environment value
  before invoking the optional tmux callback, then passes those immutable facts
  through the private native ABI. Callback-side environment mutations cannot
  rewrite the already selected terminal.
- `fuzzyMatch` lowers strings in JavaScript before passing exact UTF-16 units
  to the synchronous native scorer. `fuzzyFilter` remains a JavaScript loop so
  callback reentrancy, appended-item iteration, stable ordering, and caller
  object identity follow the reference.
- `TUI_KEYBINDINGS` is materialized once from the reviewed Rust table and is
  shared by ESM and CommonJS with the reference's ordinary mutable object and
  array descriptors. `CURSOR_MARKER` is the exact seven-code-unit APC marker.
- Component facades keep reference constructors, prototypes, callbacks, own
  properties, shared mutable graphs, and `instanceof` behavior. Per-instance
  weak registries delegate behavior-bearing state to the reviewed Rust core;
  containers and process/event-loop ownership remain deliberately JavaScript.
- `renderImage`, `imageFallback`, and `compositeTuiLine` retain their
  JavaScript-owned `Number`, path, live capability/cell fact, and callback-free
  sequencing semantics. Raw unpaired surrogates in image payloads and composed
  terminal lines are preserved across the private native boundary.
- The 69-key baseline plus one adopted overlay has the pinned 70-key namespace
  order and exact module descriptors, null prototype, shared ESM/CommonJS
  values, and non-extensible mutation behavior.
- A fresh consumer uses an npm `file:` dependency alias named
  `@earendil-works/pi-tui` and exercises the complete runtime namespace,
  component graphs, callbacks, and Main/Alt screen lifecycle. This does not
  grant publication permission or widen the recorded compatibility envelope.
- The current `dsh-pi-tui-mono` head is also tested through the packed override:
  all 40 front-door tests pass, then a real `InteractiveMode` is constructed,
  initialized, rendered from bridge events, driven through input and resize, and
  stopped with alternate-screen, stdin, and resize-listener cleanup verified.
- Oracle verification requires both the exact 0.84.2 npm tarball and its
  colocated installed `dist`. Before importing the reference it verifies the
  tarball SHA-256 and npm SHA-512 integrity, package identity, exact external
  dependency versions and complete dependency trees, all 156 collision-free
  `dist` files, the full 69-name runtime order/type namespace, and the selected
  behavior-bearing source/type closure.
- Separate drift and semantic gates authenticate the 0.84.2 and 0.84.4 npm
  tarballs. They prove the baseline remains 69/133 and adopt exactly one later
  overlay for an actual 70/134 surface. The overlay defensively clones partial
  overrides, preserves equal-cache identity, invalidates changed values, and
  persists across resets without adopting the other 0.84.4 capability changes.
- A real packed 0.84.4 coding-agent probe links this facade, loads its settings,
  and constructs `InteractiveMode`. Full initialization also needs the separate
  later `TuiAltScreen.setCopyOnSelect` method and is deliberately not claimed.
- The tarball includes this repository's exact MIT `LICENSE`; pack-manifest,
  digest, install, and local-alias tests guard it.
- Native builds pin Rust 1.98.0 and deterministically remap the source tree,
  Cargo home, Rust sysroot, and user home before linking. Verification builds
  from two distinct temporary source roots and requires byte-identical addons;
  it also scans both the build output and installed tarball for source, home,
  Cargo, sysroot, and temporary path leakage.
- The addon carries a relative, stable library identity: Mach-O uses
  `@rpath/pie-tui-native.node`; ELF uses `pie-tui-native.node` as its SONAME
  and has no RPATH or RUNPATH. Causal mutations prove the remap and link
  identity checks fail closed.
- CI builds and loads the native artifact on macOS arm64/x64 and Linux arm64/x64,
  then runs runtime, declaration, pack-consumer, and exact-head live-lifecycle
  receipts on each target.

## Pinned toolchain

- Node.js `24.4.1`
- Rust `1.98.0`
- `napi` `3.12.2`, `napi-derive` `3.6.3`, `napi-build` `2.4.1`
- `@napi-rs/cli` `3.8.6`
- TypeScript `7.0.2`, `@types/node` `24.13.3`, `marked` `18.0.5`

Node 22.19.0 passes the runtime and pack gates but diverges in the authenticated
Alt-screen search oracle. It is therefore not part of the supported floor.

The NAPI-RS layout was checked against the official v3
[package template](https://github.com/napi-rs/package-template/tree/c41e2009b2a2724d593073719f4d2a44058073da)
and [getting-started documentation](https://napi.rs/docs/introduction/getting-started).
Set `PI_TUI_TARBALL` to the pinned 0.84.2 `.tgz` and `PI_TUI_DIST` to its
installed `dist`, then run `npm run verify` here. Verification checks provenance
before importing the oracle, checks the actual ordered selected-export object,
builds the real addon, differentially checks the full namespace plus the
behavior-bearing component matrix against the pinned package on Node 24.4.1,
runs the authenticated 0.84.2 fullscreen ScrollView pane oracle,
runs the authenticated ScrollView selection-geometry oracle (including
visible-pane clipping and wide/combining/ZWJ grapheme endpoints),
runs declaration tests plus causal mutations, installs the tarball into a clean
consumer, and runs the real-PTY NAPI screen smoke through
`tools/tmux-napi-smoke.sh`.
