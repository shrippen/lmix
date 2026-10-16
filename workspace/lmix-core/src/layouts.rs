//! Layout templates (LAY.5) as a file: lmix-core's settings file
//! (`--layout-templates`) and the files they are exported to and imported
//! from have the same form.
//!
//! ```json
//! { "format": "lmix-layouts", "version": 1, "templates": [...] }
//! ```

use lmix_types::layout::TEMPLATES_FORMAT;
use lmix_types::LayoutTemplate;
use serde::{Deserialize, Serialize};

/// Current file version.
const VERSION: u32 = 1;

#[derive(Serialize, Deserialize)]
struct TemplatesFile {
    format: String,
    version: u32,
    templates: Vec<LayoutTemplate>,
}

/// Templates of a file; one of another kind or a newer version is
/// refused.
pub fn parse(bytes: &[u8]) -> Result<Vec<LayoutTemplate>, String> {
    let file: TemplatesFile = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
    if file.format != TEMPLATES_FORMAT {
        return Err(format!("not a file of layout templates ({})", file.format));
    }
    if file.version > VERSION {
        return Err(format!(
            "version {} is newer than this lmix ({VERSION})",
            file.version
        ));
    }
    Ok(file.templates)
}

pub fn to_bytes(templates: &[LayoutTemplate]) -> Vec<u8> {
    let file = TemplatesFile {
        format: TEMPLATES_FORMAT.into(),
        version: VERSION,
        templates: templates.to_vec(),
    };
    serde_json::to_vec_pretty(&file).expect("templates serialize")
}

#[cfg(test)]
mod tests {
    use super::*;
    use lmix_types::{BoxCrop, BoxRect, TemplateBox, TemplateInput};

    #[test]
    fn templates_go_through_a_file_and_others_are_refused() {
        let templates = vec![LayoutTemplate {
            name: "Interview".into(),
            background: Some(TemplateInput {
                number: 1,
                name: "Wide".into(),
            }),
            boxes: vec![TemplateBox {
                rect: BoxRect {
                    x: 0.5,
                    y: 0.0,
                    w: 0.5,
                    h: 0.5,
                },
                input: TemplateInput {
                    number: 2,
                    name: "Guest".into(),
                },
                border_px: 4,
                crop: BoxCrop {
                    left: 0.1,
                    ..BoxCrop::default()
                },
                locked: true,
            }],
        }];

        assert_eq!(parse(&to_bytes(&templates)).unwrap(), templates);
        let show = br#"{"format": "lmix-show", "version": 1, "templates": []}"#;
        assert!(parse(show).unwrap_err().contains("not a file"));
        let newer = br#"{"format": "lmix-layouts", "version": 9, "templates": []}"#;
        assert!(parse(newer).unwrap_err().contains("newer"));
        assert!(parse(b"[").is_err());
    }
}
