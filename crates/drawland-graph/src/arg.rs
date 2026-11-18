//! The values the library takes and gives, whatever language calls it: the
//! plain data a script writes (numbers, strings, lists, objects) and the
//! handles the library returns (features, areas, lines, and the lazy parts
//! of a module). JavaScript, Python and Rust each turn their own values into
//! these and back.

use serde_json::Value;

/// A value passed to or returned by the library.
#[derive(Clone, Debug, PartialEq)]
pub enum Arg {
    Null,
    Bool(bool),
    Num(f64),
    Str(String),
    List(Vec<Arg>),
    Map(Obj),
    /// A set of map features (`features.*`).
    Features(String),
    /// An area (`areas.*`, `page.frame`, a module's `.area`). `mask` is what
    /// `compose.mask` hides with: the area itself, or the module's own.
    Area { r: String, mask: String },
    /// Lines (`lines.*`, `fills.fill`, a module's output). A module's output
    /// can be drawn, not transformed.
    Lines { r: String, module: Option<String> },
    /// A module's part made only when it is asked for (`graticule.keys`,
    /// `labels.area`, a label rule's bucket): read it with `Graph::get`.
    Lazy(Lazy),
    /// A function, where the library takes none (it says so).
    Func,
}

/// An object: keys in the order they were written.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Obj(pub Vec<(String, Arg)>);

impl Obj {
    pub fn new() -> Obj {
        Obj(Vec::new())
    }

    /// The value at `key`; `None` when the key is not there.
    pub fn get(&self, key: &str) -> Option<&Arg> {
        self.0.iter().find(|(k, _)| k == key).map(|(_, v)| v)
    }

    pub fn has(&self, key: &str) -> bool {
        self.get(key).is_some()
    }

    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.0.iter().map(|(k, _)| k.as_str())
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Set `key`, in place if it is there, at the end if not.
    pub fn set(&mut self, key: &str, value: Arg) {
        match self.0.iter_mut().find(|(k, _)| k == key) {
            Some((_, v)) => *v = value,
            None => self.0.push((key.to_string(), value)),
        }
    }

    pub fn remove(&mut self, key: &str) -> Option<Arg> {
        let i = self.0.iter().position(|(k, _)| k == key)?;
        Some(self.0.remove(i).1)
    }

    /// Everything but `keys`, in order.
    pub fn without(&self, keys: &[&str]) -> Obj {
        Obj(self.0.iter().filter(|(k, _)| !keys.contains(&k.as_str())).cloned().collect())
    }
}

/// Which part of a module a [`Lazy`] is, and what it was made from.
#[derive(Clone, Debug, PartialEq)]
pub enum Lazy {
    /// `modules.graticule(…)`: its look, as the graph writes it.
    Graticule(Value),
    /// `modules.labels(…)`: the labels' node, and the rule ids it has.
    Labels { r: String, rules: Vec<String> },
    /// One rule's bucket of `modules.labels`.
    LabelRule { r: String, rule: String },
}

impl Arg {
    pub fn str(s: &str) -> Arg {
        Arg::Str(s.to_string())
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Arg::Str(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_num(&self) -> Option<f64> {
        match self {
            Arg::Num(n) => Some(*n),
            _ => None,
        }
    }

    /// A finite number.
    pub fn as_finite(&self) -> Option<f64> {
        self.as_num().filter(|n| n.is_finite())
    }

    pub fn as_map(&self) -> Option<&Obj> {
        match self {
            Arg::Map(o) => Some(o),
            _ => None,
        }
    }

    pub fn as_list(&self) -> Option<&[Arg]> {
        match self {
            Arg::List(l) => Some(l),
            _ => None,
        }
    }

    /// JavaScript's truthiness, for the flags a script may write as anything.
    pub fn truthy(&self) -> bool {
        match self {
            Arg::Null => false,
            Arg::Bool(b) => *b,
            Arg::Num(n) => *n != 0.0 && !n.is_nan(),
            Arg::Str(s) => !s.is_empty(),
            _ => true,
        }
    }

    /// The value as the graph's JSON writes it. A handle becomes its
    /// reference (an area its own, not its mask).
    pub fn to_json(&self) -> Value {
        match self {
            Arg::Null | Arg::Func => Value::Null,
            Arg::Bool(b) => Value::Bool(*b),
            Arg::Num(n) => number(*n),
            Arg::Str(s) => Value::String(s.clone()),
            Arg::List(l) => Value::Array(l.iter().map(Arg::to_json).collect()),
            Arg::Map(o) => Value::Object(o.0.iter().map(|(k, v)| (k.clone(), v.to_json())).collect()),
            Arg::Features(r) | Arg::Area { r, .. } | Arg::Lines { r, .. } => Value::String(r.clone()),
            Arg::Lazy(_) => Value::Null,
        }
    }

    /// Plain JSON as a value: no handles in it.
    pub fn from_json(v: &Value) -> Arg {
        match v {
            Value::Null => Arg::Null,
            Value::Bool(b) => Arg::Bool(*b),
            Value::Number(n) => Arg::Num(n.as_f64().unwrap_or(f64::NAN)),
            Value::String(s) => Arg::Str(s.clone()),
            Value::Array(a) => Arg::List(a.iter().map(Arg::from_json).collect()),
            Value::Object(o) => Arg::Map(Obj(o.iter().map(|(k, v)| (k.clone(), Arg::from_json(v))).collect())),
        }
    }
}

/// A number as JSON: a whole number as an integer, as a script writes it.
pub fn number(n: f64) -> Value {
    if n.fract() == 0.0 && n.abs() < 9_007_199_254_740_992.0 {
        Value::from(n as i64)
    } else {
        serde_json::Number::from_f64(n).map(Value::Number).unwrap_or(Value::Null)
    }
}

/// A number as JavaScript prints it: `3`, `0.5`, `-2.25`.
pub fn show_number(n: f64) -> String {
    if n.is_nan() {
        "NaN".into()
    } else if n.is_infinite() {
        if n > 0.0 { "Infinity".into() } else { "-Infinity".into() }
    } else if n.fract() == 0.0 && n.abs() < 1e21 {
        format!("{}", n as i64)
    } else {
        format!("{n}")
    }
}

/// How an error names what it got: `an area`, `the string "x"`, `nothing`.
pub fn describe(v: Option<&Arg>) -> String {
    match v {
        None => "nothing".into(),
        Some(Arg::Null) => "null".into(),
        Some(Arg::Features(_)) => "features".into(),
        Some(Arg::Area { .. }) => "an area".into(),
        Some(Arg::Lines { .. }) => "lines".into(),
        Some(Arg::List(_)) => "a list".into(),
        Some(Arg::Map(_)) | Some(Arg::Lazy(_)) => "an object".into(),
        Some(Arg::Func) => "a function".into(),
        Some(Arg::Bool(b)) => format!("the boolean {b}"),
        Some(Arg::Num(n)) => format!("the number {}", show_number(*n)),
        Some(Arg::Str(s)) => format!("the string {}", Value::String(s.clone())),
    }
}

/// `Arg` from the values a Rust caller has at hand.
impl From<&str> for Arg {
    fn from(s: &str) -> Arg {
        Arg::Str(s.to_string())
    }
}
impl From<String> for Arg {
    fn from(s: String) -> Arg {
        Arg::Str(s)
    }
}
impl From<f64> for Arg {
    fn from(n: f64) -> Arg {
        Arg::Num(n)
    }
}
impl From<i32> for Arg {
    fn from(n: i32) -> Arg {
        Arg::Num(n as f64)
    }
}
impl From<u32> for Arg {
    fn from(n: u32) -> Arg {
        Arg::Num(n as f64)
    }
}
impl From<bool> for Arg {
    fn from(b: bool) -> Arg {
        Arg::Bool(b)
    }
}
impl<T: Into<Arg>> From<Vec<T>> for Arg {
    fn from(v: Vec<T>) -> Arg {
        Arg::List(v.into_iter().map(Into::into).collect())
    }
}
impl<T: Into<Arg> + Clone, const N: usize> From<[T; N]> for Arg {
    fn from(v: [T; N]) -> Arg {
        Arg::List(v.into_iter().map(Into::into).collect())
    }
}
impl From<Obj> for Arg {
    fn from(o: Obj) -> Arg {
        Arg::Map(o)
    }
}
impl From<&Arg> for Arg {
    fn from(a: &Arg) -> Arg {
        a.clone()
    }
}
impl From<Value> for Arg {
    fn from(v: Value) -> Arg {
        Arg::from_json(&v)
    }
}

/// An object written in Rust: `obj! { "pattern" => "dots", "spacing" => "0.45mm", "in" => &parks }`.
#[macro_export]
macro_rules! obj {
    () => { $crate::Obj::new() };
    ($($k:expr => $v:expr),+ $(,)?) => {{
        let mut o = $crate::Obj::new();
        $( o.set($k, $crate::Arg::from($v)); )+
        o
    }};
}

impl Arg {
    /// The value as JSON with its handles marked (`{ "$area": "$areas.x",
    /// "mask": … }`), for a binding to hand its own language and take back
    /// with [`Arg::decode`]. A lazy part lists the parts it has.
    pub fn encode(&self) -> Value {
        match self {
            Arg::Features(r) => serde_json::json!({ "$features": r }),
            Arg::Area { r, mask } => serde_json::json!({ "$area": r, "mask": mask }),
            Arg::Lines { r, module } => serde_json::json!({ "$lines": r, "module": module }),
            Arg::Func => serde_json::json!({ "$function": true }),
            Arg::Lazy(l) => {
                let tag = match l {
                    Lazy::Graticule(look) => serde_json::json!({ "graticule": look }),
                    Lazy::Labels { r, rules } => serde_json::json!({ "labels": { "r": r, "rules": rules } }),
                    Lazy::LabelRule { r, rule } => serde_json::json!({ "rule": { "r": r, "rule": rule } }),
                };
                serde_json::json!({ "$lazy": tag, "parts": crate::Graph::parts(l) })
            }
            Arg::List(l) => Value::Array(l.iter().map(Arg::encode).collect()),
            Arg::Map(o) => Value::Object(o.0.iter().map(|(k, v)| (k.clone(), v.encode())).collect()),
            other => other.to_json(),
        }
    }

    /// A value from [`Arg::encode`]'s JSON: marked handles become handles.
    pub fn decode(v: &Value) -> Arg {
        match v {
            Value::Array(a) => Arg::List(a.iter().map(Arg::decode).collect()),
            Value::Object(o) => {
                let s = |k: &str| o.get(k).and_then(Value::as_str).map(str::to_string);
                if let Some(r) = s("$features") {
                    return Arg::Features(r);
                }
                if let Some(r) = s("$area") {
                    return Arg::Area { mask: s("mask").unwrap_or_else(|| r.clone()), r };
                }
                if let Some(r) = s("$lines") {
                    return Arg::Lines { r, module: s("module") };
                }
                if o.contains_key("$function") {
                    return Arg::Func;
                }
                if let Some(tag) = o.get("$lazy") {
                    let str_of = |v: &Value, k: &str| v.get(k).and_then(Value::as_str).unwrap_or_default().to_string();
                    if let Some(look) = tag.get("graticule") {
                        return Arg::Lazy(Lazy::Graticule(look.clone()));
                    }
                    if let Some(l) = tag.get("labels") {
                        let rules = l.get("rules").and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_str).map(str::to_string).collect()).unwrap_or_default();
                        return Arg::Lazy(Lazy::Labels { r: str_of(l, "r"), rules });
                    }
                    if let Some(l) = tag.get("rule") {
                        return Arg::Lazy(Lazy::LabelRule { r: str_of(l, "r"), rule: str_of(l, "rule") });
                    }
                }
                Arg::Map(Obj(o.iter().map(|(k, v)| (k.clone(), Arg::decode(v))).collect()))
            }
            other => Arg::from_json(other),
        }
    }
}
