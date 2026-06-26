fn main() {
    println!("cargo:rerun-if-changed=../frontend/src");
    println!("cargo:rerun-if-changed=../frontend/index.html");
    println!("cargo:rerun-if-changed=../frontend/input.css");
    println!("cargo:rerun-if-changed=../frontend/Cargo.toml");

    let profile = std::env::var("PROFILE").unwrap_or_default();
    let manifest = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let frontend = std::path::Path::new(&manifest).join("../frontend");

    if profile == "release" {
        let status = std::process::Command::new("trunk")
            .args(["build", "--release"])
            .current_dir(&frontend)
            .status()
            .expect("trunk não encontrado — instale com: cargo install trunk");
        assert!(status.success(), "trunk build --release falhou");
    } else if !frontend.join("dist/index.html").exists() {
        println!("cargo:warning=Frontend não compilado. Rode: cd crates/frontend && trunk build");
    }
}
