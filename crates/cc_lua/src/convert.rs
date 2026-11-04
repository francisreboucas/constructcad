//! JSON values to Lua and back, for command arguments and results.
//!
//! Objects become tables with string keys, arrays become sequences marked
//! with the array metatable (so an empty array stays an array), `null`
//! becomes `ccad.null`. Back from Lua, a sequence `1..n` (or a table marked
//! as an array) is an array, any other table an object with string keys;
//! whole floats become integers (Lua's `/` always gives a float, and ids and
//! counts are integers). Nesting and size are bounded: tables can be cyclic
//! or huge.

use mlua::{Lua, LuaSerdeExt, Table, Value as LuaValue};
use serde_json::{Map, Number, Value};

/// Deepest nesting of tables or JSON values converted.
pub const MAX_DEPTH: usize = 64;
/// Most values converted at once.
pub const MAX_VALUES: usize = 1_000_000;

/// Floats up to this magnitude are exact integers.
const EXACT_INT: f64 = 9_007_199_254_740_992.0;

/// A JSON value as a Lua value.
pub fn to_lua(lua: &Lua, value: &Value) -> mlua::Result<LuaValue> {
    let mut count = 0;
    to_lua_at(lua, value, 0, &mut count)
}

fn to_lua_at(lua: &Lua, value: &Value, depth: usize, count: &mut usize) -> mlua::Result<LuaValue> {
    *count += 1;
    if depth > MAX_DEPTH || *count > MAX_VALUES {
        return Err(mlua::Error::runtime("value too deep or too large for Lua"));
    }
    Ok(match value {
        Value::Null => LuaValue::NULL,
        Value::Bool(b) => LuaValue::Boolean(*b),
        Value::Number(n) => match n.as_i64() {
            Some(i) => LuaValue::Integer(i),
            None => LuaValue::Number(n.as_f64().unwrap_or(0.0)),
        },
        Value::String(s) => LuaValue::String(lua.create_string(s)?),
        Value::Array(items) => {
            let t = lua.create_table_with_capacity(items.len(), 0)?;
            for (i, item) in items.iter().enumerate() {
                t.raw_set(i + 1, to_lua_at(lua, item, depth + 1, count)?)?;
            }
            t.set_metatable(Some(lua.array_metatable()))?;
            LuaValue::Table(t)
        }
        Value::Object(map) => {
            let t = lua.create_table_with_capacity(0, map.len())?;
            for (k, v) in map {
                t.raw_set(k.as_str(), to_lua_at(lua, v, depth + 1, count)?)?;
            }
            LuaValue::Table(t)
        }
    })
}

/// A Lua value as JSON; `Err` says what cannot be converted.
pub fn from_lua(lua: &Lua, value: &LuaValue) -> Result<Value, String> {
    let mut count = 0;
    from_lua_at(lua, value, 0, &mut count)
}

fn from_lua_at(lua: &Lua, value: &LuaValue, depth: usize, count: &mut usize) -> Result<Value, String> {
    *count += 1;
    if depth > MAX_DEPTH {
        return Err(format!("tables nested more than {MAX_DEPTH} deep (or a table that contains itself)"));
    }
    if *count > MAX_VALUES {
        return Err(format!("more than {MAX_VALUES} values"));
    }
    Ok(match value {
        LuaValue::Nil => Value::Null,
        LuaValue::LightUserData(u) if u.0.is_null() => Value::Null,
        LuaValue::Boolean(b) => Value::Bool(*b),
        LuaValue::Integer(i) => Value::from(*i),
        LuaValue::Number(n) => number(*n)?,
        LuaValue::String(s) => Value::String(s.to_str().map_err(|_| "a string that is not UTF-8")?.to_string()),
        LuaValue::Table(t) => table(lua, t, depth, count)?,
        other => return Err(format!("a {} cannot be passed as a value", other.type_name())),
    })
}

fn number(n: f64) -> Result<Value, String> {
    if !n.is_finite() {
        return Err(format!("{n} is not a finite number"));
    }
    if n.fract() == 0.0 && n.abs() <= EXACT_INT {
        // Exact: |n| is at most 2^53.
        #[allow(clippy::cast_possible_truncation)]
        return Ok(Value::from(n as i64));
    }
    Number::from_f64(n).map(Value::Number).ok_or_else(|| format!("{n} is not a finite number"))
}

fn table(lua: &Lua, t: &Table, depth: usize, count: &mut usize) -> Result<Value, String> {
    let marked = t.metatable().is_some_and(|m| m == lua.array_metatable());
    let len = t.raw_len();
    let mut keys = 0usize;
    let mut all_index = true;
    for pair in t.pairs::<LuaValue, LuaValue>() {
        let (k, _) = pair.map_err(|e| e.to_string())?;
        keys += 1;
        if keys > MAX_VALUES {
            return Err(format!("more than {MAX_VALUES} values"));
        }
        let index = match k {
            LuaValue::Integer(i) => usize::try_from(i).ok(),
            _ => None,
        };
        if !index.is_some_and(|i| i >= 1 && i <= len) {
            all_index = false;
        }
    }
    if (marked || keys > 0) && all_index && keys == len {
        let mut items = Vec::with_capacity(len.min(4096));
        for i in 1..=len {
            let v: LuaValue = t.raw_get(i).map_err(|e| e.to_string())?;
            items.push(from_lua_at(lua, &v, depth + 1, count)?);
        }
        return Ok(Value::Array(items));
    }
    let mut map = Map::new();
    for pair in t.pairs::<LuaValue, LuaValue>() {
        let (k, v) = pair.map_err(|e| e.to_string())?;
        let key = match &k {
            LuaValue::String(s) => s.to_str().map_err(|_| "a key that is not UTF-8")?.to_string(),
            LuaValue::Integer(_) | LuaValue::Number(_) => {
                return Err("a table with both list items and named fields (or a list with gaps)".into());
            }
            other => return Err(format!("a {} key (keys must be strings)", other.type_name())),
        };
        map.insert(key, from_lua_at(lua, &v, depth + 1, count)?);
    }
    Ok(Value::Object(map))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn round_trip(v: Value) -> Value {
        let lua = Lua::new();
        let l = to_lua(&lua, &v).unwrap();
        from_lua(&lua, &l).unwrap()
    }

    #[test]
    fn json_survives_a_round_trip() {
        let v =
            json!({"a": [1, 2.5, "x", null, true], "empty_list": [], "empty_object": {}, "n": {"m": [[0, 0], [5, 0]]}});
        assert_eq!(round_trip(v.clone()), v);
    }

    #[test]
    fn lua_tables_become_json() {
        let lua = Lua::new();
        let eval = |src: &str| from_lua(&lua, &lua.load(src).eval::<LuaValue>().unwrap());
        assert_eq!(eval("return {1, 2, 3}").unwrap(), json!([1, 2, 3]));
        assert_eq!(eval("return {x = 10 / 2, y = 0.5}").unwrap(), json!({"x": 5, "y": 0.5}));
        assert_eq!(eval("return {}").unwrap(), json!({}));
        assert!(eval("return {1, 2, x = 3}").is_err());
        assert!(eval("return {[1] = 1, [3] = 3}").is_err());
        assert!(eval("return {f = print}").is_err());
        assert!(eval("return 0/0").is_err());
        assert!(eval("return {[{}] = 1}").is_err());
    }

    #[test]
    fn cyclic_and_deep_tables_are_refused() {
        let lua = Lua::new();
        let cyclic: LuaValue = lua.load("local t = {}; t.t = t; return t").eval().unwrap();
        assert!(from_lua(&lua, &cyclic).is_err());
        let deep: LuaValue = lua.load("local t = {}; for i = 1, 100000 do t = {t} end; return t").eval().unwrap();
        assert!(from_lua(&lua, &deep).is_err());
        let mut v = json!(1);
        for _ in 0..1000 {
            v = json!([v]);
        }
        assert!(to_lua(&lua, &v).is_err());
    }
}
