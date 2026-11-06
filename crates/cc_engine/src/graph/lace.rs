//! Lacing: a node given lists where it takes single values runs once per
//! item, and its outputs become lists of the same shape.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Deepest list nesting a node maps over.
pub const MAX_LEVELS: usize = 8;

/// How the items of several lists pair up.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Lacing {
    /// Item by item, as many times as the shortest list.
    #[default]
    Shortest,
    /// Item by item, as many times as the longest list (the shorter ones
    /// repeat their last item).
    Longest,
    /// Every combination: one more list level per list.
    Cross,
}

/// An input of one mapping: its value and how many list levels it runs
/// past what its port takes.
#[derive(Debug, Clone)]
pub struct Arg {
    pub value: Option<Value>,
    pub levels: usize,
}

/// The outputs of the calls, in the shape of the lists mapped over.
#[derive(Debug, Clone)]
enum Tree {
    Leaf(Vec<Value>),
    Branch(Vec<Tree>),
}

/// An input while mapping: a borrowed value and its remaining list levels.
#[derive(Clone, Copy)]
struct At<'v> {
    value: Option<&'v Value>,
    levels: usize,
}

type Call<'c> = dyn FnMut(Vec<Option<Value>>, &[u32]) -> Result<Vec<Value>, String> + 'c;

/// Run `call` once per combination of items and gather its outputs
/// (`outputs` of them per call) into lists. `call` gets the values and the
/// path of indices of the call; `budget` counts calls down.
pub fn map(
    args: Vec<Arg>,
    lacing: Lacing,
    outputs: usize,
    budget: &mut usize,
    call: &mut Call,
) -> Result<Vec<Value>, String> {
    if let Some(a) = args.iter().find(|a| a.levels > MAX_LEVELS) {
        return Err(format!("lists nested {} deep (at most {MAX_LEVELS})", a.levels));
    }
    let at: Vec<At> = args.iter().map(|a| At { value: a.value.as_ref(), levels: a.levels }).collect();
    let mut path = Vec::new();
    let tree = walk(&at, lacing, budget, &mut path, call)?;
    Ok((0..outputs).map(|i| project(&tree, i)).collect())
}

fn items<'v>(a: &At<'v>) -> &'v [Value] {
    match a.value {
        Some(Value::Array(v)) => v,
        _ => &[],
    }
}

fn with<'v>(args: &[At<'v>], picks: &[(usize, usize)]) -> Vec<At<'v>> {
    let mut next = args.to_vec();
    for &(i, k) in picks {
        next[i] = At { value: items(&args[i]).get(k), levels: args[i].levels - 1 };
    }
    next
}

fn walk(args: &[At], lacing: Lacing, budget: &mut usize, path: &mut Vec<u32>, call: &mut Call) -> Result<Tree, String> {
    let top = args.iter().map(|a| a.levels).max().unwrap_or(0);
    if top == 0 {
        if *budget == 0 {
            return Err("too many node calls in one run".into());
        }
        *budget -= 1;
        let values = args.iter().map(|a| a.value.cloned()).collect();
        return call(values, path).map(Tree::Leaf).map_err(|e| at(path, e));
    }
    // The inputs at the deepest level are mapped over; the others stay.
    let lists: Vec<usize> = (0..args.len()).filter(|&i| args[i].levels == top).collect();
    match lacing {
        Lacing::Shortest | Lacing::Longest => {
            let lens = lists.iter().map(|&i| items(&args[i]).len());
            let n = if lacing == Lacing::Shortest { lens.min() } else { lens.max() }.unwrap_or(0);
            let mut out = Vec::with_capacity(n.min(4096));
            for k in 0..n {
                let picks: Vec<(usize, usize)> =
                    lists.iter().map(|&i| (i, k.min(items(&args[i]).len().saturating_sub(1)))).collect();
                path.push(u32::try_from(k).unwrap_or(u32::MAX));
                let r = walk(&with(args, &picks), lacing, budget, path, call);
                path.pop();
                out.push(r?);
            }
            Ok(Tree::Branch(out))
        }
        Lacing::Cross => cross(args, &lists, 0, &mut Vec::new(), budget, path, call),
    }
}

fn cross(
    args: &[At],
    lists: &[usize],
    at_list: usize,
    picks: &mut Vec<(usize, usize)>,
    budget: &mut usize,
    path: &mut Vec<u32>,
    call: &mut Call,
) -> Result<Tree, String> {
    let Some(&i) = lists.get(at_list) else {
        return walk(&with(args, picks), Lacing::Cross, budget, path, call);
    };
    let n = items(&args[i]).len();
    let mut out = Vec::with_capacity(n.min(4096));
    for k in 0..n {
        picks.push((i, k));
        path.push(u32::try_from(k).unwrap_or(u32::MAX));
        let r = cross(args, lists, at_list + 1, picks, budget, path, call);
        path.pop();
        picks.pop();
        out.push(r?);
    }
    Ok(Tree::Branch(out))
}

fn project(tree: &Tree, i: usize) -> Value {
    match tree {
        Tree::Leaf(values) => values.get(i).cloned().unwrap_or(Value::Null),
        Tree::Branch(items) => Value::Array(items.iter().map(|t| project(t, i)).collect()),
    }
}

fn at(path: &[u32], e: String) -> String {
    if path.is_empty() || e.starts_with('[') {
        e
    } else {
        let p: Vec<String> = path.iter().map(u32::to_string).collect();
        format!("[{}] {e}", p.join(", "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn add(args: Vec<Arg>, lacing: Lacing) -> (Value, Vec<Vec<u32>>) {
        let mut paths = Vec::new();
        let mut budget = 1000;
        let out = map(args, lacing, 1, &mut budget, &mut |v, path| {
            paths.push(path.to_vec());
            let a = v[0].as_ref().and_then(Value::as_f64).unwrap_or(0.0);
            let b = v[1].as_ref().and_then(Value::as_f64).unwrap_or(0.0);
            Ok(vec![json!(a + b)])
        })
        .unwrap();
        (out[0].clone(), paths)
    }

    fn arg(v: Value, levels: usize) -> Arg {
        Arg { value: Some(v), levels }
    }

    #[test]
    fn single_values_call_once() {
        let (v, paths) = add(vec![arg(json!(1), 0), arg(json!(2), 0)], Lacing::Shortest);
        assert_eq!(v, json!(3.0));
        assert_eq!(paths, vec![Vec::<u32>::new()]);
    }

    #[test]
    fn shortest_longest_cross() {
        let a = || arg(json!([1, 2, 3]), 1);
        let b = || arg(json!([10, 20]), 1);
        assert_eq!(add(vec![a(), b()], Lacing::Shortest).0, json!([11.0, 22.0]));
        assert_eq!(add(vec![a(), b()], Lacing::Longest).0, json!([11.0, 22.0, 23.0]));
        let (v, paths) = add(vec![a(), b()], Lacing::Cross);
        assert_eq!(v, json!([[11.0, 21.0], [12.0, 22.0], [13.0, 23.0]]));
        assert_eq!(paths[1], vec![0, 1]);
        // A single value stays while a list maps.
        assert_eq!(add(vec![a(), arg(json!(1), 0)], Lacing::Shortest).0, json!([2.0, 3.0, 4.0]));
        // Nested lists map level by level.
        assert_eq!(
            add(vec![arg(json!([[1, 2], [3]]), 2), arg(json!(1), 0)], Lacing::Shortest).0,
            json!([[2.0, 3.0], [4.0]])
        );
        assert_eq!(add(vec![arg(json!([]), 1), arg(json!(1), 0)], Lacing::Shortest).0, json!([]));
    }

    #[test]
    fn budget_and_depth_are_bounded() {
        let mut budget = 2;
        let r = map(vec![arg(json!([1, 2, 3]), 1)], Lacing::Shortest, 1, &mut budget, &mut |_, _| Ok(vec![json!(0)]));
        assert!(r.is_err());
        let mut budget = 10;
        let r = map(vec![arg(json!(1), MAX_LEVELS + 1)], Lacing::Shortest, 1, &mut budget, &mut |_, _| Ok(vec![]));
        assert!(r.is_err());
    }
}
