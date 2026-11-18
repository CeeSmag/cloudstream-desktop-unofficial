//! Compiles the Slint UI into Rust modules with a locked dark Fluent style.

fn main() {
    let config = slint_build::CompilerConfiguration::new()
        .with_style("fluent-dark".into())
        // Edition 1.19: `if` and `for` elements count toward the size of their parent (a
        // Flickable or ScrollView), which the poster rows and shelves rely on.
        .with_edition((1, 19));
    if let Err(error) = slint_build::compile_with_config("ui/app.slint", config) {
        // Build scripts may panic to fail the build.
        #[allow(clippy::panic)]
        {
            panic!("Slint compile failed: {error:?}");
        }
    }
}
