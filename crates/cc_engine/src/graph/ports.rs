//! Ports of graph nodes: their types, read from command schemas, and how
//! deep a value runs past what a port takes (the lists a node maps over).

use serde::Serialize;
use serde_json::Value;

/// What a port carries. Values are JSON; the type says how they read.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PortType {
    /// Anything, passed through whole (never mapped over).
    Any,
    Number,
    Integer,
    Bool,
    Text,
    /// A point `[x, y]` (or `[x, y, z]`).
    Point,
    /// An element id.
    Element,
    /// A level, type, view or material: its id or its name.
    Ref,
    /// A JSON object (a command result, a curve).
    Object,
    List(Box<PortType>),
}

impl PortType {
    /// How many list levels the port takes as one value; `None` takes any
    /// value whole.
    pub fn rank(&self) -> Option<usize> {
        match self {
            PortType::Any => None,
            PortType::Point => Some(1),
            PortType::List(item) => item.rank().map(|r| r + 1),
            _ => Some(0),
        }
    }

    /// Whether an output of type `self` may be wired to an input of type
    /// `to`, counting lists the input maps over.
    pub fn fits(&self, to: &PortType) -> bool {
        fn base(t: &PortType) -> &PortType {
            match t {
                PortType::List(i) => base(i),
                other => other,
            }
        }
        let (a, b) = (base(self), base(to));
        if matches!(a, PortType::Any) || matches!(b, PortType::Any) {
            return true;
        }
        let scalar_id = |t: &PortType| matches!(t, PortType::Integer | PortType::Element | PortType::Number);
        match (a, b) {
            _ if a == b => true,
            // Ids are integers; integers are numbers.
            (x, y) if scalar_id(x) && scalar_id(y) => true,
            // A level, type, view or material by id or name.
            (x, PortType::Ref) if scalar_id(x) || matches!(x, PortType::Text) => true,
            // A point is a list of numbers.
            (PortType::Number, PortType::Point) => true,
            _ => false,
        }
    }

    /// A short name for the type (shown by the editor and `graph_nodes`).
    pub fn label(&self) -> String {
        match self {
            PortType::Any => "any".into(),
            PortType::Number => "number".into(),
            PortType::Integer => "integer".into(),
            PortType::Bool => "boolean".into(),
            PortType::Text => "text".into(),
            PortType::Point => "point".into(),
            PortType::Element => "element".into(),
            PortType::Ref => "reference".into(),
            PortType::Object => "object".into(),
            PortType::List(i) => format!("list of {}", i.label()),
        }
    }
}

/// An input or output of a node.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Port {
    pub name: String,
    #[serde(rename = "type")]
    pub ty: PortType,
    /// An input the node cannot run without.
    pub required: bool,
    /// The value of an input left unwired.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<Value>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub description: String,
}

impl Port {
    pub fn new(name: &str, ty: PortType) -> Self {
        Self { name: name.into(), ty, required: false, default: None, description: String::new() }
    }

    pub fn required(mut self) -> Self {
        self.required = true;
        self
    }

    pub fn default(mut self, v: Value) -> Self {
        self.default = Some(v);
        self
    }

    pub fn describe(mut self, d: &str) -> Self {
        self.description = d.into();
        self
    }
}

/// Names of integer parameters that hold element ids.
fn is_element_name(name: &str) -> bool {
    matches!(
        name,
        "id" | "ids"
            | "element"
            | "elements"
            | "wall"
            | "walls"
            | "host"
            | "target"
            | "targets"
            | "terrain"
            | "topography"
            | "board"
            | "panel"
            | "from"
            | "to"
    )
}

/// The type of a parameter from its JSON schema (draft 7, inlined).
pub fn type_of_schema(name: &str, schema: &Value) -> PortType {
    type_at(name, schema, 0)
}

fn type_at(name: &str, schema: &Value, depth: usize) -> PortType {
    if depth > 16 {
        return PortType::Any;
    }
    let Some(o) = schema.as_object() else { return PortType::Any };
    for key in ["anyOf", "oneOf"] {
        if let Some(branches) = o.get(key).and_then(Value::as_array) {
            let real: Vec<&Value> = branches.iter().filter(|b| !is_null_schema(b)).collect();
            if real.len() == 1 {
                return type_at(name, real[0], depth + 1);
            }
            let types: Vec<PortType> = real.iter().map(|b| type_at(name, b, depth + 1)).collect();
            if types.iter().all(|t| matches!(t, PortType::Object)) && !types.is_empty() {
                return PortType::Object;
            }
            // An id or a name.
            let id_or_name = types.len() == 2
                && types.iter().any(|t| matches!(t, PortType::Integer | PortType::Element))
                && types.iter().any(|t| matches!(t, PortType::Text));
            return if id_or_name { PortType::Ref } else { PortType::Any };
        }
    }
    let ty = match o.get("type") {
        Some(Value::String(s)) => Some(s.as_str()),
        Some(Value::Array(list)) => {
            let real: Vec<&str> = list.iter().filter_map(Value::as_str).filter(|s| *s != "null").collect();
            if real.len() == 1 { Some(real[0]) } else { None }
        }
        _ => None,
    };
    match ty {
        Some("number") => PortType::Number,
        Some("integer") if is_element_name(name) => PortType::Element,
        Some("integer") => PortType::Integer,
        Some("boolean") => PortType::Bool,
        Some("string") => PortType::Text,
        Some("object") => PortType::Object,
        Some("array") => {
            let items = o.get("items").unwrap_or(&Value::Null);
            let is_number = items.get("type").and_then(Value::as_str) == Some("number");
            let small = o.get("maxItems").and_then(Value::as_u64).is_some_and(|n| n == 2 || n == 3);
            if is_number && small {
                return PortType::Point;
            }
            let item = type_at(singular(name), items, depth + 1);
            if matches!(item, PortType::Any) { PortType::Any } else { PortType::List(Box::new(item)) }
        }
        _ => PortType::Any,
    }
}

fn singular(name: &str) -> &str {
    name.strip_suffix('s').unwrap_or(name)
}

fn is_null_schema(s: &Value) -> bool {
    s.get("type").and_then(Value::as_str) == Some("null")
}

/// How deep `value` runs in lists (an empty list counts one level).
pub fn depth(value: &Value) -> usize {
    let mut d = 0;
    let mut v = value;
    while let Value::Array(items) = v {
        d += 1;
        match items.first() {
            Some(first) if d < 64 => v = first,
            _ => break,
        }
    }
    d
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn ranks_and_depths() {
        assert_eq!(PortType::Point.rank(), Some(1));
        assert_eq!(PortType::List(Box::new(PortType::Point)).rank(), Some(2));
        assert_eq!(PortType::Any.rank(), None);
        assert_eq!(depth(&json!(1)), 0);
        assert_eq!(depth(&json!([[0, 0], [1, 0]])), 2);
        assert_eq!(depth(&json!([])), 1);
    }

    #[test]
    fn schema_types() {
        let point = json!({"type": "array", "items": {"type": "number"}, "minItems": 2, "maxItems": 2});
        assert_eq!(type_of_schema("start", &point), PortType::Point);
        let opt = json!({"type": ["number", "null"]});
        assert_eq!(type_of_schema("height", &opt), PortType::Number);
        let level = json!({"anyOf": [{"anyOf": [{"type": "integer"}, {"type": "string"}]}, {"type": "null"}]});
        assert_eq!(type_of_schema("level", &level), PortType::Ref);
        let wall = json!({"type": ["integer", "null"]});
        assert_eq!(type_of_schema("wall", &wall), PortType::Element);
        let outline = json!({"type": ["array", "null"], "items": point});
        assert_eq!(type_of_schema("boundary", &outline), PortType::List(Box::new(PortType::Point)));
        assert_eq!(type_of_schema("params", &json!({"default": null})), PortType::Any);
        assert!(PortType::Element.fits(&PortType::Ref));
        assert!(PortType::List(Box::new(PortType::Point)).fits(&PortType::Point));
        assert!(!PortType::Text.fits(&PortType::Point));
    }
}
