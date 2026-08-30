//! Keyboard parsing/matching — a 1:1 port of @earendil-works/pi-tui keys.js:
//! legacy sequences, xterm modifyOtherKeys, Kitty CSI-u/arrow/functional forms,
//! release/repeat detection, printable decoding.
//! Tables: mechanically extracted into `keys_tables.rs` (regen at pin bumps).

use std::sync::atomic::{AtomicBool, Ordering};

use crate::keys_tables::{
    ARROW_CODEPOINTS, CODEPOINTS, FUNCTIONAL_CODEPOINTS, KITTY_FUNCTIONAL_EQUIVALENTS,
    LEGACY_CTRL_SEQUENCES, LEGACY_KEY_SEQUENCES, LEGACY_SEQUENCE_KEY_IDS, LEGACY_SHIFT_SEQUENCES,
    LOCK_MASK, MOD_ALT, MOD_CTRL, MOD_SHIFT, MOD_SUPER, SYMBOL_KEYS,
};

static KITTY_PROTOCOL_ACTIVE: AtomicBool = AtomicBool::new(false);

pub fn set_kitty_protocol_active(active: bool) {
    KITTY_PROTOCOL_ACTIVE.store(active, Ordering::SeqCst);
}
pub fn is_kitty_protocol_active() -> bool {
    KITTY_PROTOCOL_ACTIVE.load(Ordering::SeqCst)
}

/// Event type carried by a Kitty CSI-u sequence (flag 2), Press otherwise.
pub fn kitty_event_type(data: &str) -> KeyEventType {
    parse_kitty_sequence(data)
        .map(|k| k.event_type)
        .unwrap_or(KeyEventType::Press)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyEventType {
    Press,
    Repeat,
    Release,
}

fn lookup(table: &[(&str, i32)], name: &str) -> Option<i32> {
    table.iter().find(|(k, _)| *k == name).map(|(_, v)| *v)
}

fn codepoint_of(name: &str) -> i32 {
    lookup(CODEPOINTS, name).expect("codepoints table")
}

fn legacy_key_sequences(name: &str) -> &[&str] {
    legacy_lookup(LEGACY_KEY_SEQUENCES, name)
}
fn legacy_shift_sequences(name: &str) -> &[&str] {
    legacy_lookup(LEGACY_SHIFT_SEQUENCES, name)
}
fn legacy_ctrl_sequences(name: &str) -> &[&str] {
    legacy_lookup(LEGACY_CTRL_SEQUENCES, name)
}
fn legacy_lookup<'a>(table: &'a [(&'a str, &'a [&'a str])], name: &str) -> &'a [&'a str] {
    table
        .iter()
        .find(|(k, _)| *k == name)
        .map(|(_, v)| *v)
        .unwrap_or(&[])
}

fn normalize_kitty_functional_codepoint(codepoint: i32) -> i32 {
    KITTY_FUNCTIONAL_EQUIVALENTS
        .iter()
        .find(|(k, _)| *k == codepoint)
        .map(|(_, v)| *v)
        .unwrap_or(codepoint)
}

fn normalize_shifted_letter_identity(codepoint: i32, modifier: i32) -> i32 {
    let effective = modifier & !LOCK_MASK;
    if (effective & MOD_SHIFT) != 0 && (65..=90).contains(&codepoint) {
        return codepoint + 32;
    }
    codepoint
}

/// ctrl formula: lowercase code & 0x1f for letters and [\]_ ; `-` maps to `_`.
fn raw_ctrl_char(key: &str) -> Option<char> {
    let mut it = key.chars();
    let ch = it.next()?;
    if it.next().is_some() {
        return None;
    }
    let lower = ch.to_ascii_lowercase();
    if lower.is_ascii_lowercase() || matches!(lower, '[' | '\\' | ']' | '_') {
        char::from_u32((lower as u32) & 0x1f)
    } else if lower == '-' {
        Some('\u{1f}')
    } else {
        None
    }
}

fn is_digit_str(key: &str) -> bool {
    key.len() == 1 && key.as_bytes()[0].is_ascii_digit()
}

// ---------------------------------------------------------------------------
// Sequence parsers
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy)]
struct KittySeq {
    codepoint: i32,
    shifted_key: Option<i32>,
    base_layout_key: Option<i32>,
    modifier: i32,
    event_type: KeyEventType,
}

#[derive(Debug, Clone, Copy)]
struct ModifyOtherKeysSeq {
    codepoint: i32,
    modifier: i32,
}

fn parse_event_type(event_type_str: Option<&str>) -> KeyEventType {
    match event_type_str.and_then(|s| s.parse::<i32>().ok()) {
        Some(2) => KeyEventType::Repeat,
        Some(3) => KeyEventType::Release,
        _ => KeyEventType::Press,
    }
}

fn is_digits(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
}

/// CSI-u form only (`ESC [ <cp>[:<shifted>[:<base>]][;<mod>[:<event>]] u`).
fn parse_kitty_csi_u_form(data: &str) -> Option<KittySeq> {
    let body = data.strip_prefix("\u{1b}[")?.strip_suffix('u')?;
    let (main_part, mod_part) = match body.split_once(';') {
        Some((m, r)) => {
            if r.contains(';') {
                return None;
            }
            (m, Some(r))
        }
        None => (body, None),
    };
    let mut main_it = main_part.split(':');
    let cp_s = main_it.next()?;
    if !is_digits(cp_s) {
        return None;
    }
    let codepoint: i32 = cp_s.parse().ok()?;
    let shifted_s = main_it.next();
    let base_layout_key = match main_it.next() {
        Some("") | None => None,
        Some(b) if is_digits(b) => Some(b.parse().unwrap()),
        Some(_) => return None,
    };
    let shifted_key = match shifted_s {
        Some("") | None => None,
        Some(s2) if is_digits(s2) => Some(s2.parse().unwrap()),
        Some(_) => return None,
    };
    if main_it.next().is_some() {
        return None;
    }
    let (mod_value, event_type) = match mod_part {
        None => (1i32, KeyEventType::Press),
        Some(mp) => {
            // Reference regexes for mod/event: `(\d+)` then `:(\d*)`; an EMPTY event
            // slot is accepted by \d* and parseInt("") → NaN → Press.
            let mut mit = mp.splitn(2, ':');
            let m = mit.next().unwrap_or("");
            if !is_digits(m) {
                return None;
            }
            if let Some(e) = mit.next().filter(|x| x.contains(':')) {
                let _ = e;
                return None;
            }
            let ev = mit.next();
            (m.parse::<i32>().ok()?, parse_event_type(ev))
        }
    };
    Some(KittySeq {
        codepoint,
        shifted_key,
        base_layout_key,
        modifier: mod_value - 1,
        event_type,
    })
}

/// Unified parseKittySequence(): tries CSI-u, arrow (ESC[1;<m>[ABCD]), functional
/// (`ESC[<n>[;<m>][:<e>]~`) mapping 2/3/5/6/7/8 to functional codepoints, and the
/// `ESC[1;<m>H/F` home/end form.
fn parse_kitty_sequence(data: &str) -> Option<KittySeq> {
    if let Some(seq) = parse_kitty_csi_u_form(data) {
        return Some(seq);
    }
    // ESC[1;<mod>[...]<term> family: arrows [ABCD], home/end [HF]
    if let Some(rest) = data.strip_prefix("\u{1b}[1;") {
        let last = rest.chars().last()?;
        let nums = &rest[..rest.len() - last.len_utf8()];
        let mut it = nums.split(':');
        let m = it.next()?;
        if !is_digits(m) || it.clone().count() > 1 {
            return None;
        }
        let ev = it.next();
        if ev.is_some_and(|e| !e.bytes().all(|b| b.is_ascii_digit()) || e.contains(':')) {
            return None;
        }
        let mod_value: i32 = m.parse().ok()?;
        let event_type = parse_event_type(ev);
        let codepoint = match last {
            'A' => -1,
            'B' => -2,
            'C' => -3,
            'D' => -4,
            'H' => lookup(FUNCTIONAL_CODEPOINTS, "home")?,
            'F' => lookup(FUNCTIONAL_CODEPOINTS, "end")?,
            _ => return None,
        };
        return Some(KittySeq {
            codepoint,
            shifted_key: None,
            base_layout_key: None,
            modifier: mod_value - 1,
            event_type,
        });
    }
    // Functional `~` form
    if let Some(body) = data
        .strip_prefix("\u{1b}[")
        .and_then(|r| r.strip_suffix('~'))
    {
        let (num_s, second) = match body.split_once(';') {
            Some((n, r)) => (n, Some(r)),
            None => (body, None),
        };
        if !is_digits(num_s) {
            return None;
        }
        let key_num: i32 = num_s.parse().ok()?;
        let func_codes: [(i32, i32); 6] = [
            (2, lookup(FUNCTIONAL_CODEPOINTS, "insert").unwrap()),
            (3, lookup(FUNCTIONAL_CODEPOINTS, "delete").unwrap()),
            (5, lookup(FUNCTIONAL_CODEPOINTS, "pageUp").unwrap()),
            (6, lookup(FUNCTIONAL_CODEPOINTS, "pageDown").unwrap()),
            (7, lookup(FUNCTIONAL_CODEPOINTS, "home").unwrap()),
            (8, lookup(FUNCTIONAL_CODEPOINTS, "end").unwrap()),
        ];
        let mapped = func_codes
            .iter()
            .find(|(n, _)| *n == key_num)
            .map(|(_, v)| *v);
        if let Some(codepoint) = mapped {
            let (mod_value, event_type) = match second {
                None => (1i32, KeyEventType::Press),
                Some(s2) => {
                    let mut it = s2.split(':');
                    let m = it.next().unwrap_or("");
                    let ev = it.next();
                    if it.next().is_some() || !is_digits(m) {
                        return None;
                    }
                    if ev.is_some_and(|e| !e.bytes().all(|b| b.is_ascii_digit())) {
                        return None;
                    }
                    (m.parse::<i32>().ok()?, parse_event_type(ev))
                }
            };
            return Some(KittySeq {
                codepoint,
                shifted_key: None,
                base_layout_key: None,
                modifier: mod_value - 1,
                event_type,
            });
        }
    }
    None
}

fn parse_modify_other_keys(data: &str) -> Option<ModifyOtherKeysSeq> {
    let rest = data.strip_prefix("\u{1b}[27;")?;
    let body = rest.strip_suffix('~')?;
    let mut it = body.split(';');
    let mod_value: i32 = it.next()?.parse().ok()?;
    let codepoint: i32 = it.next()?.parse().ok()?;
    if it.next().is_some() {
        return None;
    }
    Some(ModifyOtherKeysSeq {
        codepoint,
        modifier: mod_value - 1,
    })
}

// ---------------------------------------------------------------------------
// Matching helpers
// ---------------------------------------------------------------------------

fn matches_legacy_sequence(data: &str, sequences: &[&str]) -> bool {
    sequences.contains(&data)
}

fn matches_legacy_modifier_sequence(data: &str, key: &str, modifier: i32) -> bool {
    if modifier == MOD_SHIFT {
        return legacy_shift_sequences(key).contains(&data);
    }
    if modifier == MOD_CTRL {
        return legacy_ctrl_sequences(key).contains(&data);
    }
    false
}

fn matches_raw_backspace(data: &str, expected_modifier: i32) -> bool {
    if data == "\u{7f}" {
        return expected_modifier == 0;
    }
    if data != "\u{8}" {
        return false;
    }
    if is_windows_terminal_session() {
        expected_modifier == MOD_CTRL
    } else {
        expected_modifier == 0
    }
}

fn is_windows_terminal_session() -> bool {
    let wt = std::env::var_os("WT_SESSION").is_some_and(|v| !v.is_empty());
    let no_ssh = std::env::var_os("SSH_CONNECTION").is_none()
        && std::env::var_os("SSH_CLIENT").is_none()
        && std::env::var_os("SSH_TTY").is_none();
    wt && no_ssh
}

fn kitty_seq_matches(data: &str, expected_codepoint: i32, expected_modifier: i32) -> bool {
    let Some(p) = parse_kitty_sequence(data) else {
        return false;
    };
    let actual_mod = p.modifier & !LOCK_MASK;
    let expected_mod = expected_modifier & !LOCK_MASK;
    if actual_mod != expected_mod {
        return false;
    }
    let normalized = normalize_shifted_letter_identity(
        normalize_kitty_functional_codepoint(p.codepoint),
        p.modifier,
    );
    let normalized_expected = normalize_shifted_letter_identity(
        normalize_kitty_functional_codepoint(expected_codepoint),
        expected_modifier,
    );
    if normalized == normalized_expected {
        return true;
    }
    // Alternate match: base layout key for non-Latin layouts (only when the reported
    // codepoint is NOT an already-recognized Latin letter/symbol).
    if p.base_layout_key.is_some_and(|bk| bk == expected_codepoint) {
        let is_latin_letter = (97..=122).contains(&normalized);
        let known_symbol = char::from_u32(normalized as u32)
            .is_some_and(|c| SYMBOL_KEYS.contains(&c.to_string().as_str()));
        if !is_latin_letter && !known_symbol {
            return true;
        }
    }
    false
}

fn matches_modify_other_keys(data: &str, expected_keycode: i32, expected_modifier: i32) -> bool {
    match parse_modify_other_keys(data) {
        Some(p) => p.codepoint == expected_keycode && p.modifier == expected_modifier,
        None => false,
    }
}

fn matches_printable_modify_other_keys(
    data: &str,
    expected_keycode: i32,
    expected_modifier: i32,
) -> bool {
    if expected_modifier == 0 {
        return false;
    }
    let Some(parsed) = parse_modify_other_keys(data) else {
        return false;
    };
    if parsed.modifier != expected_modifier {
        return false;
    }
    normalize_shifted_letter_identity(parsed.codepoint, parsed.modifier)
        == normalize_shifted_letter_identity(expected_keycode, expected_modifier)
}

fn format_key_name_with_modifiers(key_name: &str, modifier: i32) -> Option<String> {
    let effective = modifier & !LOCK_MASK;
    let supported = MOD_SHIFT | MOD_CTRL | MOD_ALT | MOD_SUPER;
    if (effective & !supported) != 0 {
        return None;
    }
    let mut mods: Vec<&str> = Vec::new();
    if (effective & MOD_SHIFT) != 0 {
        mods.push("shift");
    }
    if (effective & MOD_CTRL) != 0 {
        mods.push("ctrl");
    }
    if (effective & MOD_ALT) != 0 {
        mods.push("alt");
    }
    if (effective & MOD_SUPER) != 0 {
        mods.push("super");
    }
    if mods.is_empty() {
        Some(key_name.to_string())
    } else {
        Some(format!("{}+{}", mods.join("+"), key_name))
    }
}

#[derive(Debug, Clone, Default)]
struct KeyParts {
    key: String,
    ctrl: bool,
    shift: bool,
    alt: bool,
    sup: bool,
}

fn parse_key_id(key_id: &str) -> Option<KeyParts> {
    let lowered = key_id.to_lowercase();
    let parts: Vec<&str> = lowered.split('+').collect();
    let key = (*parts.last()?).to_string();
    if key.is_empty() {
        return None;
    }
    Some(KeyParts {
        key,
        shift: parts.contains(&"shift"),
        alt: parts.contains(&"alt"),
        ctrl: parts.contains(&"ctrl"),
        sup: parts.contains(&"super"),
    })
}

/// Release-event check over raw data (Kitty flag 2 ":<n>" suffixes), paste-guarded.
pub fn is_key_release(data: &str) -> bool {
    if data.contains("\u{1b}[200~") {
        return false;
    }
    [":3u", ":3~", ":3A", ":3B", ":3C", ":3D", ":3H", ":3F"]
        .iter()
        .any(|m| data.contains(m))
}

pub fn is_key_repeat(data: &str) -> bool {
    if data.contains("\u{1b}[200~") {
        return false;
    }
    [":2u", ":2~", ":2A", ":2B", ":2C", ":2D", ":2H", ":2F"]
        .iter()
        .any(|m| data.contains(m))
}

pub fn matches_key(data: &str, key_id: &str) -> bool {
    let Some(parsed) = parse_key_id(key_id) else {
        return false;
    };
    let p = parsed;
    let key = p.key.as_str();
    let mut modifier = 0;
    if p.shift {
        modifier |= MOD_SHIFT;
    }
    if p.alt {
        modifier |= MOD_ALT;
    }
    if p.ctrl {
        modifier |= MOD_CTRL;
    }
    if p.sup {
        modifier |= MOD_SUPER;
    }

    let kitty_active = is_kitty_protocol_active();

    match key {
        "escape" | "esc" => {
            if modifier != 0 {
                return false;
            }
            data == "\u{1b}"
                || kitty_seq_matches(data, codepoint_of("escape"), 0)
                || matches_modify_other_keys(data, codepoint_of("escape"), 0)
        }
        "space" => {
            if !kitty_active {
                if modifier == MOD_CTRL && data == "\0" {
                    return true;
                }
                if modifier == MOD_ALT && data == "\u{1b} " {
                    return true;
                }
            }
            if modifier == 0 {
                return data == " "
                    || kitty_seq_matches(data, codepoint_of("space"), 0)
                    || matches_modify_other_keys(data, codepoint_of("space"), 0);
            }
            kitty_seq_matches(data, codepoint_of("space"), modifier)
                || matches_modify_other_keys(data, codepoint_of("space"), modifier)
        }
        "tab" => {
            if modifier == MOD_SHIFT {
                return data == "\u{1b}[Z"
                    || kitty_seq_matches(data, codepoint_of("tab"), MOD_SHIFT)
                    || matches_modify_other_keys(data, codepoint_of("tab"), MOD_SHIFT);
            }
            if modifier == 0 {
                return data == "\t" || kitty_seq_matches(data, codepoint_of("tab"), 0);
            }
            kitty_seq_matches(data, codepoint_of("tab"), modifier)
                || matches_modify_other_keys(data, codepoint_of("tab"), modifier)
        }
        "enter" | "return" => enter_case(data, modifier, kitty_active),
        "backspace" => backspace_case(data, modifier),
        "insert" | "delete" | "clear" | "home" | "end" | "pageup" | "pagedown" => {
            functional_case(key, data, modifier)
        }
        "up" | "down" => arrow_ud_case(key, data, modifier),
        "left" | "right" => arrow_lr_case(key, data, modifier, kitty_active),
        f if matches!(
            f,
            "f1" | "f2" | "f3" | "f4" | "f5" | "f6" | "f7" | "f8" | "f9" | "f10" | "f11" | "f12"
        ) =>
        {
            if modifier != 0 {
                return false;
            }
            matches_legacy_sequence(data, legacy_key_sequences(f))
        }
        single
            if single.chars().count() == 1
                && (single.as_bytes()[0].is_ascii_lowercase()
                    || is_digit_str(single)
                    || SYMBOL_KEYS.contains(&single)) =>
        {
            printable_case(single, data, modifier, kitty_active)
        }
        _ => false,
    }
}

fn enter_case(data: &str, modifier: i32, kitty_active: bool) -> bool {
    let enter = codepoint_of("enter");
    let kp_enter = codepoint_of("kpEnter");
    match modifier {
        m if m == MOD_SHIFT => {
            if kitty_seq_matches(data, enter, MOD_SHIFT)
                || kitty_seq_matches(data, kp_enter, MOD_SHIFT)
            {
                return true;
            }
            if matches_modify_other_keys(data, enter, MOD_SHIFT) {
                return true;
            }
            if kitty_active {
                return data == "\u{1b}\r" || data == "\n";
            }
            false
        }
        m if m == MOD_ALT => {
            if kitty_seq_matches(data, enter, MOD_ALT) || kitty_seq_matches(data, kp_enter, MOD_ALT)
            {
                return true;
            }
            if matches_modify_other_keys(data, enter, MOD_ALT) {
                return true;
            }
            if !kitty_active {
                return data == "\u{1b}\r";
            }
            false
        }
        0 => {
            data == "\r"
                || (!kitty_active && data == "\n")
                || data == "\u{1b}OM"
                || kitty_seq_matches(data, enter, 0)
                || kitty_seq_matches(data, kp_enter, 0)
        }
        m => {
            kitty_seq_matches(data, enter, m)
                || kitty_seq_matches(data, kp_enter, m)
                || matches_modify_other_keys(data, enter, m)
        }
    }
}

fn backspace_case(data: &str, modifier: i32) -> bool {
    let bs = codepoint_of("backspace");
    if modifier == MOD_ALT {
        if data == "\u{1b}\u{7f}" || data == "\u{1b}\u{8}" {
            return true;
        }
        return kitty_seq_matches(data, bs, MOD_ALT)
            || matches_modify_other_keys(data, bs, MOD_ALT);
    }
    if modifier == MOD_CTRL {
        if matches_raw_backspace(data, MOD_CTRL) {
            return true;
        }
        return kitty_seq_matches(data, bs, MOD_CTRL)
            || matches_modify_other_keys(data, bs, MOD_CTRL);
    }
    if modifier == 0 {
        return matches_raw_backspace(data, 0)
            || kitty_seq_matches(data, bs, 0)
            || matches_modify_other_keys(data, bs, 0);
    }
    kitty_seq_matches(data, bs, modifier) || matches_modify_other_keys(data, bs, modifier)
}

fn functional_case(key: &str, data: &str, modifier: i32) -> bool {
    // Reference switch keys are lowercased by parse_key_id already ("pageup"/"pagedown").
    // The legacy tables are keyed camelCase: map back.
    let table_key = match key {
        "pageup" => "pageUp",
        "pagedown" => "pageDown",
        other => other,
    };
    let func_cp = match key {
        "delete" => lookup(FUNCTIONAL_CODEPOINTS, "delete").unwrap(),
        "insert" => lookup(FUNCTIONAL_CODEPOINTS, "insert").unwrap(),
        "home" => lookup(FUNCTIONAL_CODEPOINTS, "home").unwrap(),
        "end" => lookup(FUNCTIONAL_CODEPOINTS, "end").unwrap(),
        "pageup" => lookup(FUNCTIONAL_CODEPOINTS, "pageUp").unwrap(),
        "pagedown" => lookup(FUNCTIONAL_CODEPOINTS, "pageDown").unwrap(),
        "clear" => 0, // no kitty functional mapping in reference switch for clear
        _ => unreachable!(),
    };
    if modifier == 0 {
        let legacy_ok = matches_legacy_sequence(data, legacy_key_sequences(table_key));
        return if func_cp == 0 {
            legacy_ok
        } else {
            legacy_ok || kitty_seq_matches(data, func_cp, 0)
        };
    }
    if matches_legacy_modifier_sequence(data, table_key, modifier) {
        return true;
    }
    if func_cp == 0 {
        return false;
    }
    kitty_seq_matches(data, func_cp, modifier)
}

fn arrow_ud_case(key: &str, data: &str, modifier: i32) -> bool {
    let dir_cp = lookup(ARROW_CODEPOINTS, key).unwrap();
    if modifier == MOD_ALT {
        let literal = if key == "up" { "\u{1b}p" } else { "\u{1b}n" };
        return data == literal || kitty_seq_matches(data, dir_cp, MOD_ALT);
    }
    if modifier == 0 {
        return matches_legacy_sequence(data, legacy_key_sequences(key))
            || kitty_seq_matches(data, dir_cp, 0);
    }
    if matches_legacy_modifier_sequence(data, key, modifier) {
        return true;
    }
    kitty_seq_matches(data, dir_cp, modifier)
}

fn arrow_lr_case(key: &str, data: &str, modifier: i32, kitty_active: bool) -> bool {
    let dir_cp = lookup(ARROW_CODEPOINTS, key).unwrap();
    if modifier == MOD_ALT {
        return if key == "left" {
            data == "\u{1b}[1;3D"
                || (!kitty_active && data == "\u{1b}B")
                || data == "\u{1b}b"
                || kitty_seq_matches(data, dir_cp, MOD_ALT)
        } else {
            data == "\u{1b}[1;3C"
                || (!kitty_active && data == "\u{1b}F")
                || data == "\u{1b}f"
                || kitty_seq_matches(data, dir_cp, MOD_ALT)
        };
    }
    if modifier == MOD_CTRL {
        let csi = if key == "left" {
            "\u{1b}[1;5D"
        } else {
            "\u{1b}[1;5C"
        };
        return data == csi
            || matches_legacy_modifier_sequence(data, key, MOD_CTRL)
            || kitty_seq_matches(data, dir_cp, MOD_CTRL);
    }
    if modifier == 0 {
        return matches_legacy_sequence(data, legacy_key_sequences(key))
            || kitty_seq_matches(data, dir_cp, 0);
    }
    if matches_legacy_modifier_sequence(data, key, modifier) {
        return true;
    }
    kitty_seq_matches(data, dir_cp, modifier)
}

fn printable_case(key: &str, data: &str, modifier: i32, kitty_active: bool) -> bool {
    let codepoint = key.chars().next().unwrap() as i32;
    let raw_ctrl = raw_ctrl_char(key);
    let is_letter = key.as_bytes()[0].is_ascii_lowercase();
    let is_digit = is_digit_str(key);

    if modifier == MOD_CTRL + MOD_ALT
        && !kitty_active
        && raw_ctrl.is_some_and(|rc| data == format!("\u{1b}{rc}"))
    {
        return true;
    }
    if modifier == MOD_ALT
        && !kitty_active
        && (is_letter || is_digit || SYMBOL_KEYS.contains(&key))
        && data == format!("\u{1b}{key}")
    {
        return true;
    }
    if modifier == MOD_CTRL {
        if raw_ctrl.is_some_and(|rc| data == rc.to_string()) {
            return true;
        }
        return kitty_seq_matches(data, codepoint, MOD_CTRL)
            || matches_printable_modify_other_keys(data, codepoint, MOD_CTRL);
    }
    if modifier == MOD_SHIFT + MOD_CTRL {
        return kitty_seq_matches(data, codepoint, MOD_SHIFT + MOD_CTRL)
            || matches_printable_modify_other_keys(data, codepoint, MOD_SHIFT + MOD_CTRL);
    }
    if modifier == MOD_SHIFT {
        if is_letter && data == key.to_uppercase() {
            return true;
        }
        return kitty_seq_matches(data, codepoint, MOD_SHIFT)
            || matches_printable_modify_other_keys(data, codepoint, MOD_SHIFT);
    }
    if modifier != 0 {
        return kitty_seq_matches(data, codepoint, modifier)
            || matches_printable_modify_other_keys(data, codepoint, modifier);
    }
    data == key || kitty_seq_matches(data, codepoint, 0)
}

// ---------------------------------------------------------------------------
// parseKey
// ---------------------------------------------------------------------------

fn from_code_point_u32(cp: u32) -> Option<char> {
    char::from_u32(cp)
}

fn format_parsed_key(
    codepoint: i32,
    modifier: i32,
    base_layout_key: Option<i32>,
) -> Option<String> {
    let normalized = normalize_kitty_functional_codepoint(codepoint);
    let identity_cp = normalize_shifted_letter_identity(normalized, modifier);

    let chr_for = |cp: i32| from_code_point_u32(cp as u32);

    let is_latin_letter = (97..=122).contains(&identity_cp);
    let is_digit = (48..=57).contains(&identity_cp);
    let known_symbol =
        chr_for(identity_cp).is_some_and(|c| SYMBOL_KEYS.contains(&c.to_string().as_str()));
    let effective_cp = if is_latin_letter || is_digit || known_symbol {
        identity_cp
    } else {
        base_layout_key.unwrap_or(identity_cp)
    };

    let key_name: String = if effective_cp == codepoint_of("escape") {
        "escape".into()
    } else if effective_cp == codepoint_of("tab") {
        "tab".into()
    } else if effective_cp == codepoint_of("enter") || effective_cp == codepoint_of("kpEnter") {
        "enter".into()
    } else if effective_cp == codepoint_of("space") {
        "space".into()
    } else if effective_cp == codepoint_of("backspace") {
        "backspace".into()
    } else if Some(effective_cp) == lookup(FUNCTIONAL_CODEPOINTS, "delete") {
        "delete".into()
    } else if Some(effective_cp) == lookup(FUNCTIONAL_CODEPOINTS, "insert") {
        "insert".into()
    } else if Some(effective_cp) == lookup(FUNCTIONAL_CODEPOINTS, "home") {
        "home".into()
    } else if Some(effective_cp) == lookup(FUNCTIONAL_CODEPOINTS, "end") {
        "end".into()
    } else if Some(effective_cp) == lookup(FUNCTIONAL_CODEPOINTS, "pageUp") {
        "pageUp".into()
    } else if Some(effective_cp) == lookup(FUNCTIONAL_CODEPOINTS, "pageDown") {
        "pageDown".into()
    } else if Some(effective_cp) == lookup(ARROW_CODEPOINTS, "up") {
        "up".into()
    } else if Some(effective_cp) == lookup(ARROW_CODEPOINTS, "down") {
        "down".into()
    } else if Some(effective_cp) == lookup(ARROW_CODEPOINTS, "left") {
        "left".into()
    } else if Some(effective_cp) == lookup(ARROW_CODEPOINTS, "right") {
        "right".into()
    } else if ((48..=57).contains(&effective_cp) || (97..=122).contains(&effective_cp))
        || chr_for(effective_cp).is_some_and(|c| SYMBOL_KEYS.contains(&c.to_string().as_str()))
    {
        chr_for(effective_cp)?.to_string()
    } else {
        return None;
    };
    format_key_name_with_modifiers(&key_name, modifier)
}

pub fn parse_key(data: &str) -> Option<String> {
    let kitty = parse_kitty_sequence(data);
    if let Some(k) = kitty {
        return format_parsed_key(k.codepoint, k.modifier, k.base_layout_key);
    }
    if let Some(mok) = parse_modify_other_keys(data) {
        return format_parsed_key(mok.codepoint, mok.modifier, None);
    }
    let kitty_active = is_kitty_protocol_active();
    if kitty_active && (data == "\u{1b}\r" || data == "\n") {
        return Some("shift+enter".into());
    }
    if let Some((_, id)) = LEGACY_SEQUENCE_KEY_IDS
        .iter()
        .find(|(seq, _)| **seq == *data)
    {
        return Some(id.to_string());
    }
    if data == "\u{1b}" {
        return Some("escape".into());
    }
    if data == "\u{1c}" {
        return Some("ctrl+\\".into());
    }
    if data == "\u{1d}" {
        return Some("ctrl+]".into());
    }
    if data == "\u{1f}" {
        return Some("ctrl+-".into());
    }
    if data == "\u{1b}\u{1b}" {
        return Some("ctrl+alt+[".into());
    }
    if data == "\u{1b}\u{1c}" {
        return Some("ctrl+alt+\\".into());
    }
    if data == "\u{1b}\u{1d}" {
        return Some("ctrl+alt+]".into());
    }
    if data == "\u{1b}\u{1f}" {
        return Some("ctrl+alt+-".into());
    }
    if data == "\t" {
        return Some("tab".into());
    }
    if data == "\r" || (!kitty_active && data == "\n") || data == "\u{1b}OM" {
        return Some("enter".into());
    }
    if data == "\0" {
        return Some("ctrl+space".into());
    }
    if data == " " {
        return Some("space".into());
    }
    if data == "\u{7f}" {
        return Some("backspace".into());
    }
    if data == "\u{8}" {
        return Some(
            if is_windows_terminal_session() {
                "ctrl+backspace"
            } else {
                "backspace"
            }
            .into(),
        );
    }
    if data == "\u{1b}[Z" {
        return Some("shift+tab".into());
    }
    if !kitty_active && data == "\u{1b}\r" {
        return Some("alt+enter".into());
    }
    if !kitty_active && data == "\u{1b} " {
        return Some("alt+space".into());
    }
    if data == "\u{1b}\u{7f}" || data == "\u{1b}\u{8}" {
        return Some("alt+backspace".into());
    }
    if !kitty_active && data == "\u{1b}B" {
        return Some("alt+left".into());
    }
    if !kitty_active && data == "\u{1b}F" {
        return Some("alt+right".into());
    }
    if !kitty_active && data.chars().count() == 2 && data.starts_with('\u{1b}') {
        let second = data.chars().nth(1).unwrap() as u32;
        if (1..=26).contains(&second) {
            let letter = char::from_u32(second + 96)?;
            return Some(format!("ctrl+alt+{letter}"));
        }
        let key_ch = char::from_u32(second)?;
        let key = key_ch.to_string();
        let is_known = SYMBOL_KEYS.contains(&key.as_str());
        if ((97..=122).contains(&second)) || ((48..=57).contains(&second)) || is_known {
            return Some(format!("alt+{key}"));
        }
    }
    if data == "\u{1b}[A" {
        return Some("up".into());
    }
    if data == "\u{1b}[B" {
        return Some("down".into());
    }
    if data == "\u{1b}[C" {
        return Some("right".into());
    }
    if data == "\u{1b}[D" {
        return Some("left".into());
    }
    if data == "\u{1b}[H" || data == "\u{1b}OH" {
        return Some("home".into());
    }
    if data == "\u{1b}[F" || data == "\u{1b}OF" {
        return Some("end".into());
    }
    if data == "\u{1b}[3~" {
        return Some("delete".into());
    }
    if data == "\u{1b}[5~" {
        return Some("pageUp".into());
    }
    if data == "\u{1b}[6~" {
        return Some("pageDown".into());
    }
    // Raw Ctrl+letter / printable single unit
    if data.len() == 1 {
        let b0 = data.as_bytes()[0];
        if (1..=26).contains(&b0) {
            let letter = (b0 as u32) + 96;
            return Some(format!("ctrl+{}", char::from_u32(letter)?));
        }
        if (32..=126).contains(&b0) {
            return Some(data.to_string());
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Printable decoding
// ---------------------------------------------------------------------------

const KITTY_PRINTABLE_ALLOWED_MODIFIERS: i32 = MOD_SHIFT | LOCK_MASK;

/// Decode an unmodified or Shift-only Kitty CSI-u sequence to printable text.
/// This is the reference `decodeKittyPrintable` primitive; the broader
/// [`decode_printable_key`] also accepts modifyOtherKeys sequences.
pub fn decode_kitty_printable(data: &str) -> Option<String> {
    let seq = parse_kitty_sequence(data)?;
    let modifier = seq.modifier; // already value-1
    if (modifier & !KITTY_PRINTABLE_ALLOWED_MODIFIERS) != 0 {
        return None;
    }
    if (modifier & (MOD_ALT | MOD_CTRL)) != 0 {
        return None;
    }
    let mut effective_cp = seq.codepoint;
    if let Some(sk) = seq.shifted_key.filter(|_| (modifier & MOD_SHIFT) != 0) {
        effective_cp = sk;
    }
    effective_cp = normalize_kitty_functional_codepoint(effective_cp);
    if effective_cp < 32 {
        return None;
    }
    from_code_point_u32(effective_cp as u32).map(|c| c.to_string())
}

fn decode_modify_other_keys_printable(data: &str) -> Option<String> {
    let parsed = parse_modify_other_keys(data)?;
    let modifier = parsed.modifier & !LOCK_MASK;
    if (modifier & !MOD_SHIFT) != 0 {
        return None;
    }
    if parsed.codepoint < 32 {
        return None;
    }
    from_code_point_u32(parsed.codepoint as u32).map(|c| c.to_string())
}

pub fn decode_printable_key(data: &str) -> Option<String> {
    decode_kitty_printable(data).or_else(|| decode_modify_other_keys_printable(data))
}

#[cfg(test)]
mod sanity {
    use super::*;

    #[test]
    fn kitty_sequence_paths() {
        set_kitty_protocol_active(false);
        assert_eq!(parse_key("\u{1b}[3~").as_deref(), Some("delete"));
        assert!(kitty_seq_matches("\u{1b}[1;2A", -1, 1)); // shift+up kitty arrow
        assert!(kitty_seq_matches("\u{1b}[1;5H", -14, 4)); // ctrl+home kitty form
        assert_eq!(parse_key("c"), Some("c".into()));
        assert!(matches_key("c", "c"));
        assert!(matches_key("\u{3}", "ctrl+c"));
        assert!(matches_key("\u{1b}", "escape"));
    }
}
