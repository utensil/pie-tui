# Spec: StdinBuffer — escape-timeout + bracketed-paste semantics

Reference: `@earendil-works/pi-tui@0.84.1` `dist/stdin-buffer.js` (+ `dist/terminal.js`
wiring). Written BEFORE the Rust port per the eval's behavior-spec discipline (§5.1):
the port implements THIS document; tests check THIS document. Any deviation from the
reference found later amends this spec first, then the port.

## 1. Purpose

stdin data events arrive in arbitrary chunks; a partial escape sequence must never be
misread as plain keys (e.g. mouse SGR `\x1b[<35;20;5m` split across 3 events). The
buffer accumulates input and emits only *complete* sequences; an incomplete tail is
held until more input arrives or the escape timeout fires.

Rust modeling note: the buffer itself is pure logic (pie-core). The OS timer that
calls `flush()` after `timeout_ms` of silence lives in the terminal adapter
(pie-term/adapters). Observable behavior is defined as `process(data) -> events`
plus `flush() -> events`; timing only decides WHEN flush runs, never WHAT it emits.

## 2. Sequence completeness (classification of a candidate starting at ESC)

| Shape | Complete when |
|---|---|
| no leading ESC | always (single input char, emitted as-is) |
| `\x1b` alone | never by shape — escapes only via timeout (`flush`) |
| CSI `\x1b[...` | length ≥ 3 AND final byte in 0x40..=0x7E; exceptions below |
| CSI mouse legacy `\x1b[M` | needs ESC + `[M` + 3 more bytes (6 total) |
| CSI SGR mouse `\x1b[<b;x;y[Mm]` | payload matches `^<\d+;\d+;\d+[Mm]$` |
| SGR-mouse-shaped but non-matching payload ending M/m | incomplete (keep buffering) |
| OSC `\x1b]...` | ends with BEL `\x07` or ST `\x1b\\` |
| DCS `\x1bP...` | ends with ST `\x1b\\` |
| APC `\x1b_...` | ends with ST `\x1b\\` |
| SS3 `\x1bO` + one char | length ≥ 4 |
| `\x1b` + single char | complete (meta key) |
| anything else after ESC | complete (unknown — emit as-is) |

## 3. ESC-ESC split rule

When scanning, the candidate `"\x1b\x1b"` would classify as complete (meta). If the
character IMMEDIATELY AFTER the candidate begins a new escape sequence
(`[` CSI, `]` OSC, `O` SS3, `P` DCS, `_` APC), emit only the FIRST `\x1b` and restart
scanning at the second. This keeps WezTerm's raw-ESC-press + kitty-release
concatenation (`"\x1b" + "\x1b[27;...u"`) working: input emits `ESC` as its own
sequence and the CSI-u parses normally.

## 4. Escape timeout

- Default `timeout_ms = 10`. (dsh deployment patch sets `PI_TUI_ESC_TIMEOUT=150`;
  same code path, injected value.)
- After `process()` leaves a non-empty remainder, the runtime schedules exactly one
  `flush()` after `timeout_ms` of silence; new input cancels and re-schedules.
- `flush()` returns the whole remainder as ONE sequence (even if it contains several
  partial shapes) and clears the pending-kitty-printable state.
- Observable: a lone `\x1b` plus silence ⇒ ONE `data("\x1b")` event (the escape key),
  never a swallowed byte.

## 5. Bracketed paste

Markers: start `\x1b[200~`, end `\x1b[201~`.

1. If the buffer contains a start marker: first emit all complete sequences BEFORE
   it (normal parsing), then enter paste mode with everything after the marker.
2. In paste mode, ALL bytes (including escapes/newlines) accumulate verbatim until
   the end marker appears; content before it is emitted as a single `paste(content)`
   event; the remainder (after the end marker) is re-processed through the normal
   path (recursion), including nested paste markers.
3. Entering/leaving paste mode clears the pending-kitty-printable state.
4. Empty content is still a paste event (`paste("")`).
5. The terminal adapter (reference `ProcessTerminal`) re-wraps paste content as one
   input event: `\x1b[200~` + content + `\x1b[201~` — editor components see the
   markers; everything else sees only `data` events.

## 6. Kitty printable-codepoint dedup

Each emitted `data` sequence updates pending state:
`pending = parseUnmodifiedKittyPrintableCodepoint(seq)` — a sequence matching
`^\x1b[(\d+)(?::\d*)?(?::\d+)?u$` whose codepoint ≥ 32 (printable, unmodified).

Before emitting, a single-char `data` sequence whose codepoint equals `pending` is
SWALLOWED (kitty sends press followed by a plain-text duplicate for unmodified
printable keys in some modes). State is cleared by: flush, clear, paste-mode
transitions.

## 7. Lifecycle

- `clear()` empties buffer + paste state + pending codepoint + timer.
- `flush()` on empty buffer emits nothing.
- Adapter wiring (`ProcessTerminal.start`): stdin data → `StdinBuffer.process` →
  `data` events filtered through keyboard-protocol negotiation handling → component
  input handler; `paste` events re-wrapped (§5.5). On stop: buffer destroyed.

## 8. Test vectors (normative)

Input chunks → emitted events (`D(x)` = data, `P(x)` = paste). Timeout column shows
the flush outcome given silence.

| # | chunks | events |
|---|---|---|
| V1 | `"a"`, `"b"` | `D(a)`, `D(b)` |
| V2 | `"\x1b[<35"`, `";20;5m"` | `D(\x1b[<35;20;5m)` |
| V3 | `"\x1b"`, silence | flush: `D(\x1b)` |
| V4 | `"\x1b\x1b[27;1u"` | `D(\x1b)`, `D(\x1b[27;1u)` |
| V5 | `"\x1b[200~hello world\x1b[201~"` | `P(hello world)` |
| V6 | `"x\x1b[200~pasted\x1b[201~y"` | `D(x)`, `P(pasted)`, `D(y)` |
| V7 | `"\x1b[200~ab"`, `"\x1b[201~"` | `P(ab)` (split chunks) |
| V8 | `"\x1b[97u"`, `"a"` | `D(\x1b[97u)` (the `"a"` swallowed: pending=97) |
| V9 | `"\x1b[97u"`, `"b"` | `D(\x1b[97u)`, `D(b)` |
| V10 | `"\x1b[200~a\x1b[201~b"`, flush then `"\x1b[97u"`, `"a"` | `P(a)`, `D(b)`... then swallow applies again |
| V11 | `"\x1bOA"` | `D(\x1bOA)` |
| V12 | `"\x1bOP\x1bOQ"` | `D(\x1bOP)`, `D(\x1bOQ)` |
| V13 | `"\x1b[M !!#"` (old mouse + extra) | `D(\x1b[M !!)`, `D(#)` — ESC[M + 3 bytes = 6 TOTAL, the rest re-parses |
| V14 | `"\x1b]8;;url\x07"` | `D(\x1b]8;;url\x07)` |
| V15 | `"\x1bP>|xterm\x1b\\"` | `D(\x1bP>|xterm\x1b\\)` |
| V16 | `"\x1bG..."` (unknown) | `D(\x1bG)` then rest char-wise (unknown ⇒ complete at 2 chars) |
| V17 | `"\x1b[200~"`, `"\x1b[201~"` | `P()` (empty paste) |
| V18 | `"\x1b[<1;2"`, silence | flush: `D(\x1b[<1;2)` (SGR-mouse-shaped, incomplete ⇒ one sequence) |

V1–V18 are executed by `crates/pie-core/tests/stdin_buffer_spec.rs` against the Rust
port AND by `tools/golden/gen-golden-stdin.mjs` against the reference build; both
fixture outputs must agree (differential check runs in CI via the fixture file).

## 9. Out of scope (documented seams)

- Node Buffer high-byte conversion (`byte > 127` → `ESC + chr(byte-128)`) is a
  Node-stdin decoding artifact; the Rust adapter decodes bytes to UTF-8 losslessly
  and feeds `&str` (documented, not ported).
- Keyboard-protocol negotiation (kitty flags / DA responses, 150 ms fragment timer)
  lives ABOVE the buffer in `ProcessTerminal`; spec'd with the terminal layer, not here.
