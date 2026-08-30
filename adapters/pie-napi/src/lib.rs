//! pie-napi — adapter layer. The public Rust compatibility namespace remains
//! separate from the deliberately smaller private native ABI.

pub mod pi_tui;

mod native;

pub mod placeholder {
    pub fn scaffold() -> &'static str {
        "pie-napi"
    }
}
