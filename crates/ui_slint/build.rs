//! Compiles the Slint UI into Rust modules.

fn main() {
    if let Err(error) = slint_build::compile("ui/app.slint") {
        // Build scripts may panic to fail the build.
        #[allow(clippy::panic)]
        {
            panic!("Slint compile failed: {error:?}");
        }
    }
}
