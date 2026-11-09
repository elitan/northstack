// The frontend is embedded from web/dist. Make sure the folder exists so the
// backend compiles before the frontend has been built.
fn main() {
    let dist = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../web/dist");
    if !dist.exists() {
        std::fs::create_dir_all(&dist).expect("create web/dist");
    }
    println!("cargo:rerun-if-changed=../web/dist");
    println!("cargo:rerun-if-changed=migrations");
}
