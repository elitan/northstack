//! The `northstack` command (`cargo install northstack`): `northstack new <dir>`
//! creates an app. Inside an app, `just` runs the app's own copy (its `cli/` crate).
fn main() {
    northstack::cli::main()
}
