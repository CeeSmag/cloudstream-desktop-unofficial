//! Compiles the Slint UI into Rust modules with a locked dark Fluent style.

fn main() {
    let config = slint_build::CompilerConfiguration::new().with_style("fluent-dark".into());
    if let Err(error) = slint_build::compile_with_config("ui/app.slint", config) {
        // Build scripts may panic to fail the build.
        #[allow(clippy::panic)]
        {
            panic!("Slint compile failed: {error:?}");
        }
    }
}
