//! Compile the app's icons + logos into a GResource bundle baked into the
//! binary, so they work identically in `cargo run` and when installed (no
//! reliance on CARGO_MANIFEST_DIR paths at runtime).

fn main() {
    glib_build_tools::compile_resources(
        &["."],
        "resources/meusic.gresource.xml",
        "meusic.gresource",
    );
}
