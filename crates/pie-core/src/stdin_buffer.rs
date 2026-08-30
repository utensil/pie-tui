//! StdinBuffer — buffers input and emits complete escape sequences.
//!
//! Implements `docs/specs/stdin-buffer.md` (a faithful port of the pinned
//! pi-tui `dist/stdin-buffer.js`). The OS timer that calls [`StdinBuffer::flush`]
//! after a silence timeout lives in the terminal adapter; timing decides WHEN a
//! flush happens, never WHAT it emits (spec §1/§4).

/// One emitted event: a complete input sequence, or a bracketed paste payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StdinEvent {
    /// A complete input sequence ready for the input handler.
    Data(String),
    /// Content pasted between bracketed-paste markers (markers stripped).
    Paste(String),
}

const ESC: char = '\x1b';
const BRACKETED_PASTE_START: &str = "\x1b[200~";
const BRACKETED_PASTE_END: &str = "\x1b[201~";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Completeness {
    Complete,
    Incomplete,
    NotEscape,
}

/// Status of a candidate sequence starting at ESC.
fn is_complete_sequence(data: &str) -> Completeness {
    if !data.starts_with(ESC) {
        return Completeness::NotEscape;
    }
    if data.chars().count() == 1 {
        return Completeness::Incomplete;
    }
    let after_esc = &data[1..];
    // CSI sequences: ESC [
    if let Some(rest) = after_esc.strip_prefix('[') {
        let _ = rest;
        if after_esc.starts_with("[M") {
            // Old-style mouse: ESC[M + 3 bytes = 6 total.
            return if data.len() >= 6 {
                Completeness::Complete
            } else {
                Completeness::Incomplete
            };
        }
        return is_complete_csi_sequence(data);
    }
    // OSC sequences: ESC ]
    if after_esc.starts_with(']') {
        return is_complete_osc_sequence(data);
    }
    // DCS sequences: ESC P ... ESC \
    if after_esc.starts_with('P') {
        return is_complete_dcs_sequence(data);
    }
    // APC sequences: ESC _ ... ESC \
    if after_esc.starts_with('_') {
        return is_complete_apc_sequence(data);
    }
    // SS3 sequences: ESC O + one char.
    if after_esc.starts_with('O') {
        return if after_esc.chars().count() >= 2 {
            Completeness::Complete
        } else {
            Completeness::Incomplete
        };
    }
    // Meta key (ESC + single char) or unknown — complete.
    Completeness::Complete
}

/// CSI: ESC [ ... final byte 0x40..=0x7E, with SGR/legacy mouse special cases.
fn is_complete_csi_sequence(data: &str) -> Completeness {
    if !data.starts_with("\x1b[") {
        return Completeness::Complete;
    }
    if data.len() < 3 {
        return Completeness::Incomplete;
    }
    let payload = &data[2..];
    let Some(last_char) = payload.chars().last() else {
        return Completeness::Incomplete;
    };
    let last_char_code = last_char as u32;
    if (0x40..=0x7e).contains(&last_char_code) {
        if payload.starts_with('<') {
            // SGR mouse: <digits;digits;digits[Mm]
            if is_sgr_mouse_payload(payload) {
                return Completeness::Complete;
            }
            if last_char == 'M' || last_char == 'm' {
                let inner = &payload[1..payload.len() - 1];
                let parts: Vec<&str> = inner.split(';').collect();
                if parts.len() == 3
                    && parts
                        .iter()
                        .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
                {
                    return Completeness::Complete;
                }
            }
            return Completeness::Incomplete;
        }
        return Completeness::Complete;
    }
    Completeness::Incomplete
}

/// Exact `^<\d+;\d+;\d+[Mm]$` test (the reference's first, regex-based check).
fn is_sgr_mouse_payload(payload: &str) -> bool {
    let body = match payload.strip_prefix('<') {
        Some(b) => b,
        None => return false,
    };
    let (nums, terminator) = match body.char_indices().last() {
        Some((idx, ch)) if ch == 'M' || ch == 'm' => (&body[..idx], ch),
        _ => return false,
    };
    let mut parts = nums.split(';');
    for _ in 0..3 {
        match parts.next() {
            Some(p) if !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()) => {}
            _ => return false,
        }
    }
    if parts.next().is_some() {
        return false;
    }
    // Matched the strict pattern — the reference returns "complete" here
    // regardless of the M/m-vs-shape fallback below.
    let _ = terminator;
    true
}

fn is_complete_osc_sequence(data: &str) -> Completeness {
    if !data.starts_with("\x1b]") {
        return Completeness::Complete;
    }
    if data.ends_with("\x1b\\") || data.ends_with('\x07') {
        Completeness::Complete
    } else {
        Completeness::Incomplete
    }
}

fn is_complete_dcs_sequence(data: &str) -> Completeness {
    if !data.starts_with("\x1bP") {
        return Completeness::Complete;
    }
    if data.ends_with("\x1b\\") {
        Completeness::Complete
    } else {
        Completeness::Incomplete
    }
}

fn is_complete_apc_sequence(data: &str) -> Completeness {
    if !data.starts_with("\x1b_") {
        return Completeness::Complete;
    }
    if data.ends_with("\x1b\\") {
        Completeness::Complete
    } else {
        Completeness::Incomplete
    }
}

/// If `sequence` is an unmodified printable Kitty CSI-u codepoint (`^\x1b[(\d+)
/// (?::\d*)?(?::\d+)?u$`, codepoint ≥ 32), return it (spec §6).
fn parse_unmodified_kitty_printable_codepoint(sequence: &str) -> Option<u32> {
    let rest = sequence.strip_prefix("\x1b[")?;
    let rest = rest.strip_suffix('u')?;
    // codepoint, optional `:`, optional alternate, optional modifiers — the
    // reference regex allows `\d+` `(?::\d*)?` `(?::\d+)?` before `u`.
    let mut it = rest.splitn(4, ':');
    let cp_part = it.next()?;
    let opt1 = it.next();
    let opt2 = it.next();
    if it.next().is_some() {
        return None;
    }
    if let Some(o) = opt1
        && !o.is_empty()
        && !o.bytes().all(|b| b.is_ascii_digit())
    {
        return None;
    }
    if let Some(o) = opt2
        && (o.is_empty() || !o.bytes().all(|b| b.is_ascii_digit()))
    {
        return None;
    }
    if cp_part.is_empty() || !cp_part.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let codepoint: u32 = cp_part.parse().ok()?;
    if codepoint >= 32 {
        Some(codepoint)
    } else {
        None
    }
}

/// Extracts complete sequences from `buffer`; returns them plus the remainder.
fn extract_complete_sequences(buffer: &str) -> (Vec<String>, String) {
    let mut sequences: Vec<String> = Vec::new();
    let mut pos = 0usize;
    'outer: while pos < buffer.len() {
        let remaining = &buffer[pos..];
        if remaining.starts_with(ESC) {
            let mut unit_end = 1usize; // char count (UTF-16-equivalent scanning on chars)
            let total_units = remaining.chars().count();
            while unit_end <= total_units {
                let candidate: String = remaining.chars().take(unit_end).collect();
                match is_complete_sequence(&candidate) {
                    Completeness::Complete => {
                        // ESC-ESC split rule (spec §3): if the next char would
                        // begin a new escape sequence, emit the first ESC alone.
                        if candidate == "\x1b\x1b"
                            && let Some(next_char) = remaining.chars().nth(unit_end)
                            && matches!(next_char, '[' | ']' | 'O' | 'P' | '_')
                        {
                            sequences.push(ESC.to_string());
                            pos += 1;
                            continue 'outer;
                        }
                        pos += candidate.len();
                        sequences.push(candidate);
                        continue 'outer;
                    }
                    Completeness::Incomplete => unit_end += 1,
                    Completeness::NotEscape => {
                        pos += candidate.len();
                        sequences.push(candidate);
                        continue 'outer;
                    }
                }
            }
            if unit_end > total_units {
                return (sequences, remaining.to_string());
            }
        } else {
            // Not an escape — a single input char.
            let ch = remaining.chars().next().unwrap();
            sequences.push(ch.to_string());
            pos += ch.len_utf8();
        }
    }
    (sequences, String::new())
}

/// Buffers stdin input and emits complete sequences (spec: docs/specs/stdin-buffer.md).
#[derive(Debug)]
pub struct StdinBuffer {
    buffer: String,
    timeout_ms: u64,
    paste_mode: bool,
    paste_buffer: String,
    pending_kitty_printable_codepoint: Option<u32>,
}

impl Default for StdinBuffer {
    fn default() -> Self {
        Self::new(10)
    }
}

impl StdinBuffer {
    /// `timeout_ms` mirrors the reference constructor option (default 10;
    /// deployments may raise it, e.g. dsh's 150 ms ESC timeout).
    pub fn new(timeout_ms: u64) -> Self {
        StdinBuffer {
            buffer: String::new(),
            timeout_ms,
            paste_mode: false,
            paste_buffer: String::new(),
            pending_kitty_printable_codepoint: None,
        }
    }

    pub fn timeout_ms(&self) -> u64 {
        self.timeout_ms
    }

    /// Feed one input chunk; returns synchronously-emitted events.
    pub fn process(&mut self, data: &str) -> Vec<StdinEvent> {
        let mut events = Vec::new();
        self.process_into(data, &mut events);
        events
    }

    fn process_into(&mut self, data: &str, events: &mut Vec<StdinEvent>) {
        if data.is_empty() && self.buffer.is_empty() {
            self.emit_data_sequence(String::new(), events);
            return;
        }
        self.buffer.push_str(data);

        if self.paste_mode {
            self.paste_buffer
                .push_str(&std::mem::take(&mut self.buffer));
            if let Some(end_index) = self.paste_buffer.find(BRACKETED_PASTE_END) {
                let pasted = self.paste_buffer[..end_index].to_string();
                let remaining =
                    self.paste_buffer[end_index + BRACKETED_PASTE_END.len()..].to_string();
                self.paste_mode = false;
                self.paste_buffer.clear();
                self.pending_kitty_printable_codepoint = None;
                events.push(StdinEvent::Paste(pasted));
                if !remaining.is_empty() {
                    self.process_into(&remaining, events);
                }
            }
            return;
        }

        if let Some(start_index) = self.buffer.find(BRACKETED_PASTE_START) {
            if start_index > 0 {
                let before = self.buffer[..start_index].to_string();
                let (sequences, _) = extract_complete_sequences(&before);
                for sequence in sequences {
                    self.emit_data_sequence(sequence, events);
                }
            }
            self.pending_kitty_printable_codepoint = None;
            self.buffer = self.buffer[start_index + BRACKETED_PASTE_START.len()..].to_string();
            self.paste_mode = true;
            self.paste_buffer = std::mem::take(&mut self.buffer);
            if let Some(end_index) = self.paste_buffer.find(BRACKETED_PASTE_END) {
                let pasted = self.paste_buffer[..end_index].to_string();
                let remaining =
                    self.paste_buffer[end_index + BRACKETED_PASTE_END.len()..].to_string();
                self.paste_mode = false;
                self.paste_buffer.clear();
                self.pending_kitty_printable_codepoint = None;
                events.push(StdinEvent::Paste(pasted));
                if !remaining.is_empty() {
                    self.process_into(&remaining, events);
                }
            }
            return;
        }

        let (sequences, remainder) = extract_complete_sequences(&self.buffer);
        self.buffer = remainder;
        for sequence in sequences {
            self.emit_data_sequence(sequence, events);
        }
    }

    /// Runtime-driven timeout flush: the whole remainder as ONE sequence
    /// (spec §4). Returns events to emit.
    pub fn flush(&mut self) -> Vec<StdinEvent> {
        if self.buffer.is_empty() {
            return Vec::new();
        }
        let sequence = std::mem::take(&mut self.buffer);
        self.pending_kitty_printable_codepoint = None;
        vec![StdinEvent::Data(sequence)]
    }

    /// Clear buffer, paste state, pending codepoint (and the runtime timer).
    pub fn clear(&mut self) {
        self.buffer.clear();
        self.paste_mode = false;
        self.paste_buffer.clear();
        self.pending_kitty_printable_codepoint = None;
    }

    pub fn get_buffer(&self) -> &str {
        &self.buffer
    }

    /// Emit with kitty printable dedup (spec §6): a single-char sequence whose
    /// codepoint equals the pending value is swallowed silently.
    fn emit_data_sequence(&mut self, sequence: String, events: &mut Vec<StdinEvent>) {
        let raw_codepoint = if sequence.chars().count() == 1 {
            sequence.chars().next().map(|c| c as u32)
        } else {
            None
        };
        if raw_codepoint.is_some() && raw_codepoint == self.pending_kitty_printable_codepoint {
            self.pending_kitty_printable_codepoint = None;
            return; // swallowed
        }
        self.pending_kitty_printable_codepoint =
            parse_unmodified_kitty_printable_codepoint(&sequence);
        events.push(StdinEvent::Data(sequence));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kitty_printable_parser_shapes() {
        assert_eq!(
            parse_unmodified_kitty_printable_codepoint("\x1b[97u"),
            Some(97)
        );
        assert_eq!(
            parse_unmodified_kitty_printable_codepoint("\x1b[97:65u"),
            Some(97)
        );
        assert_eq!(
            parse_unmodified_kitty_printable_codepoint("\x1b[97::65u"),
            Some(97)
        );
        assert_eq!(
            parse_unmodified_kitty_printable_codepoint("\x1b[97;2u"),
            None
        );
        assert_eq!(
            parse_unmodified_kitty_printable_codepoint("\x1b[27u"),
            None // 27 < 32: not printable
        );
        assert_eq!(parse_unmodified_kitty_printable_codepoint("\x1b[31u"), None); // < 32
    }

    #[test]
    fn sgr_mouse_payload_shapes() {
        assert!(is_sgr_mouse_payload("<35;20;5M"));
        assert!(is_sgr_mouse_payload("<0;0;0m"));
        assert!(!is_sgr_mouse_payload("<35;20M"));
        assert!(!is_sgr_mouse_payload("<a;20;5M"));
    }
}
