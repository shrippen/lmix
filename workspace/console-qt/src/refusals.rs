//! Every refusal of lmix-core has its own text in the Console
//! (`Texts.message`): without one the Console shows the core's English.

/// The variants of `ActionError` as they come over the wire: the code
/// (snake_case) and the names of its fields.
fn variants() -> Vec<(String, Vec<String>)> {
    let source = include_str!("../../lmix-types/src/action.rs");
    let start = source
        .find("pub enum ActionError {")
        .expect("ActionError in action.rs");
    let body = &source[start..];
    let body = &body
        [body.find('{').expect("ActionError {") + 1..body.find("\n}").expect("end of ActionError")];
    // Without comments and attributes: each code comes from its name.
    let code_lines: Vec<&str> = body
        .lines()
        .map(str::trim)
        .filter(|line| !line.starts_with("//") && !line.starts_with("#["))
        .collect();
    assert!(
        !body.contains("rename"),
        "a variant of ActionError is renamed: read its code from the attribute"
    );
    // One variant up to its comma, fields over several lines too.
    let mut variants = Vec::new();
    let mut current = String::new();
    let mut depth = 0;
    for c in code_lines.join(" ").chars() {
        match c {
            '{' => depth += 1,
            '}' => depth -= 1,
            ',' if depth == 0 => {
                variants.push(std::mem::take(&mut current));
                continue;
            }
            _ => {}
        }
        current.push(c);
    }
    variants.push(current);
    variants
        .iter()
        .map(|variant| variant.trim())
        .filter_map(|variant| {
            let name: String = variant
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric())
                .collect();
            if !name.starts_with(|c: char| c.is_ascii_uppercase()) {
                return None;
            }
            let fields = match (variant.find('{'), variant.rfind('}')) {
                (Some(open), Some(close)) => variant[open + 1..close]
                    .split(',')
                    .filter_map(|field| field.split(':').next())
                    .map(|field| field.trim().to_string())
                    .filter(|field| !field.is_empty())
                    .collect(),
                _ => Vec::new(),
            };
            let mut code = String::new();
            for (i, c) in name.chars().enumerate() {
                if c.is_ascii_uppercase() && i > 0 {
                    code.push('_');
                }
                code.push(c.to_ascii_lowercase());
            }
            Some((code, fields))
        })
        .collect()
}

/// The fields a line of `Texts.message` reads from the error (`e.<name>`).
fn fields_read(line: &str) -> Vec<String> {
    let mut read = Vec::new();
    let mut rest = line;
    while let Some(at) = rest.find("e.") {
        let before = rest[..at].chars().next_back();
        let name: String = rest[at + 2..]
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
            .collect();
        if !before.is_some_and(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.')
            && !name.is_empty()
        {
            read.push(name);
        }
        rest = &rest[at + 2..];
    }
    read
}

#[test]
fn every_refusal_has_a_text() {
    let texts = include_str!("../qml/Texts.qml");
    let variants = variants();
    assert!(
        variants.len() > 40,
        "only {} codes read from ActionError",
        variants.len()
    );
    let mut missing = Vec::new();
    let mut wrong = Vec::new();
    for (code, fields) in &variants {
        let case = format!("case \"{code}\":");
        match texts.lines().find(|line| line.contains(&case)) {
            None => missing.push(code.clone()),
            Some(line) => {
                for name in fields_read(line) {
                    if !fields.contains(&name) {
                        wrong.push(format!("{code}: e.{name} (has {fields:?})"));
                    }
                }
            }
        }
    }
    assert!(
        missing.is_empty(),
        "no text in Texts.message for {missing:?}"
    );
    assert!(
        wrong.is_empty(),
        "Texts.message reads fields the error does not have: {wrong:?}"
    );
}
