//! Every refusal of lmix-core has its own text in the Console
//! (`Texts.message`): without one the Console shows the core's English.

/// The codes of `ActionError` as they come over the wire (snake_case).
fn codes() -> Vec<String> {
    let source = include_str!("../../lmix-types/src/action.rs");
    let start = source.find("pub enum ActionError {").expect("ActionError in action.rs");
    let body = &source[start..];
    let body = &body[..body.find("\n}").expect("end of ActionError")];
    body.lines()
        .filter_map(|line| line.strip_prefix("    "))
        .filter_map(|line| {
            let name: String = line.chars().take_while(|c| c.is_ascii_alphanumeric()).collect();
            name.starts_with(|c: char| c.is_ascii_uppercase()).then_some(name)
        })
        .map(|name| {
            let mut code = String::new();
            for (i, c) in name.chars().enumerate() {
                if c.is_ascii_uppercase() && i > 0 {
                    code.push('_');
                }
                code.push(c.to_ascii_lowercase());
            }
            code
        })
        .collect()
}

#[test]
fn every_refusal_has_a_text() {
    let texts = include_str!("../qml/Texts.qml");
    let codes = codes();
    assert!(codes.len() > 40, "only {} codes read from ActionError", codes.len());
    let missing: Vec<_> = codes
        .iter()
        .filter(|code| !texts.contains(&format!("case \"{code}\":")))
        .collect();
    assert!(missing.is_empty(), "no text in Texts.message for {missing:?}");
}
