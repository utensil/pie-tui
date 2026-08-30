fn main() {
    napi_build::setup();
    match std::env::var("CARGO_CFG_TARGET_OS").as_deref() {
        Ok("macos") => {
            println!("cargo:rustc-cdylib-link-arg=-Wl,-install_name,@rpath/pie-tui-native.node");
        }
        Ok("linux") => {
            println!("cargo:rustc-cdylib-link-arg=-Wl,-soname,pie-tui-native.node");
        }
        _ => {}
    }
}
