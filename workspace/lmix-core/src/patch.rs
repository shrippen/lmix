//! State diffs for event clients that ask for them (API.6): the changes
//! between two states as JSON Patch operations (RFC 6902: `add`,
//! `remove`, `replace`). Objects are compared key by key, arrays item by
//! item (items added or removed at the end); anything else that differs
//! is replaced whole.

use serde_json::{json, Map, Value};

/// Operations that turn `old` into `new`.
pub fn diff(old: &Value, new: &Value) -> Vec<Value> {
    let mut ops = Vec::new();
    walk(old, new, &mut String::new(), &mut ops);
    ops
}

fn walk(old: &Value, new: &Value, path: &mut String, ops: &mut Vec<Value>) {
    if old == new {
        return;
    }
    match (old, new) {
        (Value::Object(a), Value::Object(b)) => objects(a, b, path, ops),
        (Value::Array(a), Value::Array(b)) => {
            let common = a.len().min(b.len());
            for i in 0..common {
                within(path, &i.to_string(), |path| walk(&a[i], &b[i], path, ops));
            }
            for (i, item) in b.iter().enumerate().skip(common) {
                ops.push(json!({ "op": "add", "path": format!("{path}/{i}"), "value": item }));
            }
            // From the end, so the indices stay right.
            for i in (common..a.len()).rev() {
                ops.push(json!({ "op": "remove", "path": format!("{path}/{i}") }));
            }
        }
        _ => ops.push(json!({ "op": "replace", "path": path.clone(), "value": new })),
    }
}

fn objects(
    a: &Map<String, Value>,
    b: &Map<String, Value>,
    path: &mut String,
    ops: &mut Vec<Value>,
) {
    for (key, old) in a {
        match b.get(key) {
            Some(new) => within(path, key, |path| walk(old, new, path, ops)),
            None => within(path, key, |path| {
                ops.push(json!({ "op": "remove", "path": path.clone() }));
            }),
        }
    }
    for (key, new) in b {
        if !a.contains_key(key) {
            within(path, key, |path| {
                ops.push(json!({ "op": "add", "path": path.clone(), "value": new }));
            });
        }
    }
}

/// Runs `f` with `token` appended to the JSON Pointer `path`.
fn within(path: &mut String, token: &str, f: impl FnOnce(&mut String)) {
    let len = path.len();
    path.push('/');
    path.push_str(&token.replace('~', "~0").replace('/', "~1"));
    f(path);
    path.truncate(len);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Applies `ops` as a client would.
    fn apply(mut doc: Value, ops: &[Value]) -> Value {
        for op in ops {
            let path = op["path"].as_str().unwrap();
            let tokens: Vec<String> = path
                .split('/')
                .skip(1)
                .map(|t| t.replace("~1", "/").replace("~0", "~"))
                .collect();
            if tokens.is_empty() {
                doc = op["value"].clone();
                continue;
            }
            let (last, parents) = tokens.split_last().unwrap();
            let mut at = &mut doc;
            for t in parents {
                at = match at {
                    Value::Array(a) => &mut a[t.parse::<usize>().unwrap()],
                    other => &mut other[t.as_str()],
                };
            }
            match (op["op"].as_str().unwrap(), at) {
                ("remove", Value::Array(a)) => {
                    a.remove(last.parse().unwrap());
                }
                ("remove", Value::Object(o)) => {
                    o.remove(last);
                }
                ("add", Value::Array(a)) => a.insert(last.parse().unwrap(), op["value"].clone()),
                (_, Value::Array(a)) => a[last.parse::<usize>().unwrap()] = op["value"].clone(),
                (_, Value::Object(o)) => {
                    o.insert(last.clone(), op["value"].clone());
                }
                _ => panic!("bad path {path}"),
            }
        }
        doc
    }

    #[test]
    fn diffs_rebuild_the_new_state() {
        let old = json!({
            "t_bar": 0.0,
            "inputs": [{ "name": "A", "status": "online" }, { "name": "B" }, { "name": "C" }],
            "gone": 1,
            "a/b~c": [1, 2],
            "preview": null
        });
        let new = json!({
            "t_bar": 0.5,
            "inputs": [{ "name": "A", "status": "offline" }, { "name": "B2" }],
            "a/b~c": [1, 2, 3],
            "preview": "x",
            "added": { "deep": true }
        });

        let ops = diff(&old, &new);
        assert_eq!(apply(old.clone(), &ops), new);
        // Only what changed: the status, not the whole input list.
        assert!(ops
            .contains(&json!({ "op": "replace", "path": "/inputs/0/status", "value": "offline" })));
        assert!(ops.contains(&json!({ "op": "add", "path": "/a~1b~0c/2", "value": 3 })));
        assert!(diff(&new, &new).is_empty());
        assert_eq!(apply(json!(1), &diff(&json!(1), &json!([2]))), json!([2]));
    }
}
