//! draw.land profiles in JavaScript.
//!
//! A profile is a script that calls our library (`library.js`, over the
//! natives below); running it builds the operation graph the engine draws.
//! The script never touches geometry: by the time anything is drawn, the
//! interpreter is gone and only the graph is left.
//!
//! ```js
//! const bldg = features.osm({ building: true })
//! const foot = areas.polygons(bldg)
//! draw({ id: "bldg_fill", from: fills.fill({ in: foot, pattern: "zigzag", spacing: "0.5mm", per: "feature" }) })
//! ```
//!
//! Every node remembers the script line that made it, and every error (a
//! syntax error, a wrong argument, a bad reference found by validation) comes
//! back as `line N column M: …`, the form the web editor underlines.
//!
//! The sandbox is the bare language plus the library: no network, files,
//! timers or console, no `Date` or `Math.random` (the same script always
//! draws the same map), and loops and recursion are capped.

use boa_engine::{js_string, Context, JsError, JsNativeError, JsResult, JsValue, NativeFunction, Source};
use drawland_graph::{At, Arg, Grammar, Graph};
use serde_json::Value;
use std::cell::RefCell;
use std::fmt;
use std::rc::Rc;

/// The library's JavaScript face: a function of the library's calls (`dl`)
/// that returns the namespaces a script calls. The Node package runs it too.
pub const LIBRARY: &str = include_str!("library.js");

/// Where `view(spec, "name")` finds the script that draws in style `name`:
/// the host's own (the command line reads `name.js` beside the profile, the
/// web page its presets). `Err` says why there is none.
pub type Styles = Rc<dyn Fn(&str) -> Result<String, String>>;

/// What the host lends a run: a place over the one the script's `output`
/// gives (`(lat, lng)`: the command line's `--lat`/`--lng` and `--address`;
/// `world` answers for it), the styles views can name, and whether the
/// script is TypeScript.
#[derive(Clone, Default)]
pub struct Host {
    pub place: Option<(f64, f64)>,
    pub styles: Option<Styles>,
    /// TypeScript: its types are taken out before it runs, each replaced by
    /// blanks, so every line and column stays where it was written.
    pub typescript: bool,
}

/// `source` without its TypeScript types: the same text where they were
/// blanks, so positions in it are positions in the original (`drawland-ts`).
#[cfg(feature = "typescript")]
pub fn strip_types(source: &str) -> Result<String, ScriptError> {
    drawland_ts::strip_types(source).map_err(|e| ScriptError {
        message: e.message,
        at: e.at.map(|(l, c)| At(l, c)),
        address: None,
    })
}

/// Built without TypeScript (the web page strips types in a module of its own).
#[cfg(not(feature = "typescript"))]
pub fn strip_types(_: &str) -> Result<String, ScriptError> {
    Err(ScriptError { message: "TypeScript: this build runs JavaScript only; strip the types first".into(), at: None, address: None })
}

/// The name scripts run under; positions in other sources (the library) are
/// not the user's.
pub const SCRIPT_PATH: &str = "profile.js";
const LIBRARY_PATH: &str = "drawland:library";

/// The most loop iterations one loop may run, and the deepest recursion: a
/// mistaken `while (true)` ends with an error instead of hanging the page.
pub const LOOP_LIMIT: u64 = 1_000_000;
pub const RECURSION_LIMIT: usize = 256;

#[derive(Debug, Clone, PartialEq)]
pub struct ScriptError {
    pub message: String,
    pub at: Option<At>,
    /// The script asked `world` about a place its output names only by this
    /// address: look it up and run the script again with the place given.
    pub address: Option<String>,
}

impl fmt::Display for ScriptError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self.at {
            Some(at) => write!(f, "{at}: {}", self.message),
            None => f.write_str(&self.message),
        }
    }
}

impl std::error::Error for ScriptError {}

impl From<drawland_graph::Error> for ScriptError {
    fn from(e: drawland_graph::Error) -> Self {
        ScriptError { message: e.message, at: e.at, address: e.address }
    }
}

/// Run `source` and return the graph it built, validated. The place is the
/// one its own `output` gives, and views can name no style.
pub fn evaluate(source: &str) -> Result<Grammar, ScriptError> {
    evaluate_in(source, &Host::default())
}

/// Run `source` with what `host` lends it.
pub fn evaluate_in(source: &str, host: &Host) -> Result<Grammar, ScriptError> {
    finish(run_into(Graph::new(host.place), source, host))
}

/// A draft of `source`'s graph, for a page following a drag: the same graph
/// but with every `remap` warp left out, since sampling one runs the script's
/// function hundreds of times. `true` with it when something was left out, so
/// the page knows to run the script in full once the drag settles.
pub fn evaluate_draft(source: &str, host: &Host) -> Result<(Grammar, bool), ScriptError> {
    let (graph, result) = run_into(Graph::draft(host.place), source, host);
    let drafted = graph.drafted();
    finish((graph, result)).map(|g| (g, drafted))
}

fn finish((graph, result): (Graph, Result<(), ScriptError>)) -> Result<Grammar, ScriptError> {
    let address = graph.wanted_address().map(str::to_string);
    result.map_err(|e| ScriptError { address: address.clone().or(e.address.clone()), ..e })?;
    graph.finish().map_err(ScriptError::from)
}

/// Run `source` into `graph`, as more calls to the same graph: how a Python
/// or Rust profile draws a view in a JavaScript style by name. The graph back,
/// and whether the script ran.
pub fn run_into(graph: Graph, source: &str, host: &Host) -> (Graph, Result<(), ScriptError>) {
    let stripped;
    let source = if host.typescript {
        match strip_types(source) {
            Ok(js) => {
                stripped = js;
                stripped.as_str()
            }
            Err(e) => return (graph, Err(e)),
        }
    } else {
        source
    };
    let outer_graph = GRAPH.with(|g| g.borrow_mut().replace(graph));
    let outer_styles = STYLES.with(|s| std::mem::replace(&mut *s.borrow_mut(), host.styles.clone()));
    let result = run(source);
    let graph = GRAPH.with(|g| std::mem::replace(&mut *g.borrow_mut(), outer_graph)).expect("the graph run_into lent");
    STYLES.with(|s| *s.borrow_mut() = outer_styles);
    (graph, result)
}

fn run(source: &str) -> Result<(), ScriptError> {
    let mut ctx = Context::default();
    let limits = ctx.runtime_limits_mut();
    limits.set_loop_iteration_limit(LOOP_LIMIT);
    limits.set_recursion_limit(RECURSION_LIMIT);
    install(&mut ctx).map_err(|e| ScriptError { message: format!("library: {e}"), at: None, address: None })?;
    let script = Source::from_bytes(source).with_path(std::path::Path::new(SCRIPT_PATH));
    ctx.eval(script).map(|_| ()).map_err(|e| script_error(e, source, &mut ctx))
}

type Native = fn(&JsValue, &[JsValue], &mut Context) -> JsResult<JsValue>;

/// The sandbox, the natives and the library.
fn install(ctx: &mut Context) -> JsResult<()> {
    ctx.eval(Source::from_bytes(
        "delete globalThis.Date; delete Math.random; delete globalThis.WeakRef; delete globalThis.FinalizationRegistry;",
    ))?;
    let natives: [(&str, usize, Native); 15] = [
        ("call", 2, native_call),
        ("prop", 1, native_prop),
        ("get", 2, native_get),
        ("functions", 0, native_functions),
        ("properties", 0, native_properties),
        ("describe", 1, native_describe),
        ("outputBegin", 1, native_output_begin),
        ("outputWarps", 1, native_output_warps),
        ("remapGrid", 2, native_remap_grid),
        ("remapMesh", 5, native_remap_mesh),
        ("viewBegin", 1, native_view_begin),
        ("viewEnd", 1, native_view_end),
        ("viewStyle", 2, native_view_style),
        ("burin", 1, native_burin),
        ("fail", 1, native_fail),
    ];
    let dl = boa_engine::object::ObjectInitializer::new(ctx).build();
    for (name, arity, f) in natives {
        let function = boa_engine::object::FunctionObjectBuilder::new(ctx.realm(), NativeFunction::from_fn_ptr(f))
            .name(js_string!(name))
            .length(arity)
            .build();
        dl.set(js_string!(name), function, false, ctx)?;
    }
    let make = ctx.eval(Source::from_bytes(LIBRARY).with_path(std::path::Path::new(LIBRARY_PATH)))?;
    // What the library makes of `dl` becomes the script's globals, fixed.
    let install = ctx.eval(Source::from_bytes(
        "(function (make, dl) { const lib = make(dl); for (const [name, value] of Object.entries(lib)) Object.defineProperty(globalThis, name, { value, enumerable: false, writable: false, configurable: false }); })",
    ).with_path(std::path::Path::new(LIBRARY_PATH)))?;
    let install = install.as_callable().ok_or_else(|| JsNativeError::typ().with_message("library: install is not a function"))?.clone();
    install.call(&JsValue::undefined(), &[make, dl.into()], ctx)?;
    Ok(())
}

thread_local! {
    /// The graph the script builds.
    static GRAPH: RefCell<Option<Graph>> = const { RefCell::new(None) };
    /// The host's styles, for the run in progress.
    static STYLES: RefCell<Option<Styles>> = const { RefCell::new(None) };
    /// The last error a native raised, with the user's line: a JS error
    /// thrown from the library would otherwise point inside the library.
    static FAILURE: RefCell<Option<(String, Option<At>)>> = const { RefCell::new(None) };
}

/// The graph, at the user's line `at`.
fn with<T>(at: Option<At>, f: impl FnOnce(&mut Graph) -> T) -> T {
    GRAPH.with(|g| {
        let mut g = g.borrow_mut();
        let g = g.as_mut().expect("natives run inside a script");
        g.at = at;
        f(g)
    })
}

// ---------------------------------------------------------------------------
// Natives
// ---------------------------------------------------------------------------

/// Where the user's script is in the call stack: the innermost call in the
/// profile, skipping the library's own. Read from an `Error`'s `stack`, which
/// records call sites (the frames' own positions lag inside functions).
fn user_at(ctx: &mut Context) -> Option<At> {
    let error = ctx.intrinsics().constructors().error().constructor();
    let error = error.construct(&[], None, ctx).ok()?;
    let stack = error.get(js_string!("stack"), ctx).ok()?.to_string(ctx).ok()?.to_std_string_escaped();
    let marker = format!("({SCRIPT_PATH}:");
    stack.lines().find_map(|line| {
        let spot = &line[line.find(&marker)? + marker.len()..];
        let (l, c) = spot.trim_end_matches(')').split_once(':')?;
        Some(At(l.parse().ok()?, c.parse().ok()?))
    })
}

/// A JS error carrying `message`, remembered with where it happened.
fn throw(message: String, at: Option<At>) -> JsError {
    FAILURE.with(|f| *f.borrow_mut() = Some((message.clone(), at)));
    JsNativeError::typ().with_message(message).into()
}

fn fail(ctx: &mut Context, message: impl Into<String>) -> JsError {
    let at = user_at(ctx);
    throw(message.into(), at)
}

/// A library error as a JS error, at its own position or the user's line.
fn graph_error(e: drawland_graph::Error, ctx: &mut Context) -> JsError {
    let at = e.at.or_else(|| user_at(ctx));
    throw(e.message, at)
}

/// The argument at `i`, as the library takes it.
fn arg(args: &[JsValue], i: usize, ctx: &mut Context) -> JsResult<Arg> {
    let value = args.get(i).cloned().unwrap_or_default();
    Ok(Arg::decode(&value.to_json(ctx)?.unwrap_or(Value::Null)))
}

/// A list of arguments, without the ones left off the end.
fn arg_list(v: Arg) -> Vec<Arg> {
    let mut list = match v {
        Arg::List(l) => l,
        other => vec![other],
    };
    while list.last() == Some(&Arg::Null) {
        list.pop();
    }
    list
}

fn string_arg(args: &[JsValue], i: usize, ctx: &mut Context) -> JsResult<String> {
    Ok(args.get(i).cloned().unwrap_or_default().to_string(ctx)?.to_std_string_escaped())
}

fn give(v: &Arg, ctx: &mut Context) -> JsResult<JsValue> {
    JsValue::from_json(&v.encode(), ctx)
}

fn names(list: &[&str], ctx: &mut Context) -> JsResult<JsValue> {
    JsValue::from_json(&Value::from(list.to_vec()), ctx)
}

/// `dl.call(name, args)`: the library's function `name`.
fn native_call(_: &JsValue, args: &[JsValue], ctx: &mut Context) -> JsResult<JsValue> {
    let name = string_arg(args, 0, ctx)?;
    let list = arg_list(arg(args, 1, ctx)?);
    let at = user_at(ctx);
    match with(at, |g| g.call(&name, &list)) {
        Ok(v) => give(&v, ctx),
        Err(e) => Err(graph_error(e, ctx)),
    }
}

fn native_prop(_: &JsValue, args: &[JsValue], ctx: &mut Context) -> JsResult<JsValue> {
    let name = string_arg(args, 0, ctx)?;
    let at = user_at(ctx);
    match with(at, |g| g.get_property(&name)) {
        Ok(v) => give(&v, ctx),
        Err(e) => Err(graph_error(e, ctx)),
    }
}

fn native_get(_: &JsValue, args: &[JsValue], ctx: &mut Context) -> JsResult<JsValue> {
    let of = arg(args, 0, ctx)?;
    let part = string_arg(args, 1, ctx)?;
    let at = user_at(ctx);
    match with(at, |g| g.get(&of, &part)) {
        Ok(v) => give(&v, ctx),
        Err(e) => Err(graph_error(e, ctx)),
    }
}

fn native_functions(_: &JsValue, _: &[JsValue], ctx: &mut Context) -> JsResult<JsValue> {
    names(drawland_graph::FUNCTIONS, ctx)
}

fn native_properties(_: &JsValue, _: &[JsValue], ctx: &mut Context) -> JsResult<JsValue> {
    names(drawland_graph::PROPERTIES, ctx)
}

fn native_describe(_: &JsValue, args: &[JsValue], ctx: &mut Context) -> JsResult<JsValue> {
    let v = match args.first() {
        None => None,
        Some(v) if v.is_undefined() => None,
        Some(_) => Some(arg(args, 0, ctx)?),
    };
    Ok(JsValue::from(js_string!(drawland_graph::describe(v.as_ref()))))
}

/// `dl.outputBegin(spec)` → whether its `remap` warps are to be sampled.
fn native_output_begin(_: &JsValue, args: &[JsValue], ctx: &mut Context) -> JsResult<JsValue> {
    let spec = arg(args, 0, ctx)?;
    let at = user_at(ctx);
    let result = with(at, |g| match &spec {
        Arg::Map(o) => g.output_begin(o),
        other => g.call("output", &[other.clone()]).map(|_| false),
    });
    match result {
        Ok(sample) => Ok(JsValue::from(sample)),
        Err(e) => Err(graph_error(e, ctx)),
    }
}

fn native_output_warps(_: &JsValue, args: &[JsValue], ctx: &mut Context) -> JsResult<JsValue> {
    let done = arg_list(arg(args, 0, ctx)?);
    let at = user_at(ctx);
    match with(at, |g| g.output_warps(&done)) {
        Ok(()) => Ok(JsValue::undefined()),
        Err(e) => Err(graph_error(e, ctx)),
    }
}

/// `dl.remapGrid(warp, n)` → `{ columns, rows, points }`.
fn native_remap_grid(_: &JsValue, args: &[JsValue], ctx: &mut Context) -> JsResult<JsValue> {
    let warp = arg(args, 0, ctx)?;
    let n = args.get(1).cloned().unwrap_or_default().to_number(ctx)? as usize;
    let at = user_at(ctx);
    let warp = warp.as_map().cloned().unwrap_or_default();
    match with(at, |g| g.remap_grid(&warp, n)) {
        Ok((columns, rows, points)) => JsValue::from_json(&serde_json::json!({ "columns": columns, "rows": rows, "points": points }), ctx),
        Err(e) => Err(graph_error(e, ctx)),
    }
}

/// `dl.remapMesh(n, columns, rows, points, went)` → the mesh warp.
fn native_remap_mesh(_: &JsValue, args: &[JsValue], ctx: &mut Context) -> JsResult<JsValue> {
    let number = |i: usize, ctx: &mut Context| -> JsResult<usize> { Ok(args.get(i).cloned().unwrap_or_default().to_number(ctx)? as usize) };
    let (n, columns, rows) = (number(0, ctx)?, number(1, ctx)?, number(2, ctx)?);
    let points: Vec<[f64; 2]> = serde_json::from_value(args.get(3).cloned().unwrap_or_default().to_json(ctx)?.unwrap_or(Value::Null)).unwrap_or_default();
    let went = match arg(args, 4, ctx)? {
        Arg::List(l) => l,
        _ => vec![],
    };
    let at = user_at(ctx);
    match with(at, |g| g.remap_mesh(n, columns, rows, &points, &went)) {
        Ok(v) => give(&v, ctx),
        Err(e) => Err(graph_error(e, ctx)),
    }
}

fn native_view_begin(_: &JsValue, args: &[JsValue], ctx: &mut Context) -> JsResult<JsValue> {
    let spec = arg(args, 0, ctx)?;
    let at = user_at(ctx);
    match with(at, |g| g.view_begin(&spec)) {
        Ok(()) => Ok(JsValue::undefined()),
        Err(e) => Err(graph_error(e, ctx)),
    }
}

fn native_view_end(_: &JsValue, args: &[JsValue], ctx: &mut Context) -> JsResult<JsValue> {
    let ok = args.first().is_some_and(JsValue::to_boolean);
    let at = user_at(ctx);
    match with(at, |g| g.view_end(ok)) {
        Ok(v) => give(&v, ctx),
        Err(e) => Err(graph_error(e, ctx)),
    }
}

/// `dl.viewStyle(spec, name)`: a view drawn in a style the host lends by
/// name, run on its own (its top-level names never meet the page's).
fn native_view_style(_: &JsValue, args: &[JsValue], ctx: &mut Context) -> JsResult<JsValue> {
    let spec = arg(args, 0, ctx)?;
    let name = string_arg(args, 1, ctx)?;
    let at = user_at(ctx);
    let source = match STYLES.with(|s| s.borrow().clone()) {
        Some(styles) => styles(&name),
        None => Err("this host lends no styles by name; pass a function that draws".to_string()),
    };
    let source = source.map_err(|e| fail(ctx, format!("no style {name:?}: {e}")))?;
    if let Err(e) = with(at, |g| g.view_begin(&spec)) {
        return Err(graph_error(e, ctx));
    }
    let id = with(at, |g| g.view_id().unwrap_or_default().to_string());
    let result = run(&source);
    if let Err(e) = result {
        let _ = with(at, |g| g.view_end(false));
        return Err(throw(format!("view {id} ({name}): {e}"), at));
    }
    match with(at, |g| g.view_end(true)) {
        Ok(v) => give(&v, ctx),
        Err(e) => Err(graph_error(e, ctx)),
    }
}

/// `dl.burin(callback)` → the atlas JSON the burin document built.
///
/// The callback is a real function in this engine, so a burin document written
/// here has the same errors, positions and editor hints as anywhere else; the
/// atlas it builds is burin's own, and the engine draws it later.
fn native_burin(_: &JsValue, args: &[JsValue], ctx: &mut Context) -> JsResult<JsValue> {
    let callback = args.first().cloned().unwrap_or_default();
    match burin_map_script::run_builder(ctx, &callback, SCRIPT_PATH) {
        Ok(atlas) => Ok(JsValue::from(js_string!(atlas.to_string()))),
        Err(e) => Err(throw(e.message, e.at.map(|a| At(a.0, a.1)))),
    }
}

fn native_fail(_: &JsValue, args: &[JsValue], ctx: &mut Context) -> JsResult<JsValue> {
    let message = string_arg(args, 0, ctx)?;
    Err(fail(ctx, message))
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// The frame's old calls, and what replaced them: an older script that still
/// calls one is told the new name instead of only "not a callable function".
const RENAMED: [(&str, &str); 3] = [
    ("engine.frame(", "engine.frame() is now page.frame (a property: no parentheses)"),
    ("engine.scale(", "engine.scale() is now world.scale (a property: no parentheses)"),
    ("engine.paper(", "engine.paper() is now page: page.width and page.height"),
];

fn script_error(err: JsError, source: &str, ctx: &mut Context) -> ScriptError {
    let mut e = raw_script_error(err, source, ctx);
    if e.message.contains("not a callable function") {
        let line = e.at.and_then(|At(l, _)| source.lines().nth(l.saturating_sub(1) as usize)).unwrap_or_default();
        if let Some((_, hint)) = RENAMED.iter().find(|(old, _)| line.contains(old)) {
            e.message = format!("{}: {hint}", e.message);
        }
    }
    e
}

fn raw_script_error(err: JsError, source: &str, ctx: &mut Context) -> ScriptError {
    let native = err.try_native(ctx).ok();
    let message = native.as_ref().map(|n| n.message().to_string());
    if let (Some(message), Some((failed, at))) = (&message, FAILURE.with(|f| f.borrow_mut().take())) {
        if *message == failed {
            return ScriptError { message: failed, at, address: None };
        }
    }
    let text = err.to_string();
    let first = text.lines().next().unwrap_or_default();
    let kind = native.as_ref().map(|n| format!("{}", n.kind())).unwrap_or_default();
    let message = match (&message, kind.is_empty()) {
        (Some(m), false) => format!("{kind}: {m}"),
        _ => first.split(" (").next().unwrap_or(first).to_string(),
    };
    // `x is not defined` comes without a reliable position: the first use of
    // the name.
    if let Some(name) = message.strip_prefix("ReferenceError: ").and_then(|m| m.strip_suffix(" is not defined")) {
        if let Some(at) = first_use(source, name) {
            return ScriptError { at: Some(at), message, address: None };
        }
    }
    // Runtime errors: `TypeError: … (profile.js:2:7)`.
    let marker = format!("({SCRIPT_PATH}:");
    if let Some(i) = first.rfind(&marker) {
        let spot = &first[i + marker.len()..first.len().saturating_sub(1)];
        if let Some((line, column)) = spot.split_once(':') {
            if let (Ok(line), Ok(column)) = (line.parse(), column.parse()) {
                return ScriptError { message, at: Some(At(line, column)), address: None };
            }
        }
    }
    // Syntax errors: `… at line 1, col 9`.
    if let Some(i) = first.rfind(" at line ") {
        let spot = &first[i + 9..];
        if let Some((line, column)) = spot.split_once(", col ") {
            let column: String = column.chars().take_while(char::is_ascii_digit).collect();
            if let (Ok(line), Ok(column)) = (line.trim().parse(), column.parse()) {
                let message = format!("SyntaxError: {}", first[..i].trim_start_matches("SyntaxError: "));
                return ScriptError { message, at: Some(At(line, column)), address: None };
            }
        }
    }
    ScriptError { message, at: None, address: None }
}

/// Where `name` first appears as a whole word.
fn first_use(source: &str, name: &str) -> Option<At> {
    let word = |c: char| c.is_alphanumeric() || c == '_' || c == '$';
    for (i, line) in source.lines().enumerate() {
        let mut from = 0;
        while let Some(j) = line[from..].find(name) {
            let start = from + j;
            let end = start + name.len();
            let before = line[..start].chars().next_back();
            let after = line[end..].chars().next();
            if !before.is_some_and(|c| word(c) || c == '.') && !after.is_some_and(word) {
                return Some(At(i as u32 + 1, line[..start].chars().count() as u32 + 1));
            }
            from = end;
        }
    }
    None
}

/// Every function the library defines, as `areas.union` or `draw`, sorted
/// (properties such as `page.width` are not calls, and not listed): what
/// `drawland.d.ts` must declare, read from the library as a script sees it.
pub fn library_names() -> Result<Vec<String>, ScriptError> {
    let outer = GRAPH.with(|g| g.borrow_mut().replace(Graph::new(None)));
    let mut ctx = Context::default();
    let listed = install(&mut ctx).map_err(|e| ScriptError { message: format!("library: {e}"), at: None, address: None }).and_then(|_| {
        ctx.eval(Source::from_bytes(
            "JSON.stringify(['features','areas','engine','page','world','lines','fills','burin','ink','modules','draw','compose','output','title','view'].flatMap((n) => typeof globalThis[n] === 'function' ? [n] : [n, ...Object.keys(globalThis[n]).filter((k) => typeof Object.getOwnPropertyDescriptor(globalThis[n], k).value === 'function').map((k) => n + '.' + k)]))",
        ))
        .and_then(|v| v.to_string(&mut ctx))
        .map_err(|e| ScriptError { message: e.to_string(), at: None, address: None })
    });
    GRAPH.with(|g| *g.borrow_mut() = outer);
    let mut names: Vec<String> = serde_json::from_str(&listed?.to_std_string_escaped()).unwrap_or_default();
    names.sort();
    Ok(names)
}

#[cfg(test)]
mod catalog_dts;
#[cfg(test)]
mod tests;
