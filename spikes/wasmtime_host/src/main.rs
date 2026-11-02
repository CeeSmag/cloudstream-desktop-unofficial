use anyhow::{Context, Result};
use wasmtime::{Config, Engine, Linker, Module, Store};
use wasmtime_wasi::p1::{self, WasiP1Ctx};
use wasmtime_wasi::WasiCtxBuilder;

fn main() -> Result<()> {
    let path = std::env::args().nth(1).context("usage: wasmtime_host <file.wasm>")?;
    let mut config = Config::new();
    config.wasm_gc(true);
    config.wasm_function_references(true);
    config.wasm_exceptions(true);
    let engine = Engine::new(&config)?;
    let module = Module::from_file(&engine, &path)?;
    for import in module.imports() {
        println!("import {}::{} {:?}", import.module(), import.name(), import.ty());
    }
    for export in module.exports() {
        println!("export {} {:?}", export.name(), export.ty());
    }
    let mut linker: Linker<WasiP1Ctx> = Linker::new(&engine);
    p1::add_to_linker_sync(&mut linker, |ctx| ctx)?;
    linker.func_wrap("host", "host_double", |value: i32| value * 2)?;
    let mut store = Store::new(&engine, WasiCtxBuilder::new().inherit_stdio().build_p1());
    let instance = linker.instantiate(&mut store, &module)?;
    if let Ok(start) = instance.get_typed_func::<(), ()>(&mut store, "_initialize") {
        start.call(&mut store, ())?;
    }
    let run = instance.get_typed_func::<i32, i32>(&mut store, "plugin_run")?;
    let result = run.call(&mut store, 20)?;
    println!("plugin_run(20) = {result} (expected 41)");
    Ok(())
}
