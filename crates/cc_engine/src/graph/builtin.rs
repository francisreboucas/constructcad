//! The built-in nodes: input values, math, lists, geometry (as the JSON
//! values commands take: points `[x, y]` and curves) and element
//! parameters.

use super::ports::{Port, PortType};
use super::{MAX_LIST, Node};
use crate::Session;
use serde_json::{Value, json};

/// The outputs of one call, or why it failed.
pub type Outputs = Result<Vec<Value>, String>;
/// A node that reads only its inputs.
pub type PureFn = fn(&[Option<Value>], &Node) -> Outputs;
/// A node that reads or changes the model.
pub type SessionFn = fn(&mut Session, &[Option<Value>], &Node) -> Outputs;

/// What running a built-in node needs.
#[derive(Clone, Copy)]
pub enum Eval {
    /// A function of its inputs (and the node's stored values).
    Pure(PureFn),
    /// Reads the model.
    Query(SessionFn),
    /// Changes the model (never creates elements).
    Edit(SessionFn),
}

/// A built-in node's definition.
#[derive(Clone)]
pub struct Builtin {
    pub name: &'static str,
    pub group: &'static str,
    pub description: &'static str,
    pub inputs: Vec<Port>,
    pub outputs: Vec<Port>,
    pub eval: Eval,
    /// An input node: its value is `values.value`, and when the node has a
    /// name it is an input of the graph (`graph_run`'s `inputs`).
    pub input: bool,
}

/// Groups of built-in nodes, in the order the node search shows them.
pub const GROUPS: [&str; 7] = ["Input", "Math", "Logic", "Lists", "Geometry", "Elements", "Output"];

fn p(name: &str, ty: PortType) -> Port {
    Port::new(name, ty)
}

fn list(t: PortType) -> PortType {
    PortType::List(Box::new(t))
}

fn b(
    name: &'static str,
    group: &'static str,
    description: &'static str,
    inputs: Vec<Port>,
    outputs: Vec<Port>,
    eval: Eval,
) -> Builtin {
    Builtin { name, group, description, inputs, outputs, eval, input: false }
}

fn input(name: &'static str, description: &'static str, ty: PortType, default: Value) -> Builtin {
    Builtin {
        name,
        group: "Input",
        description,
        inputs: Vec::new(),
        outputs: vec![p("value", ty).default(default)],
        eval: Eval::Pure(stored_value),
        input: true,
    }
}

/// Every built-in node.
pub fn catalog() -> Vec<Builtin> {
    use Eval::*;
    use PortType as T;
    let num = |n: &str| p(n, T::Number).required();
    let num_or = |n: &str, d: f64| p(n, T::Number).default(json!(d));
    let any = |n: &str| p(n, T::Any).required();
    let out = |n: &str, t: T| p(n, t);
    vec![
        // Input
        input("number", "A number.", T::Number, json!(0)),
        input("integer", "A whole number.", T::Integer, json!(0)),
        input("slider", "A number between `min` and `max` in steps of `step` (stored values).", T::Number, json!(0)),
        input("boolean", "True or false.", T::Bool, json!(false)),
        input("text", "A text (also a level, type, view or material by name).", T::Text, json!("")),
        b(
            "select_elements",
            "Input",
            "Elements picked in the model (kept by persistent id in `values.uids`).",
            vec![],
            vec![out("elements", list(T::Element))],
            Query(select_elements),
        ),
        b(
            "elements_of_category",
            "Input",
            "The ids of the elements of a category (and level).",
            vec![p("category", T::Text).required(), p("level", T::Ref)],
            vec![out("elements", list(T::Element))],
            Query(elements_of_category),
        ),
        b(
            "levels",
            "Input",
            "The levels, lowest first: ids and names.",
            vec![],
            vec![out("ids", list(T::Integer)), out("names", list(T::Text)), out("elevations", list(T::Number))],
            Query(levels),
        ),
        // Math
        b(
            "add",
            "Math",
            "a + b.",
            vec![num("a"), num("b")],
            vec![out("result", T::Number)],
            Pure(|a, _| bin(a, |x, y| x + y)),
        ),
        b(
            "subtract",
            "Math",
            "a - b.",
            vec![num("a"), num("b")],
            vec![out("result", T::Number)],
            Pure(|a, _| bin(a, |x, y| x - y)),
        ),
        b(
            "multiply",
            "Math",
            "a × b.",
            vec![num("a"), num("b")],
            vec![out("result", T::Number)],
            Pure(|a, _| bin(a, |x, y| x * y)),
        ),
        b("divide", "Math", "a ÷ b.", vec![num("a"), num("b")], vec![out("result", T::Number)], Pure(divide)),
        b(
            "modulo",
            "Math",
            "The remainder of a ÷ b.",
            vec![num("a"), num("b")],
            vec![out("result", T::Number)],
            Pure(modulo),
        ),
        b(
            "power",
            "Math",
            "a to the power b.",
            vec![num("a"), num("b")],
            vec![out("result", T::Number)],
            Pure(|a, _| bin(a, f64::powf)),
        ),
        b(
            "min",
            "Math",
            "The smaller of a and b.",
            vec![num("a"), num("b")],
            vec![out("result", T::Number)],
            Pure(|a, _| bin(a, f64::min)),
        ),
        b(
            "max",
            "Math",
            "The larger of a and b.",
            vec![num("a"), num("b")],
            vec![out("result", T::Number)],
            Pure(|a, _| bin(a, f64::max)),
        ),
        b("abs", "Math", "|x|.", vec![num("x")], vec![out("result", T::Number)], Pure(|a, _| un(a, f64::abs))),
        b(
            "round",
            "Math",
            "x rounded to `digits` decimals.",
            vec![num("x"), num_or("digits", 0.0)],
            vec![out("result", T::Number)],
            Pure(round),
        ),
        b(
            "floor",
            "Math",
            "The whole number at or below x.",
            vec![num("x")],
            vec![out("result", T::Number)],
            Pure(|a, _| un(a, f64::floor)),
        ),
        b(
            "ceiling",
            "Math",
            "The whole number at or above x.",
            vec![num("x")],
            vec![out("result", T::Number)],
            Pure(|a, _| un(a, f64::ceil)),
        ),
        b(
            "sqrt",
            "Math",
            "The square root of x.",
            vec![num("x")],
            vec![out("result", T::Number)],
            Pure(|a, _| un(a, f64::sqrt)),
        ),
        b(
            "sin",
            "Math",
            "Sine of an angle in degrees.",
            vec![num("degrees")],
            vec![out("result", T::Number)],
            Pure(|a, _| un(a, |d| d.to_radians().sin())),
        ),
        b(
            "cos",
            "Math",
            "Cosine of an angle in degrees.",
            vec![num("degrees")],
            vec![out("result", T::Number)],
            Pure(|a, _| un(a, |d| d.to_radians().cos())),
        ),
        b(
            "tan",
            "Math",
            "Tangent of an angle in degrees.",
            vec![num("degrees")],
            vec![out("result", T::Number)],
            Pure(|a, _| un(a, |d| d.to_radians().tan())),
        ),
        b(
            "pi",
            "Math",
            "π.",
            vec![],
            vec![out("result", T::Number)],
            Pure(|_, _| Ok(vec![json!(std::f64::consts::PI)])),
        ),
        b(
            "range",
            "Math",
            "Numbers from `start` to `end` (included when reached) in steps of `step`.",
            vec![num_or("start", 0.0), num("end"), num_or("step", 1.0)],
            vec![out("list", list(T::Number))],
            Pure(range),
        ),
        b(
            "sequence",
            "Math",
            "`count` numbers from `start` in steps of `step`.",
            vec![num_or("start", 0.0), p("count", T::Integer).required(), num_or("step", 1.0)],
            vec![out("list", list(T::Number))],
            Pure(sequence),
        ),
        // Logic
        b(
            "equal",
            "Logic",
            "Whether a equals b.",
            vec![any("a"), any("b")],
            vec![out("result", T::Bool)],
            Pure(|a, _| Ok(vec![json!(equal(&a[0], &a[1]))])),
        ),
        b(
            "less",
            "Logic",
            "Whether a < b.",
            vec![num("a"), num("b")],
            vec![out("result", T::Bool)],
            Pure(|a, _| cmp(a, |x, y| x < y)),
        ),
        b(
            "greater",
            "Logic",
            "Whether a > b.",
            vec![num("a"), num("b")],
            vec![out("result", T::Bool)],
            Pure(|a, _| cmp(a, |x, y| x > y)),
        ),
        b(
            "and",
            "Logic",
            "a and b.",
            vec![p("a", T::Bool).required(), p("b", T::Bool).required()],
            vec![out("result", T::Bool)],
            Pure(|a, _| logic(a, |x, y| x && y)),
        ),
        b(
            "or",
            "Logic",
            "a or b.",
            vec![p("a", T::Bool).required(), p("b", T::Bool).required()],
            vec![out("result", T::Bool)],
            Pure(|a, _| logic(a, |x, y| x || y)),
        ),
        b(
            "not",
            "Logic",
            "Not x.",
            vec![p("x", T::Bool).required()],
            vec![out("result", T::Bool)],
            Pure(|a, _| Ok(vec![json!(!boolean(&a[0])?)])),
        ),
        b(
            "if",
            "Logic",
            "`then` when the test is true, else `else`.",
            vec![p("test", T::Bool).required(), any("then"), any("else")],
            vec![out("result", T::Any)],
            Pure(|a, _| Ok(vec![if boolean(&a[0])? { val(&a[1]) } else { val(&a[2]) }])),
        ),
        // Lists
        b(
            "count",
            "Lists",
            "How many items the list has.",
            vec![any("list")],
            vec![out("count", T::Integer)],
            Pure(|a, _| Ok(vec![json!(items(&a[0])?.len())])),
        ),
        b(
            "get_item",
            "Lists",
            "The item at `index` (from 0; negative counts from the end).",
            vec![any("list"), p("index", T::Integer).required()],
            vec![out("item", T::Any)],
            Pure(get_item),
        ),
        b(
            "first",
            "Lists",
            "The first item.",
            vec![any("list")],
            vec![out("item", T::Any)],
            Pure(|a, _| Ok(vec![items(&a[0])?.first().cloned().unwrap_or(Value::Null)])),
        ),
        b(
            "last",
            "Lists",
            "The last item.",
            vec![any("list")],
            vec![out("item", T::Any)],
            Pure(|a, _| Ok(vec![items(&a[0])?.last().cloned().unwrap_or(Value::Null)])),
        ),
        b(
            "reverse",
            "Lists",
            "The items in reverse order.",
            vec![any("list")],
            vec![out("list", T::Any)],
            Pure(|a, _| Ok(vec![Value::Array(items(&a[0])?.iter().rev().cloned().collect())])),
        ),
        b(
            "flatten",
            "Lists",
            "The items of nested lists in one list.",
            vec![any("list")],
            vec![out("list", T::Any)],
            Pure(flatten),
        ),
        b(
            "sort",
            "Lists",
            "The items sorted (numbers, then texts).",
            vec![any("list")],
            vec![out("list", T::Any)],
            Pure(sort),
        ),
        b(
            "chop",
            "Lists",
            "The list cut into lists of `size` items.",
            vec![any("list"), p("size", T::Integer).required()],
            vec![out("lists", T::Any)],
            Pure(chop),
        ),
        b(
            "transpose",
            "Lists",
            "Rows become columns (lists of lists).",
            vec![any("lists")],
            vec![out("lists", T::Any)],
            Pure(transpose),
        ),
        b(
            "repeat",
            "Lists",
            "A list of `count` copies of the item.",
            vec![any("item"), p("count", T::Integer).required()],
            vec![out("list", T::Any)],
            Pure(repeat),
        ),
        b(
            "filter",
            "Lists",
            "The items whose mask is true, and the others.",
            vec![any("list"), p("mask", T::Any).required()],
            vec![out("in", T::Any), out("out", T::Any)],
            Pure(filter),
        ),
        b("sum", "Lists", "The sum of the numbers.", vec![any("list")], vec![out("sum", T::Number)], Pure(sum)),
        b(
            "create_list",
            "Lists",
            "A list of the inputs given (a to f).",
            ["a", "b", "c", "d", "e", "f"].iter().map(|n| p(n, T::Any)).collect(),
            vec![out("list", T::Any)],
            Pure(|a, _| Ok(vec![Value::Array(a.iter().flatten().cloned().collect())])),
        ),
        b(
            "join_lists",
            "Lists",
            "The items of a, then those of b.",
            vec![any("a"), any("b")],
            vec![out("list", T::Any)],
            Pure(|a, _| Ok(vec![Value::Array(items(&a[0])?.iter().chain(items(&a[1])?).cloned().collect())])),
        ),
        b(
            "get",
            "Lists",
            "A field of an object by its path (`params.height`, `ids.0`).",
            vec![p("object", T::Any).required(), p("path", T::Text).required()],
            vec![out("value", T::Any)],
            Pure(get_path),
        ),
        // Geometry
        b(
            "point",
            "Geometry",
            "The point `[x, y]`.",
            vec![num_or("x", 0.0), num_or("y", 0.0)],
            vec![out("point", T::Point)],
            Pure(|a, _| Ok(vec![json!([number(&a[0])?, number(&a[1])?])])),
        ),
        b(
            "point_xy",
            "Geometry",
            "The coordinates of a point.",
            vec![p("point", T::Point).required()],
            vec![out("x", T::Number), out("y", T::Number)],
            Pure(|a, _| {
                let q = point(&a[0])?;
                Ok(vec![json!(q[0]), json!(q[1])])
            }),
        ),
        b(
            "translate",
            "Geometry",
            "A point moved by `dx`, `dy`.",
            vec![p("point", T::Point).required(), num_or("dx", 0.0), num_or("dy", 0.0)],
            vec![out("point", T::Point)],
            Pure(|a, _| {
                let q = point(&a[0])?;
                Ok(vec![json!([q[0] + number(&a[1])?, q[1] + number(&a[2])?])])
            }),
        ),
        b(
            "rotate",
            "Geometry",
            "A point turned by `degrees` (counter-clockwise) about `center`.",
            vec![p("point", T::Point).required(), p("center", T::Point).default(json!([0, 0])), num_or("degrees", 0.0)],
            vec![out("point", T::Point)],
            Pure(rotate),
        ),
        b(
            "distance",
            "Geometry",
            "The distance between two points.",
            vec![p("a", T::Point).required(), p("b", T::Point).required()],
            vec![out("distance", T::Number)],
            Pure(|a, _| {
                let (u, v) = (point(&a[0])?, point(&a[1])?);
                Ok(vec![json!((v[0] - u[0]).hypot(v[1] - u[1]))])
            }),
        ),
        b(
            "divide_line",
            "Geometry",
            "`count` + 1 points evenly from `start` to `end`.",
            vec![p("start", T::Point).required(), p("end", T::Point).required(), p("count", T::Integer).required()],
            vec![out("points", list(T::Point))],
            Pure(divide_line),
        ),
        b(
            "polygon_points",
            "Geometry",
            "The corners of a regular polygon of `sides` around `center`, the first at `rotation` degrees.",
            vec![
                p("center", T::Point).default(json!([0, 0])),
                num("radius"),
                p("sides", T::Integer).required(),
                num_or("rotation", 0.0),
            ],
            vec![out("points", list(T::Point))],
            Pure(polygon_points),
        ),
        b(
            "rectangle_points",
            "Geometry",
            "The four corners of a rectangle from a corner to the opposite one, counter-clockwise.",
            vec![p("corner", T::Point).default(json!([0, 0])), p("opposite", T::Point).required()],
            vec![out("points", list(T::Point))],
            Pure(|a, _| {
                let (c, o) = (point(&a[0])?, point(&a[1])?);
                Ok(vec![json!([[c[0], c[1]], [o[0], c[1]], [o[0], o[1]], [c[0], o[1]]])])
            }),
        ),
        b(
            "polyline_segments",
            "Geometry",
            "The segments between consecutive points (`closed` adds the last one back to the first): starts and ends.",
            vec![p("points", list(T::Point)).required(), p("closed", T::Bool).default(json!(false))],
            vec![out("starts", list(T::Point)), out("ends", list(T::Point))],
            Pure(segments),
        ),
        b(
            "line",
            "Geometry",
            "A line curve (for `curves` inputs).",
            vec![p("start", T::Point).required(), p("end", T::Point).required()],
            vec![out("curve", T::Object)],
            Pure(|a, _| Ok(vec![json!({"kind": "line", "start": point(&a[0])?, "end": point(&a[1])?})])),
        ),
        b(
            "arc",
            "Geometry",
            "An arc curve from `start` to `end` with a `bulge` (1 is a half circle).",
            vec![p("start", T::Point).required(), p("end", T::Point).required(), num_or("bulge", 0.5)],
            vec![out("curve", T::Object)],
            Pure(|a, _| {
                Ok(vec![json!({"kind": "arc", "start": point(&a[0])?, "end": point(&a[1])?, "bulge": number(&a[2])?})])
            }),
        ),
        b(
            "circle",
            "Geometry",
            "A circle curve.",
            vec![p("center", T::Point).default(json!([0, 0])), num("radius")],
            vec![out("curve", T::Object)],
            Pure(|a, _| Ok(vec![json!({"kind": "circle", "center": point(&a[0])?, "radius": number(&a[1])?})])),
        ),
        b(
            "rectangle",
            "Geometry",
            "A rectangle curve from a corner to the opposite one.",
            vec![
                p("corner", T::Point).default(json!([0, 0])),
                p("opposite", T::Point).required(),
                num_or("rotation", 0.0),
            ],
            vec![out("curve", T::Object)],
            Pure(|a, _| {
                Ok(vec![
                    json!({"kind": "rectangle", "corner": point(&a[0])?, "opposite": point(&a[1])?, "rotation": number(&a[2])?}),
                ])
            }),
        ),
        // Elements
        b(
            "parameter_get",
            "Elements",
            "A value of an element by its path: `mark`, `comments`, `level`, `type`, `category`, or a field of its `params` (`height`, `shape.Gable.slope`).",
            vec![p("element", T::Element).required(), p("path", T::Text).required()],
            vec![out("value", T::Any)],
            Query(parameter_get),
        ),
        b(
            "parameter_set",
            "Elements",
            "Set a value of an element by its path (as `parameter_get` reads it); returns the element.",
            vec![p("element", T::Element).required(), p("path", T::Text).required(), any("value")],
            vec![out("element", T::Element)],
            Edit(parameter_set),
        ),
        // Output
        b(
            "watch",
            "Output",
            "Shows the value it gets and passes it on.",
            vec![p("value", T::Any)],
            vec![out("value", T::Any)],
            Pure(|a, _| Ok(vec![val(&a[0])])),
        ),
        b(
            "output",
            "Output",
            "A result of the graph, named by the node's name.",
            vec![p("value", T::Any)],
            vec![],
            Pure(|_, _| Ok(vec![])),
        ),
    ]
}

/// One built-in node by name.
pub fn find(name: &str) -> Option<Builtin> {
    catalog().into_iter().find(|b| b.name == name)
}

// ---- value helpers ----

fn val(v: &Option<Value>) -> Value {
    v.clone().unwrap_or(Value::Null)
}

pub(crate) fn number(v: &Option<Value>) -> Result<f64, String> {
    match v {
        Some(Value::Number(n)) => n.as_f64().filter(|x| x.is_finite()).ok_or_else(|| "not a finite number".into()),
        Some(Value::Bool(b)) => Ok(if *b { 1.0 } else { 0.0 }),
        Some(Value::String(s)) => {
            s.trim().parse::<f64>().ok().filter(|x| x.is_finite()).ok_or_else(|| format!("\"{s}\" is not a number"))
        }
        Some(other) => Err(format!("expected a number, got {}", kind_of(other))),
        None => Err("missing number".into()),
    }
}

fn integer(v: &Option<Value>) -> Result<i64, String> {
    let x = number(v)?;
    if x.abs() > 9.0e15 {
        return Err("number too large".into());
    }
    #[allow(clippy::cast_possible_truncation)]
    Ok(x.round() as i64)
}

fn count(v: &Option<Value>) -> Result<usize, String> {
    let n = integer(v)?;
    let n = usize::try_from(n).map_err(|_| "a count must not be negative".to_string())?;
    if n > MAX_LIST {
        return Err(format!("more than {MAX_LIST} items"));
    }
    Ok(n)
}

fn boolean(v: &Option<Value>) -> Result<bool, String> {
    match v {
        Some(Value::Bool(b)) => Ok(*b),
        Some(Value::Number(n)) => Ok(n.as_f64().is_some_and(|x| x != 0.0)),
        Some(Value::Null) | None => Ok(false),
        Some(other) => Err(format!("expected true or false, got {}", kind_of(other))),
    }
}

fn text(v: &Option<Value>) -> Result<String, String> {
    match v {
        Some(Value::String(s)) => Ok(s.clone()),
        Some(Value::Number(n)) => Ok(n.to_string()),
        Some(other) => Err(format!("expected a text, got {}", kind_of(other))),
        None => Err("missing text".into()),
    }
}

pub(crate) fn point(v: &Option<Value>) -> Result<[f64; 2], String> {
    let a = match v {
        Some(Value::Array(a)) if a.len() == 2 || a.len() == 3 => a,
        Some(other) => return Err(format!("expected a point [x, y], got {}", kind_of(other))),
        None => return Err("missing point".into()),
    };
    Ok([number(&a.first().cloned())?, number(&a.get(1).cloned())?])
}

fn items(v: &Option<Value>) -> Result<&Vec<Value>, String> {
    match v {
        Some(Value::Array(a)) => Ok(a),
        Some(other) => Err(format!("expected a list, got {}", kind_of(other))),
        None => Err("missing list".into()),
    }
}

fn kind_of(v: &Value) -> &'static str {
    match v {
        Value::Null => "nothing",
        Value::Bool(_) => "a boolean",
        Value::Number(_) => "a number",
        Value::String(_) => "a text",
        Value::Array(_) => "a list",
        Value::Object(_) => "an object",
    }
}

/// A finite number as JSON (an error for NaN or infinity).
fn finite(x: f64) -> Result<Value, String> {
    if x.is_finite() { Ok(json!(x)) } else { Err("the result is not a finite number".into()) }
}

fn checked_list(v: Vec<Value>) -> Result<Value, String> {
    if v.len() > MAX_LIST { Err(format!("more than {MAX_LIST} items")) } else { Ok(Value::Array(v)) }
}

// ---- node functions ----

fn stored_value(_: &[Option<Value>], node: &Node) -> Result<Vec<Value>, String> {
    let v = node.values.get("value").cloned().unwrap_or(json!(0));
    let v = match node.kind {
        super::NodeKind::Builtin(ref name) if name == "slider" => {
            let x = number(&Some(v))?;
            let min = node.values.get("min").and_then(Value::as_f64).unwrap_or(f64::MIN);
            let max = node.values.get("max").and_then(Value::as_f64).unwrap_or(f64::MAX);
            json!(if min <= max { x.clamp(min, max) } else { x })
        }
        _ => v,
    };
    Ok(vec![v])
}

fn bin(a: &[Option<Value>], f: fn(f64, f64) -> f64) -> Result<Vec<Value>, String> {
    Ok(vec![finite(f(number(&a[0])?, number(&a[1])?))?])
}

fn un(a: &[Option<Value>], f: fn(f64) -> f64) -> Result<Vec<Value>, String> {
    Ok(vec![finite(f(number(&a[0])?))?])
}

fn cmp(a: &[Option<Value>], f: fn(f64, f64) -> bool) -> Result<Vec<Value>, String> {
    Ok(vec![json!(f(number(&a[0])?, number(&a[1])?))])
}

fn logic(a: &[Option<Value>], f: fn(bool, bool) -> bool) -> Result<Vec<Value>, String> {
    Ok(vec![json!(f(boolean(&a[0])?, boolean(&a[1])?))])
}

fn equal(a: &Option<Value>, b: &Option<Value>) -> bool {
    match (a, b) {
        (Some(Value::Number(x)), Some(Value::Number(y))) => x.as_f64() == y.as_f64(),
        _ => a == b,
    }
}

fn divide(a: &[Option<Value>], _: &Node) -> Result<Vec<Value>, String> {
    let (x, y) = (number(&a[0])?, number(&a[1])?);
    if y == 0.0 {
        return Err("division by zero".into());
    }
    Ok(vec![finite(x / y)?])
}

fn modulo(a: &[Option<Value>], _: &Node) -> Result<Vec<Value>, String> {
    let (x, y) = (number(&a[0])?, number(&a[1])?);
    if y == 0.0 {
        return Err("division by zero".into());
    }
    Ok(vec![finite(x.rem_euclid(y))?])
}

fn round(a: &[Option<Value>], _: &Node) -> Result<Vec<Value>, String> {
    let x = number(&a[0])?;
    let digits = integer(&a[1])?.clamp(-15, 15);
    #[allow(clippy::cast_possible_truncation)]
    let f = 10f64.powi(digits as i32);
    Ok(vec![finite((x * f).round() / f)?])
}

fn range(a: &[Option<Value>], _: &Node) -> Result<Vec<Value>, String> {
    let (start, end, step) = (number(&a[0])?, number(&a[1])?, number(&a[2])?);
    if step == 0.0 || (end - start) * step < 0.0 {
        return if start == end {
            Ok(vec![json!([start])])
        } else {
            Err("the step does not lead from start to end".into())
        };
    }
    let n = ((end - start) / step + 1e-9).floor();
    if !(0.0..MAX_LIST as f64).contains(&n) {
        return Err(format!("more than {MAX_LIST} items"));
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let n = n as usize;
    Ok(vec![Value::Array((0..=n).map(|i| json!(start + step * i as f64)).collect())])
}

fn sequence(a: &[Option<Value>], _: &Node) -> Result<Vec<Value>, String> {
    let (start, n, step) = (number(&a[0])?, count(&a[1])?, number(&a[2])?);
    Ok(vec![Value::Array((0..n).map(|i| json!(start + step * i as f64)).collect())])
}

fn get_item(a: &[Option<Value>], _: &Node) -> Result<Vec<Value>, String> {
    let list = items(&a[0])?;
    let i = integer(&a[1])?;
    let len = i64::try_from(list.len()).unwrap_or(i64::MAX);
    let k = if i < 0 { len + i } else { i };
    usize::try_from(k)
        .ok()
        .and_then(|k| list.get(k))
        .cloned()
        .map(|v| vec![v])
        .ok_or_else(|| format!("index {i} outside a list of {len} items"))
}

fn flatten(a: &[Option<Value>], _: &Node) -> Result<Vec<Value>, String> {
    let mut out = Vec::new();
    let mut pending: Vec<&Value> = items(&a[0])?.iter().rev().collect();
    while let Some(v) = pending.pop() {
        match v {
            Value::Array(inner) => pending.extend(inner.iter().rev()),
            other => {
                out.push(other.clone());
                if out.len() > MAX_LIST {
                    return Err(format!("more than {MAX_LIST} items"));
                }
            }
        }
    }
    Ok(vec![Value::Array(out)])
}

fn sort(a: &[Option<Value>], _: &Node) -> Result<Vec<Value>, String> {
    let mut v = items(&a[0])?.clone();
    v.sort_by(|x, y| match (x, y) {
        (Value::Number(p), Value::Number(q)) => {
            p.as_f64().unwrap_or(0.0).partial_cmp(&q.as_f64().unwrap_or(0.0)).unwrap_or(std::cmp::Ordering::Equal)
        }
        (Value::Number(_), _) => std::cmp::Ordering::Less,
        (_, Value::Number(_)) => std::cmp::Ordering::Greater,
        (Value::String(p), Value::String(q)) => p.cmp(q),
        _ => std::cmp::Ordering::Equal,
    });
    Ok(vec![Value::Array(v)])
}

fn chop(a: &[Option<Value>], _: &Node) -> Result<Vec<Value>, String> {
    let list = items(&a[0])?;
    let size = count(&a[1])?;
    if size == 0 {
        return Err("the size must be at least 1".into());
    }
    Ok(vec![Value::Array(list.chunks(size).map(|c| Value::Array(c.to_vec())).collect())])
}

fn transpose(a: &[Option<Value>], _: &Node) -> Result<Vec<Value>, String> {
    let rows = items(&a[0])?;
    let width = rows.iter().map(|r| r.as_array().map_or(0, Vec::len)).max().unwrap_or(0);
    let cols = (0..width)
        .map(|c| Value::Array(rows.iter().filter_map(|r| r.as_array().and_then(|r| r.get(c)).cloned()).collect()))
        .collect();
    Ok(vec![Value::Array(cols)])
}

fn repeat(a: &[Option<Value>], _: &Node) -> Result<Vec<Value>, String> {
    let n = count(&a[1])?;
    Ok(vec![Value::Array(vec![val(&a[0]); n])])
}

fn filter(a: &[Option<Value>], _: &Node) -> Result<Vec<Value>, String> {
    let list = items(&a[0])?;
    let mask = items(&a[1])?;
    let (mut yes, mut no) = (Vec::new(), Vec::new());
    for (i, item) in list.iter().enumerate() {
        if boolean(&mask.get(i).cloned())? { yes.push(item.clone()) } else { no.push(item.clone()) }
    }
    Ok(vec![Value::Array(yes), Value::Array(no)])
}

fn sum(a: &[Option<Value>], _: &Node) -> Result<Vec<Value>, String> {
    let mut s = 0.0;
    for v in items(&a[0])? {
        s += number(&Some(v.clone()))?;
    }
    Ok(vec![finite(s)?])
}

/// The value at a dotted path (`params.height`, `ids.0`).
pub(crate) fn at_path<'a>(mut v: &'a Value, path: &str) -> Option<&'a Value> {
    for part in path.split('.').filter(|s| !s.is_empty()) {
        v = match v {
            Value::Object(m) => m.get(part)?,
            Value::Array(a) => a.get(part.parse::<usize>().ok()?)?,
            _ => return None,
        };
    }
    Some(v)
}

fn get_path(a: &[Option<Value>], _: &Node) -> Result<Vec<Value>, String> {
    let path = text(&a[1])?;
    let obj = val(&a[0]);
    at_path(&obj, &path).cloned().map(|v| vec![v]).ok_or_else(|| format!("no field \"{path}\""))
}

fn rotate(a: &[Option<Value>], _: &Node) -> Result<Vec<Value>, String> {
    let (q, c) = (point(&a[0])?, point(&a[1])?);
    let (s, co) = number(&a[2])?.to_radians().sin_cos();
    let (x, y) = (q[0] - c[0], q[1] - c[1]);
    Ok(vec![json!([c[0] + x * co - y * s, c[1] + x * s + y * co])])
}

fn divide_line(a: &[Option<Value>], _: &Node) -> Result<Vec<Value>, String> {
    let (s, e) = (point(&a[0])?, point(&a[1])?);
    let n = count(&a[2])?.max(1);
    let pts = (0..=n)
        .map(|i| {
            let t = i as f64 / n as f64;
            json!([s[0] + (e[0] - s[0]) * t, s[1] + (e[1] - s[1]) * t])
        })
        .collect();
    Ok(vec![checked_list(pts)?])
}

fn polygon_points(a: &[Option<Value>], _: &Node) -> Result<Vec<Value>, String> {
    let c = point(&a[0])?;
    let r = number(&a[1])?;
    let n = count(&a[2])?;
    if n < 3 {
        return Err("a polygon needs at least 3 sides".into());
    }
    let rot = number(&a[3])?.to_radians();
    let pts = (0..n)
        .map(|i| {
            let t = rot + std::f64::consts::TAU * i as f64 / n as f64;
            json!([c[0] + r * t.cos(), c[1] + r * t.sin()])
        })
        .collect();
    Ok(vec![checked_list(pts)?])
}

fn segments(a: &[Option<Value>], _: &Node) -> Result<Vec<Value>, String> {
    let pts = items(&a[0])?;
    let closed = boolean(&a[1])?;
    let n = pts.len();
    let count = if closed && n > 2 { n } else { n.saturating_sub(1) };
    let starts = (0..count).map(|i| pts[i].clone()).collect();
    let ends = (0..count).filter_map(|i| pts.get((i + 1) % n).cloned()).collect();
    Ok(vec![Value::Array(starts), Value::Array(ends)])
}

// ---- model nodes ----

fn element_id(v: &Option<Value>) -> Result<u64, String> {
    match v {
        Some(Value::Number(n)) => n.as_u64().ok_or_else(|| "not an element id".into()),
        Some(Value::Object(m)) => m.get("id").and_then(Value::as_u64).ok_or_else(|| "not an element".into()),
        Some(other) => Err(format!("expected an element, got {}", kind_of(other))),
        None => Err("missing element".into()),
    }
}

fn select_elements(s: &mut Session, _: &[Option<Value>], node: &Node) -> Result<Vec<Value>, String> {
    let uids: Vec<String> = node
        .values
        .get("uids")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(Value::as_str).map(str::to_string).collect())
        .unwrap_or_default();
    let p = s.project();
    let ids: Vec<Value> = uids
        .iter()
        .filter_map(|u| p.elements.values().find(|e| e.uid.to_string() == *u).map(|e| json!(e.id.0)))
        .collect();
    Ok(vec![Value::Array(ids)])
}

fn elements_of_category(s: &mut Session, a: &[Option<Value>], _: &Node) -> Result<Vec<Value>, String> {
    let mut args = json!({ "category": text(&a[0])? });
    if let Some(level) = &a[1] {
        args["level"] = level.clone();
    }
    let r = s.execute("elements_list", args).map_err(|e| e.message)?;
    let ids = r.as_array().map(|l| l.iter().filter_map(|e| e.get("id").cloned()).collect()).unwrap_or_default();
    Ok(vec![Value::Array(ids)])
}

fn levels(s: &mut Session, _: &[Option<Value>], _: &Node) -> Result<Vec<Value>, String> {
    let mut levels: Vec<&cc_core::Level> = s.project().levels.iter().collect();
    levels.sort_by(|a, b| a.elevation.partial_cmp(&b.elevation).unwrap_or(std::cmp::Ordering::Equal));
    Ok(vec![
        Value::Array(levels.iter().map(|l| json!(l.id.0)).collect()),
        Value::Array(levels.iter().map(|l| json!(l.name)).collect()),
        Value::Array(levels.iter().map(|l| json!(l.elevation)).collect()),
    ])
}

/// Top-level fields of `element_get` that `element_set` takes as such.
const TOP_FIELDS: [&str; 4] = ["mark", "comments", "level", "type"];

fn parameter_get(s: &mut Session, a: &[Option<Value>], _: &Node) -> Result<Vec<Value>, String> {
    let id = element_id(&a[0])?;
    let path = text(&a[1])?;
    let r = s.execute("element_get", json!({ "id": id })).map_err(|e| e.message)?;
    let first = path.split('.').next().unwrap_or_default();
    let in_params = !TOP_FIELDS.contains(&first) && r.get(first).is_none();
    let found = if in_params { r.get("params").and_then(|p| at_path(p, &path)) } else { at_path(&r, &path) };
    found.cloned().map(|v| vec![v]).ok_or_else(|| format!("no parameter \"{path}\""))
}

fn parameter_set(s: &mut Session, a: &[Option<Value>], _: &Node) -> Result<Vec<Value>, String> {
    let id = element_id(&a[0])?;
    let path = text(&a[1])?;
    let value = val(&a[2]);
    let parts: Vec<&str> = path.split('.').filter(|p| !p.is_empty()).collect();
    let Some((&first, rest)) = parts.split_first() else { return Err("empty parameter path".into()) };
    let args = if TOP_FIELDS.contains(&first) && rest.is_empty() {
        json!({ "id": id, first: value })
    } else {
        let mut patch = value;
        for part in parts.iter().rev() {
            patch = json!({ *part: patch });
        }
        json!({ "id": id, "params": patch })
    };
    s.execute("element_set", args).map_err(|e| e.message)?;
    Ok(vec![json!(id)])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(name: &str, args: Vec<Option<Value>>) -> Result<Vec<Value>, String> {
        let b = find(name).unwrap();
        let node = Node::new(1, super::super::NodeKind::Builtin(name.into()), [0.0, 0.0]);
        match b.eval {
            Eval::Pure(f) => f(&args, &node),
            _ => panic!("not pure"),
        }
    }

    #[test]
    fn names_are_unique_and_grouped() {
        let all = catalog();
        let mut names: Vec<&str> = all.iter().map(|b| b.name).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), all.len());
        assert!(all.iter().all(|b| GROUPS.contains(&b.group)));
    }

    #[test]
    fn math_and_lists() {
        assert_eq!(run("divide", vec![Some(json!(1)), Some(json!(0))]).map_err(|_| ()), Err(()));
        assert_eq!(
            run("range", vec![Some(json!(0)), Some(json!(1)), Some(json!(0.5))]).unwrap()[0],
            json!([0.0, 0.5, 1.0])
        );
        assert!(run("range", vec![Some(json!(0)), Some(json!(1e12)), Some(json!(1))]).is_err());
        assert_eq!(run("get_item", vec![Some(json!([1, 2, 3])), Some(json!(-1))]).unwrap()[0], json!(3));
        assert_eq!(run("flatten", vec![Some(json!([1, [2, [3]]]))]).unwrap()[0], json!([1, 2, 3]));
        assert_eq!(run("chop", vec![Some(json!([1, 2, 3])), Some(json!(2))]).unwrap()[0], json!([[1, 2], [3]]));
        assert_eq!(
            run("polyline_segments", vec![Some(json!([[0, 0], [1, 0], [1, 1]])), Some(json!(true))]).unwrap()[1],
            json!([[1, 0], [1, 1], [0, 0]])
        );
        assert_eq!(run("get", vec![Some(json!({"a": {"b": [5, 6]}})), Some(json!("a.b.1"))]).unwrap()[0], json!(6));
    }
}
