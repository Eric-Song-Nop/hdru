use std::{env, path::PathBuf};

fn main() {
    println!("cargo:rerun-if-env-changed=HDRU_NSH_PACKAGE");
    println!("cargo:rerun-if-env-changed=HDRU_NSH_EXECUTABLE");
    let package = env::var_os("HDRU_NSH_PACKAGE")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap()).join("../../examples/counter")
        });
    let package = package.canonicalize().expect("hd package directory");
    let name = env::var("HDRU_NSH_EXECUTABLE").unwrap_or_else(|_| "counter-nsh".to_owned());
    let artifacts = package.join("build/debug");
    println!("cargo:rustc-env=HDRU_NSH_NAME={name}");
    for (key, path) in [
        ("HDRU_NSH_MANIFEST", package.join("hd.toml")),
        ("HDRU_NSH_WASM", artifacts.join(format!("{name}.wasm"))),
        (
            "HDRU_NSH_BINDINGS",
            artifacts.join(format!("{name}.host.rs")),
        ),
    ] {
        println!("cargo:rerun-if-changed={}", path.display());
        println!("cargo:rustc-env={key}={}", path.display());
    }
}
