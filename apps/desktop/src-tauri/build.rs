use std::path::Path;

fn main() {
    // O visualizador do export HTML (packages/export-shell) entra no binário.
    // Sem ele compilado, o app funciona, mas o export avisa que está indisponível.
    let shell = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../packages/export-shell/dist/index.html");
    println!("cargo:rerun-if-changed={}", shell.display());
    let out = Path::new(&std::env::var("OUT_DIR").unwrap()).join("export-shell.html");
    match std::fs::read(&shell) {
        Ok(html) => std::fs::write(&out, html).unwrap(),
        Err(_) => {
            println!("cargo:warning=export-shell não compilado: rode `pnpm --filter export-shell build`");
            std::fs::write(&out, b"").unwrap();
        }
    }
    tauri_build::build()
}
