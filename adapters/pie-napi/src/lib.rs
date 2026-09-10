//! pie-napi — adapter layer. The public Rust compatibility namespace remains
//! separate from the deliberately smaller private native ABI.

pub mod pi_tui;

mod autocomplete_bridge;
mod native;

pub mod placeholder {
    pub fn scaffold() -> &'static str {
        "pie-napi"
    }
}
