use std::{env, fs, path::PathBuf};

fn main() {
    println!("cargo:rerun-if-env-changed=TARGET");
    println!("cargo:rerun-if-changed=memory.x");

    let target = env::var("TARGET").unwrap_or_default();

    if target != "thumbv7em-none-eabihf" {
        panic!(
            "dongle must be built for `thumbv7em-none-eabihf`.\n\
             Use one of:\n\
             - `cargo build-firmware`\n\
             - `cargo check-firmware`\n\
             - `cargo run-firmware`\n\
             - `cargo build -p dongle --target thumbv7em-none-eabihf`"
        );
    }

    let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR should be set"));
    fs::write(out_dir.join("memory.x"), include_bytes!("memory.x"))
        .expect("failed to copy memory.x to OUT_DIR");
    println!("cargo:rustc-link-search={}", out_dir.display());
}
