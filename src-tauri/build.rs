fn main() {
    // Resource-only changes must invalidate Cargo's cached Windows executable.
    println!("cargo:rerun-if-changed=icons");
    tauri_build::build()
}
