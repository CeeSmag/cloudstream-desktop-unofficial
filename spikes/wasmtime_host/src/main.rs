//! Spike host: runs a Kotlin/Wasm provider through a string + DOM + HTTP bridge.
use wasmtime::error::Context;
use wasmtime::{bail, format_err, Result};
use scraper::{ElementRef, Html, Selector};
use wasmtime::{Caller, Config, Engine, Extern, Linker, Module, Store};
use wasmtime_wasi::p1::{self, WasiP1Ctx};
use wasmtime_wasi::WasiCtxBuilder;

const SEARCH_FIXTURE: &str = r#"<html><body>
<div class="item"><a class="title" href="/anime/1">Naruto</a></div>
<div class="item"><a class="title" href="/anime/2">Naruto Shippuden</a></div>
<div class="ad"><a class="title" href="/ad">Buy now</a></div></body></html>"#;

struct HostState {
    wasi: WasiP1Ctx,
    input: Vec<u8>,
    output: Vec<u8>,
    docs: Vec<String>, // raw HTML; scraper::Html is !Send so it is re-parsed per call (spike only)
    nodes: Vec<(usize, ego_tree::NodeId)>,
    lists: Vec<Vec<i32>>,
}

fn memory(caller: &mut Caller<'_, HostState>) -> Result<wasmtime::Memory> {
    match caller.get_export("memory") {
        Some(Extern::Memory(memory)) => Ok(memory),
        _ => bail!("plugin exports no memory"),
    }
}

fn read_str(caller: &mut Caller<'_, HostState>, ptr: i32, len: i32) -> Result<String> {
    let memory = memory(caller)?;
    let bytes = memory.data(&*caller).get(ptr as usize..(ptr + len) as usize).context("oob read")?;
    Ok(String::from_utf8_lossy(bytes).into_owned())
}

fn write_bytes(caller: &mut Caller<'_, HostState>, ptr: i32, cap: i32, bytes: &[u8]) -> Result<i32> {
    if bytes.len() > cap as usize {
        return Ok(-1);
    }
    let memory = memory(caller)?;
    let target = memory.data_mut(&mut *caller).get_mut(ptr as usize..ptr as usize + bytes.len()).context("oob write")?;
    target.copy_from_slice(bytes);
    Ok(bytes.len() as i32)
}

fn with_element<T>(state: &HostState, handle: i32, f: impl FnOnce(ElementRef<'_>) -> T) -> Result<T> {
    let (doc, id) = *state.nodes.get(handle as usize).context("bad element handle")?;
    let html = Html::parse_document(state.docs.get(doc).context("bad doc")?);
    let node = html.tree.get(id).context("bad node")?;
    Ok(f(ElementRef::wrap(node).context("not an element")?))
}

fn register(linker: &mut Linker<HostState>) -> Result<()> {
    linker.func_wrap("host", "host_read_input", |mut c: Caller<'_, HostState>, ptr: i32, cap: i32| -> Result<i32> {
        let input = c.data().input.clone();
        write_bytes(&mut c, ptr, cap, &input)
    })?;
    linker.func_wrap("host", "host_write_output", |mut c: Caller<'_, HostState>, ptr: i32, len: i32| -> Result<()> {
        let text = read_str(&mut c, ptr, len)?;
        c.data_mut().output = text.into_bytes();
        Ok(())
    })?;
    linker.func_wrap("host", "host_http_get_document", |mut c: Caller<'_, HostState>, ptr: i32, len: i32| -> Result<i32> {
        let url = read_str(&mut c, ptr, len)?;
        println!("  [host] GET {url}");
        if url != "https://mock.test/search?q=naruto" {
            return Ok(-1);
        }
        let state = c.data_mut();
        state.docs.push(SEARCH_FIXTURE.to_owned());
        let doc = state.docs.len() - 1;
        let root = Html::parse_document(SEARCH_FIXTURE).root_element().id();
        state.nodes.push((doc, root));
        Ok(state.nodes.len() as i32 - 1)
    })?;
    linker.func_wrap("host", "host_select", |mut c: Caller<'_, HostState>, handle: i32, ptr: i32, len: i32| -> Result<i32> {
        let selector = Selector::parse(&read_str(&mut c, ptr, len)?).map_err(|e| format_err!("{e}"))?;
        let state = c.data_mut();
        let doc = state.nodes.get(handle as usize).context("bad handle")?.0;
        let found: Vec<_> = with_element(state, handle, |e| e.select(&selector).map(|n| n.id()).collect())?;
        let mut handles = Vec::new();
        for id in found {
            state.nodes.push((doc, id));
            handles.push(state.nodes.len() as i32 - 1);
        }
        state.lists.push(handles);
        Ok(state.lists.len() as i32 - 1)
    })?;
    linker.func_wrap("host", "host_list_len", |c: Caller<'_, HostState>, list: i32| -> i32 {
        c.data().lists.get(list as usize).map_or(0, |l| l.len() as i32)
    })?;
    linker.func_wrap("host", "host_list_item", |c: Caller<'_, HostState>, list: i32, index: i32| -> i32 {
        c.data().lists.get(list as usize).and_then(|l| l.get(index as usize)).copied().unwrap_or(-1)
    })?;
    linker.func_wrap("host", "host_text", |mut c: Caller<'_, HostState>, handle: i32, ptr: i32, cap: i32| -> Result<i32> {
        let text = with_element(c.data(), handle, |e| e.text().collect::<String>())?;
        write_bytes(&mut c, ptr, cap, text.trim().as_bytes())
    })?;
    linker.func_wrap("host", "host_attr", |mut c: Caller<'_, HostState>, handle: i32, np: i32, nl: i32, ptr: i32, cap: i32| -> Result<i32> {
        let name = read_str(&mut c, np, nl)?;
        let value = with_element(c.data(), handle, |e| e.value().attr(&name).map(str::to_owned))?;
        match value {
            Some(v) => write_bytes(&mut c, ptr, cap, v.as_bytes()),
            None => Ok(-1),
        }
    })?;
    Ok(())
}

fn run(engine: &Engine, module: &Module, export: &str, input: &str) -> Result<(i32, String)> {
    let mut linker: Linker<HostState> = Linker::new(engine);
    p1::add_to_linker_sync(&mut linker, |s| &mut s.wasi)?;
    register(&mut linker)?;
    let state = HostState {
        wasi: WasiCtxBuilder::new().build_p1(),
        input: input.as_bytes().to_vec(),
        output: Vec::new(),
        docs: Vec::new(),
        nodes: Vec::new(),
        lists: Vec::new(),
    };
    let mut store = Store::new(engine, state);
    let instance = linker.instantiate(&mut store, module)?;
    instance.get_typed_func::<(), ()>(&mut store, "_start")?.call(&mut store, ())?;
    let code = instance.get_typed_func::<(), i32>(&mut store, export)?.call(&mut store, ())?;
    Ok((code, String::from_utf8_lossy(&store.data().output).into_owned()))
}

fn main() -> Result<()> {
    let path = std::env::args().nth(1).context("usage: wasmtime_host <file.wasm>")?;
    let mut config = Config::new();
    config.wasm_gc(true).wasm_function_references(true).wasm_exceptions(true);
    let engine = Engine::new(&config)?;
    let module = Module::from_file(&engine, &path)?;
    for (label, export, input) in [
        ("search hit", "plugin_search", "naruto"),
        ("search miss (handled failure)", "plugin_search", "zzz"),
        ("plugin crash (trap)", "plugin_crash", ""),
        ("search after crash", "plugin_search", "naruto"),
        ("suspend search via coroutines", "plugin_suspend_search", ""),
    ] {
        match run(&engine, &module, export, input) {
            Ok((code, out)) => println!("{label}: code={code} output={out}"),
            Err(error) => println!("{label}: HOST SURVIVED error: {}", error.root_cause()),
        }
    }
    Ok(())
}
