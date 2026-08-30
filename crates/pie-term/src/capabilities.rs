//! Terminal capability policy, probing, and lazy cache.
//!
//! Detection is split into an injected, deterministic environment matrix and
//! the small real-world tmux probe used by the global facade. Tests never touch
//! the controlling terminal or mutate process environment.

use std::collections::HashMap;
use std::io::Read;
use std::process::{Command, Stdio};
use std::sync::{Arc, OnceLock, RwLock};
use std::time::{Duration, Instant};

use pie_core::terminal_image::ImageProtocol;

const TMUX_PROBE_TIMEOUT: Duration = Duration::from_millis(250);

/// Inline-image, true-color, and OSC 8 support selected for one environment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TerminalCapabilities {
    pub images: Option<ImageProtocol>,
    pub true_color: bool,
    pub hyperlinks: bool,
}

/// Immutable facts used by capability detection.
#[derive(Debug, Clone, Default)]
pub struct TerminalEnvironment {
    variables: HashMap<String, String>,
    is_windows: bool,
}

impl TerminalEnvironment {
    pub fn from_pairs<'a>(
        is_windows: bool,
        pairs: impl IntoIterator<Item = (&'a str, &'a str)>,
    ) -> Self {
        Self {
            variables: pairs
                .into_iter()
                .map(|(key, value)| (key.to_owned(), value.to_owned()))
                .collect(),
            is_windows,
        }
    }

    pub fn current() -> Self {
        Self {
            variables: std::env::vars().collect(),
            is_windows: cfg!(windows),
        }
    }

    fn value(&self, key: &str) -> &str {
        self.variables.get(key).map_or("", String::as_str)
    }

    fn truthy(&self, key: &str) -> bool {
        !self.value(key).is_empty()
    }
}

/// Reproduce the pinned terminal-emulator priority with the tmux probe injected.
pub fn detect_capabilities(
    environment: &TerminalEnvironment,
    mut tmux_forwards_hyperlinks: impl FnMut() -> bool,
) -> TerminalCapabilities {
    let term_program = environment.value("TERM_PROGRAM").to_ascii_lowercase();
    let terminal_emulator = environment.value("TERMINAL_EMULATOR").to_ascii_lowercase();
    let term = environment.value("TERM").to_ascii_lowercase();
    let color_term = environment.value("COLORTERM").to_ascii_lowercase();
    let has_true_color_hint = matches!(color_term.as_str(), "truecolor" | "24bit");

    // Multiplexers win over every inner-emulator hint. Their forwarding policy
    // determines what the attached client, rather than the inner shell, can use.
    if environment.truthy("TMUX") || term.starts_with("tmux") {
        return TerminalCapabilities {
            images: None,
            true_color: has_true_color_hint,
            hyperlinks: tmux_forwards_hyperlinks(),
        };
    }
    if term.starts_with("screen") {
        return TerminalCapabilities {
            images: None,
            true_color: has_true_color_hint,
            hyperlinks: false,
        };
    }
    if environment.truthy("KITTY_WINDOW_ID") || term_program == "kitty" {
        return fully_capable(ImageProtocol::Kitty);
    }
    if term_program == "ghostty"
        || term.contains("ghostty")
        || environment.truthy("GHOSTTY_RESOURCES_DIR")
    {
        return fully_capable(ImageProtocol::Kitty);
    }
    if environment.truthy("WEZTERM_PANE") || term_program == "wezterm" {
        return fully_capable(ImageProtocol::Kitty);
    }
    if term_program == "warpterminal"
        || environment.truthy("WARP_SESSION_ID")
        || environment.truthy("WARP_TERMINAL_SESSION_UUID")
    {
        return fully_capable(ImageProtocol::Kitty);
    }
    if environment.truthy("ITERM_SESSION_ID") || term_program == "iterm.app" {
        return fully_capable(ImageProtocol::ITerm2);
    }
    if environment.truthy("WT_SESSION") || matches!(term_program.as_str(), "vscode" | "alacritty") {
        return TerminalCapabilities {
            images: None,
            true_color: true,
            hyperlinks: true,
        };
    }
    if terminal_emulator == "jetbrains-jediterm" {
        return TerminalCapabilities {
            images: None,
            true_color: true,
            hyperlinks: false,
        };
    }
    if environment.is_windows {
        return TerminalCapabilities {
            images: None,
            true_color: true,
            hyperlinks: false,
        };
    }
    TerminalCapabilities {
        images: None,
        true_color: has_true_color_hint,
        hyperlinks: false,
    }
}

fn fully_capable(images: ImageProtocol) -> TerminalCapabilities {
    TerminalCapabilities {
        images: Some(images),
        true_color: true,
        hyperlinks: true,
    }
}

/// Identity-preserving lazy capability cache.
#[derive(Debug, Default)]
pub struct CapabilitiesCache {
    cached: RwLock<Option<Arc<TerminalCapabilities>>>,
}

impl CapabilitiesCache {
    pub fn get_or_detect(
        &self,
        environment: &TerminalEnvironment,
        tmux_forwards_hyperlinks: impl FnMut() -> bool,
    ) -> Arc<TerminalCapabilities> {
        if let Some(cached) = read_unpoisoned(&self.cached).as_ref() {
            return Arc::clone(cached);
        }
        let mut slot = write_unpoisoned(&self.cached);
        if let Some(cached) = slot.as_ref() {
            return Arc::clone(cached);
        }
        let detected = Arc::new(detect_capabilities(environment, tmux_forwards_hyperlinks));
        *slot = Some(Arc::clone(&detected));
        detected
    }

    pub fn set(&self, capabilities: Arc<TerminalCapabilities>) {
        *write_unpoisoned(&self.cached) = Some(capabilities);
    }

    pub fn reset(&self) {
        *write_unpoisoned(&self.cached) = None;
    }
}

static GLOBAL_CAPABILITIES: OnceLock<CapabilitiesCache> = OnceLock::new();

/// Lazily detect and return the process-global capability object.
pub fn get_capabilities() -> Arc<TerminalCapabilities> {
    global_cache().get_or_detect(&TerminalEnvironment::current(), probe_tmux_hyperlinks)
}

/// Override the global cache while retaining the caller's `Arc` identity.
pub fn set_capabilities(capabilities: Arc<TerminalCapabilities>) {
    global_cache().set(capabilities);
}

/// Clear the process-global cache so the next lookup re-reads the environment.
pub fn reset_capabilities_cache() {
    global_cache().reset();
}

fn global_cache() -> &'static CapabilitiesCache {
    GLOBAL_CAPABILITIES.get_or_init(CapabilitiesCache::default)
}

/// Ask the attached tmux client whether it forwards OSC 8 hyperlinks.
///
/// The subprocess is stdin-detached, output-bounded by tmux itself, and killed
/// after the reference's 250 ms timeout. Errors conservatively mean `false`.
pub fn probe_tmux_hyperlinks() -> bool {
    let Ok(mut child) = Command::new("tmux")
        .args(["display-message", "-p", "#{client_termfeatures}"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    else {
        return false;
    };
    let deadline = Instant::now() + TMUX_PROBE_TIMEOUT;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let mut output = String::new();
                if !status.success()
                    || child
                        .stdout
                        .take()
                        .is_none_or(|mut stdout| stdout.read_to_string(&mut output).is_err())
                {
                    return false;
                }
                return output
                    .split(',')
                    .any(|feature| feature.trim() == "hyperlinks");
            }
            Ok(None) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(5));
            }
            Ok(None) | Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return false;
            }
        }
    }
}

fn read_unpoisoned<T>(lock: &RwLock<T>) -> std::sync::RwLockReadGuard<'_, T> {
    lock.read()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn write_unpoisoned<T>(lock: &RwLock<T>) -> std::sync::RwLockWriteGuard<'_, T> {
    lock.write()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn screen_never_invokes_tmux_probe() {
        let environment = TerminalEnvironment::from_pairs(false, [("TERM", "screen")]);
        let result = detect_capabilities(&environment, || panic!("screen must not probe tmux"));
        assert!(!result.hyperlinks);
    }
}
