//! Expressions, as Mapbox GL / MapLibre styles write them: a property's
//! value that depends on the zoom or on the feature's tags.
//!
//! ```text
//! ["step", ["zoom"], "spleen-6x12", 10, "spleen-8x16", 14, "spleen-12x24"]
//! ["match", ["get", "class"], ["city", "town"], "spleen-8x16", "spleen-5x8"]
//! ["interpolate", ["linear"], ["zoom"], 12, 1, 18, 5]
//! ```
//!
//! The syntax is Mapbox's, so its documentation and a style's habits carry
//! over; the operators are a subset of it ([`OPERATORS`]), and one name it
//! does not have, `["scale"]` (the ground a unit spans, metres). An array
//! whose first element names an operator is an expression; any other value
//! is itself (`["literal", …]` makes an array that would read as one).
//!
//! The zoom is Mapbox's at the view's centre: the web Mercator world 512
//! units across at zoom 0, a unit being a pixel on a bitmap, a character on
//! text, a millimetre on paper ([`Frame`]).
//!
//! An operator is one arm in [`Expr::parse`] and one in [`Expr::eval`]:
//! adding one is adding both, and its name to [`OPERATORS`].

use crate::osm::Tags;
use serde_json::{json, Value};

/// What an expression is evaluated against: the frame's zoom and scale, and
/// the feature's tags when there is one.
#[derive(Clone, Copy, Debug)]
pub struct Context<'a> {
    pub frame: Frame,
    pub tags: Option<&'a Tags>,
}

/// The zoom and the scale a picture is drawn at.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Frame {
    /// Mapbox's zoom at the centre.
    pub zoom: f64,
    /// The ground a unit spans at the centre, metres.
    pub scale: f64,
}

/// The equator, metres (web Mercator's sphere).
const EQUATOR: f64 = 2.0 * std::f64::consts::PI * 6_378_137.0;

impl Frame {
    /// The frame of a picture whose unit spans `metres` of ground at its
    /// centre, at latitude `lat`: a web Mercator unit there spans
    /// `metres / cos(lat)`, which gives the zoom.
    pub fn new(metres: f64, lat: f64) -> Frame {
        let mercator = metres / lat.to_radians().cos().max(1e-6);
        Frame { zoom: (EQUATOR / (512.0 * mercator)).log2(), scale: metres }
    }

    /// The frame at a zoom (at the equator): for tests and examples.
    pub fn at_zoom(zoom: f64) -> Frame {
        Frame { zoom, scale: EQUATOR / (512.0 * 2f64.powf(zoom)) }
    }
}

/// The operators understood. Mapbox's others are refused by name.
pub const OPERATORS: &[&str] = &[
    "literal", "get", "has", "zoom", "scale", "coalesce", "match", "case", "step", "interpolate", "==", "!=", "<", "<=", ">",
    ">=", "all", "any", "!", "+", "-", "*", "/", "%", "min", "max", "to-number", "to-string", "to-boolean", "upcase",
    "downcase", "concat",
];

/// Mapbox operators not (yet) understood: named so that a style using one
/// is told so, not read as a plain array.
const ELSEWHERE: &[&str] = &[
    "array", "at", "boolean", "collator", "format", "geometry-type", "heatmap-density", "id", "image", "in", "index-of",
    "is-supported-script", "length", "let", "var", "line-progress", "number", "number-format", "object", "properties",
    "feature-state", "resolved-locale", "slice", "string", "to-color", "typeof", "within", "distance", "accumulated",
    "sqrt", "ln", "ln2", "log10", "log2", "pi", "e", "^", "abs", "acos", "asin", "atan", "ceil", "cos", "floor", "round",
    "sin", "tan", "rgb", "rgba", "to-rgba", "interpolate-hcl", "interpolate-lab", "pitch", "distance-from-center",
    "config", "global-state",
];

/// How a ramp is interpolated.
#[derive(Clone, Debug, PartialEq)]
pub enum Curve {
    Linear,
    Exponential(f64),
}

/// A parsed expression.
#[derive(Clone, Debug, PartialEq)]
pub enum Expr {
    Literal(Value),
    Get(String),
    Has(String),
    Zoom,
    Scale,
    Coalesce(Vec<Expr>),
    /// An input, cases (labels, output), a fallback.
    Match(Box<Expr>, Vec<(Vec<Value>, Expr)>, Box<Expr>),
    /// Conditions and outputs, a fallback.
    Case(Vec<(Expr, Expr)>, Box<Expr>),
    /// An input, the output below the first stop, then each stop's.
    Step(Box<Expr>, Box<Expr>, Vec<(f64, Expr)>),
    Interpolate(Curve, Box<Expr>, Vec<(f64, Expr)>),
    /// A comparison, logic, arithmetic or conversion over its arguments.
    Call(String, Vec<Expr>),
}

impl Expr {
    /// Whether `v` is an expression: an array led by an operator's name.
    pub fn is_expression(v: &Value) -> bool {
        matches!(v.as_array().and_then(|a| a.first()).and_then(Value::as_str), Some(op) if OPERATORS.contains(&op) || ELSEWHERE.contains(&op))
    }

    /// `v` read as an expression; a value that is not one is a literal.
    pub fn parse(v: &Value) -> Result<Expr, String> {
        let Some(items) = v.as_array().filter(|_| Expr::is_expression(v)) else {
            return Ok(Expr::Literal(v.clone()));
        };
        let op = items[0].as_str().unwrap_or_default();
        let args = &items[1..];
        let arity = |n: usize| -> Result<(), String> {
            if args.len() == n {
                Ok(())
            } else {
                Err(format!("[{op:?}, …] takes {n} argument{}, got {}", if n == 1 { "" } else { "s" }, args.len()))
            }
        };
        let key = |a: &Value| a.as_str().map(str::to_string).ok_or_else(|| format!("[{op:?}, …]: the tag's name is a string, got {a}"));
        let all = || args.iter().map(Expr::parse).collect::<Result<Vec<_>, _>>();
        let stops = |rest: &[Value], what: &str| -> Result<Vec<(f64, Expr)>, String> {
            if rest.len() % 2 != 0 {
                return Err(format!("[{what:?}, …]: stops come in pairs, an input and an output"));
            }
            let mut out: Vec<(f64, Expr)> = Vec::new();
            for pair in rest.chunks(2) {
                let stop = pair[0].as_f64().ok_or_else(|| format!("[{what:?}, …]: a stop's input is a number, got {}", pair[0]))?;
                if out.last().is_some_and(|(last, _)| stop <= *last) {
                    return Err(format!("[{what:?}, …]: the stops' inputs rise, {stop} after {}", out.last().unwrap().0));
                }
                out.push((stop, Expr::parse(&pair[1])?));
            }
            Ok(out)
        };
        Ok(match op {
            "literal" => {
                arity(1)?;
                Expr::Literal(args[0].clone())
            }
            "get" => {
                arity(1)?;
                Expr::Get(key(&args[0])?)
            }
            "has" => {
                arity(1)?;
                Expr::Has(key(&args[0])?)
            }
            "zoom" => {
                arity(0)?;
                Expr::Zoom
            }
            "scale" => {
                arity(0)?;
                Expr::Scale
            }
            "coalesce" => Expr::Coalesce(all()?),
            "match" => {
                if args.len() < 4 || args.len() % 2 != 0 {
                    return Err("[\"match\", input, label, output, …, fallback]".into());
                }
                let input = Expr::parse(&args[0])?;
                let mut cases = Vec::new();
                for pair in args[1..args.len() - 1].chunks(2) {
                    let labels = match &pair[0] {
                        Value::Array(ls) => ls.clone(),
                        l => vec![l.clone()],
                    };
                    if labels.is_empty() || labels.iter().any(|l| !(l.is_string() || l.is_number() || l.is_boolean())) {
                        return Err(format!("[\"match\", …]: a label is a string, number or boolean, or an array of them, got {}", pair[0]));
                    }
                    cases.push((labels, Expr::parse(&pair[1])?));
                }
                Expr::Match(Box::new(input), cases, Box::new(Expr::parse(&args[args.len() - 1])?))
            }
            "case" => {
                if args.len() < 3 || args.len() % 2 == 0 {
                    return Err("[\"case\", condition, output, …, fallback]".into());
                }
                let cases = args[..args.len() - 1].chunks(2).map(|p| Ok((Expr::parse(&p[0])?, Expr::parse(&p[1])?))).collect::<Result<Vec<_>, String>>()?;
                Expr::Case(cases, Box::new(Expr::parse(&args[args.len() - 1])?))
            }
            "step" => {
                if args.len() < 2 {
                    return Err("[\"step\", input, output, stop, output, …]".into());
                }
                Expr::Step(Box::new(Expr::parse(&args[0])?), Box::new(Expr::parse(&args[1])?), stops(&args[2..], "step")?)
            }
            "interpolate" => {
                if args.len() < 4 {
                    return Err("[\"interpolate\", [\"linear\"], input, stop, output, …]".into());
                }
                let curve = match args[0].as_array().map(Vec::as_slice) {
                    Some([t]) if t == "linear" => Curve::Linear,
                    Some([t, b]) if t == "exponential" && b.as_f64().is_some_and(|b| b > 0.0) => Curve::Exponential(b.as_f64().unwrap()),
                    _ => return Err(format!("[\"interpolate\", …]: the curve is [\"linear\"] or [\"exponential\", base], got {}", args[0])),
                };
                Expr::Interpolate(curve, Box::new(Expr::parse(&args[1])?), stops(&args[2..], "interpolate")?)
            }
            "==" | "!=" | "<" | "<=" | ">" | ">=" => {
                arity(2)?;
                Expr::Call(op.into(), all()?)
            }
            "!" | "to-number" | "to-string" | "to-boolean" | "upcase" | "downcase" => {
                arity(1)?;
                Expr::Call(op.into(), all()?)
            }
            "-" if args.len() == 1 || args.len() == 2 => Expr::Call(op.into(), all()?),
            "/" | "%" | "-" => {
                arity(2)?;
                Expr::Call(op.into(), all()?)
            }
            "all" | "any" | "+" | "*" | "min" | "max" | "concat" => Expr::Call(op.into(), all()?),
            other => {
                return Err(format!(
                    "the expression {other:?} is Mapbox's, not (yet) draw.land's: these are understood: {}",
                    OPERATORS.join(" ")
                ))
            }
        })
    }

    /// What the expression reads: the frame, a feature's tags.
    pub fn reads(&self) -> Reads {
        let mut r = Reads::default();
        self.walk(&mut |e| match e {
            Expr::Zoom | Expr::Scale => r.frame = true,
            Expr::Get(_) | Expr::Has(_) => r.feature = true,
            _ => {}
        });
        r
    }

    /// The values the expression can give that are written in it: the
    /// outputs of its branches, down to literals. `None` when one is
    /// computed (a tag, arithmetic), and so cannot be known before.
    pub fn outputs(&self) -> Option<Vec<Value>> {
        match self {
            Expr::Literal(v) => Some(vec![v.clone()]),
            Expr::Coalesce(es) => es.iter().map(Expr::outputs).collect::<Option<Vec<_>>>().map(|v| v.concat()),
            Expr::Match(_, cases, fallback) => {
                cases.iter().map(|(_, o)| o.outputs()).chain([fallback.outputs()]).collect::<Option<Vec<_>>>().map(|v| v.concat())
            }
            Expr::Case(cases, fallback) => {
                cases.iter().map(|(_, o)| o.outputs()).chain([fallback.outputs()]).collect::<Option<Vec<_>>>().map(|v| v.concat())
            }
            Expr::Step(_, first, stops) => {
                [first.outputs()].into_iter().chain(stops.iter().map(|(_, o)| o.outputs())).collect::<Option<Vec<_>>>().map(|v| v.concat())
            }
            Expr::Interpolate(_, _, stops) => stops.iter().map(|(_, o)| o.outputs()).collect::<Option<Vec<_>>>().map(|v| v.concat()),
            _ => None,
        }
    }

    fn walk(&self, f: &mut impl FnMut(&Expr)) {
        f(self);
        match self {
            Expr::Coalesce(es) | Expr::Call(_, es) => es.iter().for_each(|e| e.walk(f)),
            Expr::Match(i, cases, fb) => {
                i.walk(f);
                cases.iter().for_each(|(_, o)| o.walk(f));
                fb.walk(f);
            }
            Expr::Case(cases, fb) => {
                cases.iter().for_each(|(c, o)| {
                    c.walk(f);
                    o.walk(f)
                });
                fb.walk(f);
            }
            Expr::Step(i, first, stops) => {
                i.walk(f);
                first.walk(f);
                stops.iter().for_each(|(_, o)| o.walk(f));
            }
            Expr::Interpolate(_, i, stops) => {
                i.walk(f);
                stops.iter().for_each(|(_, o)| o.walk(f));
            }
            _ => {}
        }
    }

    /// The expression's value in `cx`. What Mapbox would call an error (a
    /// number compared with a string, a tag that is not there) is `null`,
    /// which a property takes as its default.
    pub fn eval(&self, cx: &Context) -> Value {
        match self {
            Expr::Literal(v) => v.clone(),
            Expr::Get(k) => cx.tags.and_then(|t| t.get(k)).map(|s| tag_value(s)).unwrap_or(Value::Null),
            Expr::Has(k) => Value::Bool(cx.tags.is_some_and(|t| t.contains_key(k))),
            Expr::Zoom => json!(cx.frame.zoom),
            Expr::Scale => json!(cx.frame.scale),
            Expr::Coalesce(es) => es.iter().map(|e| e.eval(cx)).find(|v| !v.is_null()).unwrap_or(Value::Null),
            Expr::Match(input, cases, fallback) => {
                let v = input.eval(cx);
                cases.iter().find(|(labels, _)| labels.iter().any(|l| same(l, &v))).map_or_else(|| fallback.eval(cx), |(_, o)| o.eval(cx))
            }
            Expr::Case(cases, fallback) => {
                cases.iter().find(|(c, _)| truthy(&c.eval(cx))).map_or_else(|| fallback.eval(cx), |(_, o)| o.eval(cx))
            }
            Expr::Step(input, first, stops) => {
                let Some(x) = input.eval(cx).as_f64() else { return first.eval(cx) };
                stops.iter().rev().find(|(s, _)| x >= *s).map_or_else(|| first.eval(cx), |(_, o)| o.eval(cx))
            }
            Expr::Interpolate(curve, input, stops) => {
                let Some(x) = input.eval(cx).as_f64() else { return Value::Null };
                let i = stops.iter().position(|(s, _)| *s > x).unwrap_or(stops.len());
                if i == 0 {
                    return stops[0].1.eval(cx);
                }
                if i == stops.len() {
                    return stops[i - 1].1.eval(cx);
                }
                let ((a, ea), (b, eb)) = (&stops[i - 1], &stops[i]);
                let (va, vb) = (ea.eval(cx), eb.eval(cx));
                let (Some(va), Some(vb)) = (va.as_f64(), vb.as_f64()) else { return va };
                let t = match curve {
                    Curve::Linear => (x - a) / (b - a),
                    Curve::Exponential(base) if (base - 1.0).abs() < 1e-12 => (x - a) / (b - a),
                    Curve::Exponential(base) => (base.powf(x - a) - 1.0) / (base.powf(b - a) - 1.0),
                };
                json!(va + (vb - va) * t)
            }
            Expr::Call(op, args) => call(op, args, cx),
        }
    }
}

/// What an expression reads.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Reads {
    pub frame: bool,
    pub feature: bool,
}

impl Reads {
    pub fn or(self, other: Reads) -> Reads {
        Reads { frame: self.frame || other.frame, feature: self.feature || other.feature }
    }
}

/// A tag's value: a number when it reads as one (vector tiles' `rank`,
/// `capital`), else its text.
fn tag_value(s: &str) -> Value {
    match s.parse::<f64>() {
        Ok(n) if n.is_finite() && !s.trim().is_empty() => json!(n),
        _ => Value::String(s.to_string()),
    }
}

fn same(a: &Value, b: &Value) -> bool {
    match (a.as_f64(), b.as_f64()) {
        (Some(x), Some(y)) => x == y,
        _ => a == b,
    }
}

fn truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().is_some_and(|n| n != 0.0),
        Value::String(s) => !s.is_empty(),
        _ => true,
    }
}

fn text(v: &Value) -> String {
    match v {
        Value::Null => String::new(),
        Value::String(s) => s.clone(),
        Value::Number(n) => match n.as_f64() {
            Some(f) if f.fract() == 0.0 && f.abs() < 1e15 => format!("{}", f as i64),
            _ => n.to_string(),
        },
        other => other.to_string(),
    }
}

fn call(op: &str, args: &[Expr], cx: &Context) -> Value {
    let vals: Vec<Value> = args.iter().map(|a| a.eval(cx)).collect();
    let nums = || vals.iter().map(Value::as_f64).collect::<Option<Vec<f64>>>();
    let num = |f: f64| if f.is_finite() { json!(f) } else { Value::Null };
    match op {
        "==" => Value::Bool(same(&vals[0], &vals[1])),
        "!=" => Value::Bool(!same(&vals[0], &vals[1])),
        "<" | "<=" | ">" | ">=" => {
            let ord = match (&vals[0], &vals[1]) {
                (a, b) if a.is_number() && b.is_number() => a.as_f64().unwrap().partial_cmp(&b.as_f64().unwrap()),
                (Value::String(a), Value::String(b)) => Some(a.cmp(b)),
                _ => None,
            };
            let Some(ord) = ord else { return Value::Bool(false) };
            Value::Bool(match op {
                "<" => ord.is_lt(),
                "<=" => ord.is_le(),
                ">" => ord.is_gt(),
                _ => ord.is_ge(),
            })
        }
        "all" => Value::Bool(args.iter().all(|a| truthy(&a.eval(cx)))),
        "any" => Value::Bool(args.iter().any(|a| truthy(&a.eval(cx)))),
        "!" => Value::Bool(!truthy(&vals[0])),
        "+" => nums().map_or(Value::Null, |n| num(n.iter().sum())),
        "*" => nums().map_or(Value::Null, |n| num(n.iter().product())),
        "-" => nums().map_or(Value::Null, |n| num(if n.len() == 1 { -n[0] } else { n[0] - n[1] })),
        "/" => nums().map_or(Value::Null, |n| num(n[0] / n[1])),
        "%" => nums().map_or(Value::Null, |n| num(n[0] % n[1])),
        "min" => nums().filter(|n| !n.is_empty()).map_or(Value::Null, |n| num(n.into_iter().fold(f64::INFINITY, f64::min))),
        "max" => nums().filter(|n| !n.is_empty()).map_or(Value::Null, |n| num(n.into_iter().fold(f64::NEG_INFINITY, f64::max))),
        "to-number" => match &vals[0] {
            Value::Number(_) => vals[0].clone(),
            Value::Bool(b) => json!(*b as u8),
            Value::String(s) => s.trim().parse::<f64>().ok().map_or(Value::Null, num),
            _ => Value::Null,
        },
        "to-string" => Value::String(text(&vals[0])),
        "to-boolean" => Value::Bool(truthy(&vals[0])),
        "upcase" => Value::String(text(&vals[0]).to_uppercase()),
        "downcase" => Value::String(text(&vals[0]).to_lowercase()),
        "concat" => Value::String(vals.iter().map(text).collect()),
        _ => Value::Null,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tags(pairs: &[(&str, &str)]) -> Tags {
        pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    fn eval(e: Value, zoom: f64, t: &Tags) -> Value {
        Expr::parse(&e).unwrap().eval(&Context { frame: Frame::at_zoom(zoom), tags: Some(t) })
    }

    #[test]
    fn mapbox_expressions_read_as_mapbox_reads_them() {
        let city = tags(&[("class", "city"), ("capital", "2"), ("name", "Roma")]);
        let font = json!(["step", ["zoom"], "small", 6, "medium", 10, "large"]);
        assert_eq!(eval(font.clone(), 3.0, &city), "small");
        assert_eq!(eval(font.clone(), 6.0, &city), "medium");
        assert_eq!(eval(font, 12.5, &city), "large");
        let by_class = json!(["match", ["get", "class"], ["city", "town"], 1, "suburb", 2, 0]);
        assert_eq!(eval(by_class.clone(), 0.0, &city), 1);
        assert_eq!(eval(by_class, 0.0, &tags(&[("class", "hamlet")])), 0);
        // A tag that reads as a number is one.
        assert_eq!(eval(json!(["==", ["get", "capital"], 2]), 0.0, &city), true);
        assert_eq!(eval(json!(["case", ["<=", ["get", "capital"], 2], "capital", "not"]), 0.0, &city), "capital");
        assert_eq!(eval(json!(["interpolate", ["linear"], ["zoom"], 10, 1, 20, 5]), 15.0, &city), 3.0);
        assert_eq!(eval(json!(["interpolate", ["linear"], ["zoom"], 10, 1, 20, 5]), 25.0, &city), 5);
        let e = eval(json!(["interpolate", ["exponential", 2], ["zoom"], 0, 0, 2, 3]), 1.0, &city).as_f64().unwrap();
        assert!((e - 1.0).abs() < 1e-9, "{e}");
        assert_eq!(eval(json!(["all", ["has", "name"], ["!", ["has", "ref"]]]), 0.0, &city), true);
        assert_eq!(eval(json!(["coalesce", ["get", "name:en"], ["get", "name"]]), 0.0, &city), "Roma");
        assert_eq!(eval(json!(["concat", ["upcase", ["get", "name"]], "!"]), 0.0, &city), "ROMA!");
        // A plain array is itself.
        assert_eq!(eval(json!(["~ ", " ~"]), 0.0, &city), json!(["~ ", " ~"]));
        assert_eq!(eval(json!(["literal", ["zoom"]]), 0.0, &city), json!(["zoom"]));
    }

    #[test]
    fn what_is_not_understood_is_said() {
        assert!(Expr::parse(&json!(["within", {}])).unwrap_err().contains("Mapbox's"));
        assert!(Expr::parse(&json!(["step", ["zoom"], 1, 5, 2, 3, 3])).unwrap_err().contains("rise"));
        assert!(Expr::parse(&json!(["get"])).is_err());
    }

    #[test]
    fn what_it_reads_and_can_give() {
        let e = Expr::parse(&json!(["match", ["get", "class"], "city", ["step", ["zoom"], "a", 5, "b"], "c"])).unwrap();
        assert_eq!(e.reads(), Reads { frame: true, feature: true });
        assert_eq!(e.outputs().unwrap(), vec![json!("a"), json!("b"), json!("c")]);
        assert_eq!(Expr::parse(&json!(["get", "font"])).unwrap().outputs(), None);
    }

    #[test]
    fn the_zoom_is_mapbox_zoom() {
        // Zoom 0: the world 512 units across. A metre a unit is near zoom 16.3 at the equator.
        assert!((Frame::new(EQUATOR / 512.0, 0.0).zoom).abs() < 1e-9);
        assert!((Frame::new(1.0, 0.0).zoom - 16.256).abs() < 0.01);
        // The same ground at 60° is a zoom less, as on a web map (whose
        // tiles at 60° span half the ground they do at the equator).
        assert!((Frame::new(1.0, 0.0).zoom - Frame::new(1.0, 60.0).zoom - 1.0).abs() < 1e-9);
        assert!((Frame::at_zoom(12.0).zoom - 12.0).abs() < 1e-12);
    }
}
