//! The graph being built: its nodes, draws and composition, the output and
//! the frame `page` and `world` read from it, and the views drawn into it,
//! each a graph of its own.

use crate::arg::{Arg, Obj};
use drawland_core::frame::Frame;
use drawland_core::grammar::{self, At, Clip, Compose, Def, Draw, Grammar, Id, Mask, Name, Section, Target};
use drawland_core::profile::{OutputSpec, StrokeSpec};
use serde_json::Value;
use std::collections::HashMap;
use std::fmt;

/// How deep views may nest (a view drawing a style that has views of its own).
pub const VIEW_DEPTH: usize = 4;

/// What went wrong, and where in the script when the caller said
/// ([`Graph::at`]).
#[derive(Clone, Debug, PartialEq)]
pub struct Error {
    pub message: String,
    pub at: Option<At>,
    /// The script asked about a place its output names only by this address:
    /// look it up and build again with the place given ([`Graph::new`]).
    pub address: Option<String>,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self.at {
            Some(at) => write!(f, "{at}: {}", self.message),
            None => f.write_str(&self.message),
        }
    }
}

impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;

/// A view's id, corner, ink, and the output keys its spec sets.
#[derive(Clone, Debug)]
pub(crate) struct ViewHead {
    pub id: String,
    pub at: (f64, f64),
    pub keys: serde_json::Map<String, Value>,
    pub ink: Option<String>,
}

/// One graph's worth: the page's, or a view's.
#[derive(Default)]
pub(crate) struct Builder {
    pub title: Option<String>,
    pub output: Option<OutputSpec>,
    pub defs: Vec<Def>,
    pub count: usize,
    pub draw: Vec<Draw>,
    pub paint: Vec<drawland_core::paint::Paint>,
    pub order: Vec<Id>,
    pub order_at: Option<At>,
    pub masks: Vec<(String, Mask)>,
    pub clip: Option<Clip>,
    pub optimize: Vec<drawland_core::optimize::OptimizeStep>,
    pub views: Vec<grammar::View>,
    pub view: Option<ViewHead>,
    /// The host's place, `(lat, lng)`, over the one the output asks.
    pub place: Option<(f64, f64)>,
    pub frame: Option<Frame>,
    /// The parts of modules made so far, by what made them.
    pub memo: HashMap<String, Arg>,
}

/// A draw.land graph, built a call at a time.
///
/// The library's functions are [`Graph::call`] by name (`"areas.union"`),
/// as JavaScript and Python call them, or the typed methods of [`crate::api`]
/// for Rust. [`Graph::finish`] gives the graph the engine draws.
pub struct Graph {
    pub(crate) stack: Vec<Builder>,
    /// Where in the script the call being made is, for the graph's nodes and
    /// for errors: set by the caller before each call.
    pub at: Option<At>,
    /// `Some` in a draft: whether a `remap` has been left out yet.
    pub(crate) draft: Option<bool>,
    pub(crate) wanted_address: Option<String>,
}

impl Default for Graph {
    fn default() -> Self {
        Graph::new(None)
    }
}

impl Graph {
    /// An empty graph. `place` (`(lat, lng)`) goes over whatever place the
    /// output asks: `world` answers for it.
    pub fn new(place: Option<(f64, f64)>) -> Graph {
        Graph { stack: vec![Builder { place, ..Builder::default() }], at: None, draft: None, wanted_address: None }
    }

    /// A draft graph, for a page following a drag: `output` leaves its
    /// `remap` warps out, since sampling one runs a function hundreds of
    /// times. [`Graph::drafted`] says whether one was.
    pub fn draft(place: Option<(f64, f64)>) -> Graph {
        Graph { draft: Some(false), ..Graph::new(place) }
    }

    /// Whether a draft left a `remap` out.
    pub fn drafted(&self) -> bool {
        self.draft == Some(true)
    }

    /// The address the script needed coordinates for, if it asked `world`
    /// about an output that names only one.
    pub fn wanted_address(&self) -> Option<&str> {
        self.wanted_address.as_deref()
    }

    pub(crate) fn top(&mut self) -> &mut Builder {
        self.stack.last_mut().expect("a graph always has its page")
    }

    pub(crate) fn top_ref(&self) -> &Builder {
        self.stack.last().expect("a graph always has its page")
    }

    /// An error at the call being made.
    pub fn error(&self, message: impl Into<String>) -> Error {
        Error { message: message.into(), at: self.at, address: None }
    }

    /// A new node in `section`, from its operation as the graph writes it
    /// (`{ "tools_union": [...] }`): its reference.
    pub(crate) fn node(&mut self, section: Section, op: Value) -> Result<String> {
        let key = op.as_object().and_then(|o| o.keys().next().cloned()).unwrap_or_default();
        let parsed = grammar::op_from_json(section, op).map_err(|e| self.error(strip_kind(&e.to_string())))?;
        let short = key
            .strip_prefix("tools_")
            .or_else(|| key.strip_prefix("engine_"))
            .or_else(|| key.strip_suffix("_select"))
            .unwrap_or(&key)
            .to_string();
        let name = match key.strip_prefix("module_") {
            Some(module) => {
                if self.top_ref().defs.iter().any(|d| d.name == module) {
                    return Err(self.error(format!("modules.{module} is called twice; call it once and reuse its outputs")));
                }
                module.to_string()
            }
            None => {
                let b = self.top();
                b.count += 1;
                format!("{short}_{}", b.count)
            }
        };
        let def = Def { name, op: parsed, at: self.at };
        let reference = def.reference().to_string();
        self.top().defs.push(def);
        Ok(reference)
    }

    pub(crate) fn add_draw(&mut self, from: &str, id: Option<String>, stroke: Option<&Arg>) -> Result<String> {
        let from = target(from).map_err(|e| self.error(format!("draw: {e}")))?;
        let stroke = match stroke {
            None => None,
            Some(s) => {
                let s = s.to_json();
                Some(serde_json::from_value::<StrokeSpec>(s.clone()).map_err(|e| {
                    self.error(format!("draw: stroke {s}: {e}; write solid, dash, dotted, none or {{ style, dash, gap, mark, color, width, cap, join }}"))
                })?)
            }
        };
        let draw = Draw { id, from, stroke, at: self.at };
        let group = draw.group();
        if self.top_ref().draw.iter().any(|d| d.group() == group) {
            return Err(self.error(format!("draw: the id {group:?} is already drawn")));
        }
        self.top().draw.push(draw);
        Ok(group)
    }

    /// A layer of the screen: `from` (a reference) painted as `look` says.
    pub(crate) fn add_paint(&mut self, id: String, from: &str, look: drawland_core::paint::Style, shown: drawland_core::paint::Shown, halo: drawland_core::paint::Halo) -> Result<String> {
        let from = target(from).map_err(|e| self.error(format!("paint: {e}")))?;
        if self.top_ref().paint.iter().any(|p| p.id == id) {
            return Err(self.error(format!("paint: the id {id:?} is already painted")));
        }
        let at = self.at;
        self.top().paint.push(drawland_core::paint::Paint { id: id.clone(), from, look, shown, halo, at });
        Ok(id)
    }

    pub(crate) fn set_order(&mut self, ids: Vec<String>) {
        let at = self.at;
        let b = self.top();
        b.order = ids.into_iter().map(Id).collect();
        b.order_at = at;
    }

    pub(crate) fn add_mask(&mut self, name: Option<String>, from: &str, occludes: Vec<String>) -> Result<()> {
        let from = target(from).map_err(|e| self.error(format!("compose.mask: {e}")))?;
        let name = name.unwrap_or_else(|| format!("mask_{}", self.top_ref().masks.len() + 1));
        if self.top_ref().masks.iter().any(|(n, _)| *n == name) {
            return Err(self.error(format!("compose.mask: the name {name:?} is already used")));
        }
        let at = self.at;
        self.top().masks.push((name, Mask { from, occludes: occludes.into_iter().map(Id).collect(), at }));
        Ok(())
    }

    pub(crate) fn set_clip(&mut self, clip: Clip) {
        self.top().clip = Some(clip);
    }

    pub(crate) fn clip_areas(&self, refs: &[String]) -> Result<Clip> {
        refs.iter()
            .map(|r| Name::parse(r))
            .collect::<std::result::Result<Vec<_>, _>>()
            .map(Clip::Areas)
            .map_err(|e| self.error(format!("compose.clip: {e}")))
    }

    pub(crate) fn add_optimize(&mut self, step: Value) -> Result<()> {
        let raw: grammar::RawOptimizeStep =
            serde_json::from_value(step).map_err(|e| self.error(format!("compose: {}", strip_kind(&e.to_string()))))?;
        let step = drawland_core::optimize::OptimizeStep {
            layers: raw.layers.into_iter().map(Id).collect(),
            except: raw.except.into_iter().map(Id).collect(),
            op: raw.op,
            at: self.at,
        };
        self.top().optimize.push(step);
        Ok(())
    }

    /// The output as it will be: inside a view, the view's place and
    /// rectangle win over what the style asks, so a style drawn in a view
    /// keeps its look (pens, patterns) at the view's place; the host's place
    /// wins over the script's.
    pub(crate) fn set_output(&mut self, spec: &Obj) -> Result<()> {
        let mut spec = Arg::Map(spec.clone()).to_json();
        if let (Some(obj), Some(keys)) = (spec.as_object_mut(), self.top_ref().view.as_ref().map(|v| v.keys.clone())) {
            for k in ["address", "slices"] {
                obj.remove(k);
            }
            obj.extend(keys);
        }
        if let (Some(obj), Some((lat, lng))) = (spec.as_object_mut(), self.top_ref().place) {
            obj.remove("address");
            obj.insert("lat".into(), Value::from(lat));
            obj.insert("lng".into(), Value::from(lng));
        }
        self.top().frame = None;
        if let Some(obj) = spec.as_object() {
            if obj.contains_key("screen") {
                if let Some(k) = ["width", "height", "padding", "slices"].into_iter().find(|k| obj.contains_key(*k)) {
                    return Err(self.error(format!("output: a screen sets its own size; leave out {k}")));
                }
            }
        }
        let mut output: OutputSpec = serde_json::from_value(spec).map_err(|e| self.error(format!("output: {e}")))?;
        output.fit_screen().map_err(|e| self.error(e))?;
        if self.top().output.replace(output).is_some() {
            return Err(self.error("output is set twice"));
        }
        Ok(())
    }

    /// The output's warps replaced, so `page` and `world` answer through
    /// them: how a `remap` is sampled through the warps before it.
    pub(crate) fn set_warp(&mut self, warps: &Arg) -> Result<()> {
        let warps: drawland_core::warp::Warps =
            serde_json::from_value(warps.to_json()).map_err(|e| self.error(format!("output.warp: {e}")))?;
        let b = self.top();
        b.frame = None;
        if b.output.as_mut().map(|o| o.warp = warps).is_none() {
            return Err(self.error("output.warp: call output({ … }) first"));
        }
        Ok(())
    }

    /// Whether this is a draft, which leaves `remap` warps out; asking in
    /// one marks the graph drafted.
    pub(crate) fn ask_draft(&mut self) -> bool {
        match self.draft {
            Some(_) => {
                self.draft = Some(true);
                true
            }
            None => false,
        }
    }

    /// The output `page` and `world` describe: the script's own, or inside
    /// a view whose style never called `output`, the view's keys.
    pub fn current_output(&self, asked: &str) -> Result<OutputSpec> {
        let b = self.top_ref();
        match (&b.output, &b.view) {
            (Some(o), _) => Ok(o.clone()),
            (None, Some(v)) => {
                serde_json::from_value(Value::Object(v.keys.clone())).map_err(|e| self.error(format!("{asked}: {e}")))
            }
            (None, None) => Err(self.error(format!("{asked}: call output({{ … }}) first"))),
        }
    }

    /// The frame `world` and the conversions answer from, worked out once
    /// per output.
    pub fn frame(&mut self, asked: &str) -> Result<Frame> {
        if let Some(f) = self.top_ref().frame.clone() {
            return Ok(f);
        }
        let spec = self.current_output(asked)?;
        let (lat, lng) = match (spec.lat, spec.lng, &spec.address) {
            (Some(lat), Some(lng), _) => (lat, lng),
            (_, _, Some(address)) => {
                self.wanted_address = Some(address.clone());
                let mut e = self.error(format!(
                    "{asked}: the output's place is the address {address:?}; a script can't look it up, so give the output a lat and lng"
                ));
                e.address = Some(address.clone());
                return Err(e);
            }
            _ => return Err(self.error(format!("{asked}: the output has no place; give it a lat and lng"))),
        };
        let frame = Frame::new(&spec, (lng, lat));
        self.top().frame = Some(frame.clone());
        Ok(frame)
    }

    /// `view({ … }, style)` begins: the calls until [`Graph::view_end`]
    /// build the view's own graph.
    pub fn view_begin(&mut self, spec: &Arg) -> Result<()> {
        let head = view_keys(spec).map_err(|e| self.error(e))?;
        if self.stack.len() > VIEW_DEPTH {
            return Err(self.error("views nest too deep"));
        }
        if self.top_ref().views.iter().any(|v| v.id == head.id) {
            return Err(self.error(format!("two views are called {:?}", head.id)));
        }
        self.stack.push(Builder { view: Some(head), ..Builder::default() });
        Ok(())
    }

    /// The view's graph is done: it joins the page's views (`ok`), or is
    /// dropped (its style failed). Its id.
    pub fn view_end(&mut self, ok: bool) -> Result<Arg> {
        if self.stack.len() < 2 {
            return Err(self.error("view_end without view_begin"));
        }
        let inner = self.stack.pop().expect("checked above");
        let head = inner.view.clone().expect("a view's builder");
        if !ok {
            return Ok(Arg::Null);
        }
        let grammar = inner.finish().map_err(|e| Error { message: format!("view {}: {}", head.id, e.message), at: self.at, address: None })?;
        self.top().views.push(grammar::View { id: head.id.clone(), at: head.at, grammar: Box::new(grammar), ink: head.ink });
        Ok(Arg::Str(head.id))
    }

    /// The id of the view being built, if any.
    pub fn view_id(&self) -> Option<&str> {
        self.top_ref().view.as_ref().map(|v| v.id.as_str())
    }

    pub(crate) fn set_title(&mut self, text: String) {
        self.top().title = Some(text);
    }

    /// The graph built: validated, ready for the engine.
    pub fn finish(mut self) -> Result<Grammar> {
        if self.stack.len() != 1 {
            return Err(self.error("a view was begun and never ended"));
        }
        self.stack.pop().expect("the page").finish()
    }
}

impl Builder {
    fn finish(self) -> Result<Grammar> {
        let output = match (self.output, &self.view) {
            (Some(o), _) => o,
            // A view whose style never called `output`: its own keys are all.
            (None, Some(v)) => serde_json::from_value(Value::Object(v.keys.clone()))
                .map_err(|e| Error { message: format!("view: {e}"), at: None, address: None })?,
            (None, None) => serde_json::from_value(Value::Object(Default::default())).expect("an empty output is valid"),
        };
        let mut sections: Vec<(Section, Vec<Def>)> = Vec::new();
        for section in Section::ALL {
            let defs: Vec<Def> = self.defs.iter().filter(|d| d.op.section() == section).cloned().collect();
            if !defs.is_empty() {
                sections.push((section, defs));
            }
        }
        let groups: Vec<String> = self.draw.iter().map(Draw::group).collect();
        if let Some(missing) = self.order.iter().find(|id| !groups.contains(&id.0)) {
            return Err(Error {
                message: format!("compose.order names {:?}, which no draw writes", missing.0),
                at: self.order_at,
                address: None,
            });
        }
        let compose = Compose {
            order: self.order,
            masks: self.masks,
            clip: Default::default(),
            clip_default: self.clip.unwrap_or(Clip::Auto),
            optimize: self.optimize,
        };
        let graph = Grammar { title: self.title, output, sections, draw: self.draw, compose, paint: self.paint, views: self.views };
        grammar::validate(&graph).map_err(|e| {
            let message = e.to_string();
            Error { at: find_at(&message), message, address: None }
        })?;
        Ok(graph)
    }
}

/// `$lines.x`, `$fills.x` or `roads.outlines`.
fn target(text: &str) -> std::result::Result<Target, String> {
    if text.starts_with('$') {
        return Name::parse(text).map(|name| Target { module: None, name });
    }
    match text.split_once('.') {
        Some((module, output)) => Ok(Target {
            module: Some(module.to_string()),
            name: Name { section: Section::Lines, name: output.to_string() },
        }),
        None => Err(format!("{text:?} is not a reference")),
    }
}

/// The output keys a view's spec sets: `{ id, at: [x, y], size: [w, h] }` in
/// page mm, and the place (`lat`, `lng`, `scale`, `bearing`, `tileZoom`,
/// `projection`).
fn view_keys(spec: &Arg) -> std::result::Result<ViewHead, String> {
    let spec = spec.to_json();
    let o = spec.as_object().ok_or("view takes { at, size, lat, lng, scale, … }")?;
    let pair = |k: &str| -> std::result::Result<(f64, f64), String> {
        match o.get(k).and_then(Value::as_array).map(|a| a.iter().map(Value::as_f64).collect::<Vec<_>>()) {
            Some(v) if v.len() == 2 && v.iter().all(Option::is_some) => Ok((v[0].unwrap(), v[1].unwrap())),
            _ => Err(format!("view.{k} is [x, y] in millimetres")),
        }
    };
    let at = pair("at")?;
    let (w, h) = pair("size")?;
    if w <= 0.0 || h <= 0.0 {
        return Err("view.size must be positive".into());
    }
    let id = o.get("id").and_then(Value::as_str).ok_or("view.id names the view, as compose.mask occludes it")?.to_string();
    let mut keys = serde_json::Map::new();
    keys.insert("width".into(), Value::String(format!("{w}mm")));
    keys.insert("height".into(), Value::String(format!("{h}mm")));
    keys.insert("padding".into(), Value::from(0));
    for k in ["lat", "lng", "scale", "bearing", "tileZoom", "projection"] {
        if let Some(v) = o.get(k) {
            keys.insert(k.into(), v.clone());
        }
    }
    let ink = match o.get("ink") {
        None | Some(Value::Null) => None,
        Some(Value::String(c)) if !c.trim().is_empty() => Some(c.trim().to_string()),
        Some(v) => return Err(format!("view.ink is a colour like \"#e4572e\", got {v}")),
    };
    Ok(ViewHead { id, at, keys, ink })
}

/// `profile: …` and the like: the grammar's own prefixes mean nothing to a
/// script author.
pub(crate) fn strip_kind(message: &str) -> String {
    message.strip_prefix("profile: ").unwrap_or(message).to_string()
}

/// The first `line N column M` in a message.
pub fn find_at(message: &str) -> Option<At> {
    let start = message.find("line ")?;
    let rest = &message[start + 5..];
    let (line, rest) = rest.split_once(" column ")?;
    let column: String = rest.chars().take_while(char::is_ascii_digit).collect();
    Some(At(line.parse().ok()?, column.parse().ok()?))
}
