fn main() {
    let arch = std::env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();
    if arch != "wasm32" {
        return;
    }
    println!("cargo:rerun-if-changed=c-shims/memcmp.c");
    cc::Build::new()
        .file("c-shims/memcmp.c")
        .opt_level(2)
        .warnings(false)
        .compile("cdc_memcmp_shim");
}
