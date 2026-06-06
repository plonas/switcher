use std::env;

fn main() {
    println!("cargo:rerun-if-env-changed=TARGET");

    let target = env::var("TARGET").unwrap_or_default();

    if target != "thumbv7em-none-eabihf" {
        panic!(
            "switcher-firmware must be built for `thumbv7em-none-eabihf`.\n\
             Use one of:\n\
             - `cargo build-firmware`\n\
             - `cargo check-firmware`\n\
             - `cargo run-firmware`\n\
             - `cargo build -p switcher-firmware --target thumbv7em-none-eabihf`"
        );
    }
}
