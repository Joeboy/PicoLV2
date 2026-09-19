use std::env;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;

fn main() {
    // Put `memory.x` in our output directory and ensure it's on the linker search path.
    let out = &PathBuf::from(env::var_os("OUT_DIR").unwrap());
    File::create(out.join("memory.x"))
        .unwrap()
        .write_all(include_bytes!("memory.x"))
        .unwrap();
    println!("cargo:rustc-link-search={}", out.display());
    println!("cargo:rerun-if-changed=memory.x");
    println!("cargo:rerun-if-env-changed=DEFMT_LOG");

    println!("cargo:rustc-check-cfg=cfg(picolv2_perf_diagnostics)");
    let log_level = env::var("DEFMT_LOG").unwrap_or_else(|_| "info".into());
    if matches!(log_level.as_str(), "debug" | "trace") {
        println!("cargo:rustc-cfg=picolv2_perf_diagnostics");
    }

    println!("cargo:rustc-link-arg-bins=--nmagic");
    println!("cargo:rustc-link-arg-bins=-Tlink.x");
    println!("cargo:rustc-link-arg-bins=-Tdefmt.x");
}
