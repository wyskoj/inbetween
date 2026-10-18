//! A parameter form generated from a command's schema, for commands the menu cannot run on
//! its own (they need a file, a number...). The same schema drives the CLI and MCP.

use inbetween_engine::{CommandSpec, ParamKind, ParamSpec};
use serde_json::{Map, Value};

#[derive(Clone, Debug)]
pub struct Field {
    pub spec: ParamSpec,
    /// What the person typed (or picked in a file dialog).
    pub text: String,
    /// For `Bool` parameters.
    pub flag: bool,
}

#[derive(Clone, Debug)]
pub struct Form {
    pub id: &'static str,
    pub title: String,
    pub description: String,
    pub fields: Vec<Field>,
    /// The last problem, shown under the fields.
    pub error: Option<String>,
}

impl Form {
    pub fn for_spec(spec: &CommandSpec) -> Self {
        Self {
            id: spec.id,
            title: spec.label.to_string(),
            description: spec.description.to_string(),
            fields: spec
                .params
                .iter()
                .map(|p| Field {
                    spec: *p,
                    text: String::new(),
                    flag: false,
                })
                .collect(),
            error: None,
        }
    }

    /// Turn the typed values into the JSON object the command expects. Empty optional fields
    /// are left out so the command's own default applies. The engine still checks ranges.
    pub fn to_params(&self) -> Result<Value, String> {
        let mut object = Map::new();
        for field in &self.fields {
            let name = field.spec.name;
            let text = field.text.trim();
            if field.spec.kind == ParamKind::Bool {
                if field.flag {
                    object.insert(name.to_string(), Value::Bool(true));
                }
                continue;
            }
            if text.is_empty() {
                if field.spec.required {
                    return Err(format!("`{name}` is required"));
                }
                continue;
            }
            let value = match field.spec.kind {
                ParamKind::Path | ParamKind::String => Value::String(text.to_string()),
                ParamKind::Integer => Value::from(
                    text.parse::<i64>()
                        .map_err(|_| format!("`{name}` must be a whole number"))?,
                ),
                ParamKind::Number => {
                    let n: f64 = text
                        .parse()
                        .map_err(|_| format!("`{name}` must be a number"))?;
                    if !n.is_finite() {
                        return Err(format!("`{name}` must be a finite number"));
                    }
                    Value::from(n)
                }
                ParamKind::Bool => continue,
            };
            object.insert(name.to_string(), value);
        }
        Ok(Value::Object(object))
    }

    /// Set a field's text, e.g. from a file dialog.
    pub fn set(&mut self, name: &str, text: impl Into<String>) {
        if let Some(f) = self.fields.iter_mut().find(|f| f.spec.name == name) {
            f.text = text.into();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use inbetween_engine::Registry;

    fn form(id: &str) -> Form {
        Form::for_spec(Registry::standard().get(id).unwrap())
    }

    #[test]
    fn required_fields_must_be_filled() {
        let mut f = form("project.open");
        assert_eq!(f.to_params().unwrap_err(), "`path` is required");
        f.set("path", "  C:/demo/demo.inbetween  ");
        assert_eq!(f.to_params().unwrap()["path"], "C:/demo/demo.inbetween");
    }

    #[test]
    fn optional_fields_are_left_out_when_empty() {
        let mut f = form("export.frame");
        f.set("out", "mid.png");
        let p = f.to_params().unwrap();
        assert_eq!(p.as_object().unwrap().len(), 1);
        f.set("t", "0.5");
        f.set("sequence", "1");
        let p = f.to_params().unwrap();
        assert_eq!(p["t"], 0.5);
        assert_eq!(p["sequence"], 1);
    }

    #[test]
    fn numbers_are_parsed_strictly() {
        let mut f = form("export.frame");
        f.set("out", "x.png");
        for (field, bad) in [
            ("t", "abc"),
            ("t", "NaN"),
            ("t", "inf"),
            ("frame", "1.5"),
            ("sequence", "one"),
        ] {
            f.set(field, bad);
            assert!(f.to_params().is_err(), "{field}={bad}");
            f.set(field, "");
        }
        assert!(f.to_params().is_ok());
    }

    #[test]
    fn every_command_gets_a_form_whose_defaults_are_empty_params() {
        let registry = Registry::standard();
        for spec in registry.specs() {
            let f = Form::for_spec(spec);
            assert_eq!(f.fields.len(), spec.params.len());
            let needs_input = spec.params.iter().any(|p| p.required);
            assert_eq!(f.to_params().is_err(), needs_input, "{}", spec.id);
        }
    }

    #[test]
    fn form_output_passes_engine_validation_for_the_open_command() {
        // The form's JSON is accepted by the engine's own schema check.
        let registry = Registry::standard();
        let mut f = form("project.new");
        f.set("image1", "a.png");
        f.set("image2", "b.png");
        f.set("name", "demo");
        let params = f.to_params().unwrap();
        let spec = registry.get("project.new").unwrap();
        assert!(inbetween_engine::Params::parse(spec.params, &params).is_ok());
    }
}
