//! What changed between the show state QML has and a newer one, as a
//! list of operations. QML applies them by copying only the containers on
//! the changed paths (StatePatch.js): the rest keeps its objects, and a
//! binding that reads an unchanged part gets the same object back, so
//! nothing below it is evaluated again. The whole state, parsed anew for
//! every fader move, made the GUI thread evaluate every binding of the
//! show 28 times a second (29.10.2026).
//!
//! The operations, applied in order (`p`: keys and indices from the root):
//! - `{"p": [...], "v": value}`: set (an empty `p` replaces the root),
//! - `{"p": [...], "d": true}`: remove the key,
//! - `{"p": [...], "a": [{"k": old index} | {"v": value}, ...]}`: the array
//!   anew from kept old elements and new ones; operations after it reach
//!   into it by the new indices. For arrays that grew, shrank or moved:
//!   their elements are matched by `id`, or by being equal.

use serde_json::{json, Value};

pub fn diff(old: &Value, new: &Value) -> Vec<Value> {
    let mut ops = Vec::new();
    walk(&mut Vec::new(), old, new, &mut ops);
    ops
}

fn walk(path: &mut Vec<Value>, old: &Value, new: &Value, ops: &mut Vec<Value>) {
    match (old, new) {
        (Value::Object(a), Value::Object(b)) => {
            for (key, value) in b {
                path.push(Value::String(key.clone()));
                match a.get(key) {
                    Some(before) => walk(path, before, value, ops),
                    None => ops.push(json!({ "p": path, "v": value })),
                }
                path.pop();
            }
            for key in a.keys().filter(|k| !b.contains_key(*k)) {
                path.push(Value::String(key.clone()));
                ops.push(json!({ "p": path, "d": true }));
                path.pop();
            }
        }
        (Value::Array(a), Value::Array(b))
            if a.len() == b.len() && a.iter().zip(b).all(|(x, y)| id(x) == id(y)) =>
        {
            for (i, (before, value)) in a.iter().zip(b).enumerate() {
                path.push(Value::from(i));
                walk(path, before, value, ops);
                path.pop();
            }
        }
        (Value::Array(a), Value::Array(b)) => rebuild(path, a, b, ops),
        _ if old == new => {}
        _ => ops.push(json!({ "p": path, "v": new })),
    }
}

/// The `id` of an element that has one.
fn id(value: &Value) -> Option<&Value> {
    value.as_object().and_then(|o| o.get("id"))
}

fn rebuild(path: &mut Vec<Value>, a: &[Value], b: &[Value], ops: &mut Vec<Value>) {
    // The root is an object; an array there is set as a whole.
    if path.is_empty() {
        ops.push(json!({ "p": path, "v": b }));
        return;
    }
    let mut used = vec![false; a.len()];
    let mut kept = Vec::new();
    let entries: Vec<Value> = b
        .iter()
        .enumerate()
        .map(|(j, value)| {
            // Plain values are as short as their index.
            if !value.is_object() && !value.is_array() {
                return json!({ "v": value });
            }
            let found = match id(value) {
                Some(wanted) => (0..a.len()).find(|&i| !used[i] && id(&a[i]) == Some(wanted)),
                None => (0..a.len()).find(|&i| !used[i] && id(&a[i]).is_none() && a[i] == *value),
            };
            match found {
                Some(i) => {
                    used[i] = true;
                    kept.push((i, j));
                    json!({ "k": i })
                }
                None => json!({ "v": value }),
            }
        })
        .collect();
    ops.push(json!({ "p": path, "a": entries }));
    for (i, j) in kept {
        path.push(Value::from(j));
        walk(path, &a[i], &b[j], ops);
        path.pop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Map;

    /// The operations as StatePatch.js applies them.
    fn apply(mut root: Value, ops: &[Value]) -> Value {
        for op in ops {
            let path = op["p"].as_array().unwrap();
            let Some((last, parents)) = path.split_last() else {
                root = op["v"].clone();
                continue;
            };
            let mut at = &mut root;
            for step in parents {
                at = match step {
                    Value::String(key) => at.get_mut(key).unwrap(),
                    _ => at.get_mut(step.as_u64().unwrap() as usize).unwrap(),
                };
            }
            let slot = match last {
                Value::String(key) if op.get("d").is_some() => {
                    at.as_object_mut().unwrap().remove(key);
                    continue;
                }
                Value::String(key) => at
                    .as_object_mut()
                    .unwrap()
                    .entry(key.clone())
                    .or_insert(Value::Null),
                _ => at.get_mut(last.as_u64().unwrap() as usize).unwrap(),
            };
            if let Some(entries) = op.get("a") {
                let old = slot.as_array().unwrap().clone();
                *slot = Value::Array(
                    entries
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|e| match e.get("k") {
                            Some(k) => old[k.as_u64().unwrap() as usize].clone(),
                            None => e["v"].clone(),
                        })
                        .collect(),
                );
            } else {
                *slot = op["v"].clone();
            }
        }
        root
    }

    fn check(old: Value, new: Value) -> Vec<Value> {
        let ops = diff(&old, &new);
        assert_eq!(apply(old, &ops), new, "ops: {ops:?}");
        ops
    }

    fn inputs(volumes: &[(&str, u32)]) -> Value {
        json!({ "program": "a", "inputs": volumes.iter().map(|(id, v)| json!({ "id": id, "audio": { "volume": v } })).collect::<Vec<_>>() })
    }

    #[test]
    fn a_volume_change_is_one_set_at_its_path() {
        let ops = check(
            inputs(&[("a", 100), ("b", 100)]),
            inputs(&[("a", 98), ("b", 100)]),
        );
        assert_eq!(
            ops,
            vec![json!({ "p": ["inputs", 0, "audio", "volume"], "v": 98 })]
        );
    }

    #[test]
    fn equal_states_need_nothing() {
        assert!(diff(&inputs(&[("a", 1)]), &inputs(&[("a", 1)])).is_empty());
    }

    #[test]
    fn the_first_state_replaces_the_root() {
        let ops = check(Value::Null, inputs(&[("a", 1)]));
        assert_eq!(ops.len(), 1);
        assert_eq!(ops[0]["p"], json!([]));
    }

    #[test]
    fn an_added_input_keeps_the_others() {
        let ops = check(
            inputs(&[("a", 1), ("b", 2)]),
            inputs(&[("a", 1), ("c", 5), ("b", 3)]),
        );
        assert_eq!(
            ops[0]["a"],
            json!([{ "k": 0 }, { "v": { "id": "c", "audio": { "volume": 5 } } }, { "k": 1 }])
        );
        assert_eq!(
            ops[1],
            json!({ "p": ["inputs", 2, "audio", "volume"], "v": 3 })
        );
    }

    #[test]
    fn moved_and_removed_elements() {
        check(
            inputs(&[("a", 1), ("b", 2), ("c", 3)]),
            inputs(&[("c", 3), ("a", 1)]),
        );
        check(
            json!([null, { "x": 1 }, [1, 2]]),
            json!([[1, 2], null, { "x": 1 }, { "x": 1 }]),
        );
    }

    #[test]
    fn keys_come_and_go() {
        check(
            json!({ "a": 1, "b": { "c": 2 } }),
            json!({ "b": { "d": [1] }, "e": null }),
        );
        check(json!({ "a": [1, 2] }), json!({ "a": { "0": 1 } }));
        check(json!({ "a": 1 }), Value::Null);
    }

    /// QML sets each key of the state as a property of ShowState.qml.
    #[test]
    fn state_keys_are_declared() {
        let schema = serde_json::to_value(schemars::schema_for!(lmix_types::ShowState)).unwrap();
        let qml = include_str!("../qml/ShowState.qml");
        let declared: Vec<&str> = qml
            .lines()
            .filter_map(|l| l.trim().strip_prefix("property var "))
            .collect();
        let keys: Vec<&String> = schema["properties"].as_object().unwrap().keys().collect();
        assert!(keys.len() > 40);
        for key in keys {
            assert!(
                declared.contains(&key.as_str()),
                "{key} is missing in qml/ShowState.qml"
            );
        }
    }

    /// Random states from a few shapes, each pair round trips.
    #[test]
    fn random_pairs_round_trip() {
        let mut seed = 0x2545_f491_4f6c_dd1du64;
        let mut next = move |n: u64| {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed % n
        };
        fn value(next: &mut impl FnMut(u64) -> u64, depth: u32) -> Value {
            match next(if depth == 0 { 3 } else { 6 }) {
                0 => Value::Null,
                1 => Value::from(next(4)),
                2 => Value::from(["x", "y"][next(2) as usize]),
                3 => Value::Array((0..next(4)).map(|_| value(next, depth - 1)).collect()),
                4 => {
                    let mut o: Map<String, Value> = (0..next(3))
                        .map(|_| {
                            (
                                ["a", "b", "c"][next(3) as usize].to_string(),
                                value(next, depth - 1),
                            )
                        })
                        .collect();
                    if next(2) == 0 {
                        o.insert("id".into(), Value::from(next(3)));
                    }
                    Value::Object(o)
                }
                _ => {
                    json!({ "inputs": (0..next(4)).map(|_| value(next, depth - 1)).collect::<Vec<_>>() })
                }
            }
        }
        for _ in 0..5000 {
            let old = value(&mut next, 4);
            let new = value(&mut next, 4);
            check(old, new);
        }
    }
}
