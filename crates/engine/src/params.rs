use crate::CommandError;
use serde::Serialize;
use serde_json::Value;
use std::collections::BTreeMap;

/// Longest string or path parameter accepted, in bytes.
pub const MAX_STRING_BYTES: usize = 4096;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ParamKind {
    /// A file or directory path.
    Path,
    String,
    Integer,
    Number,
    Bool,
}

/// One parameter in a command's schema. Built with `const fn`s so each command can keep its
/// schema in a `static`.
#[derive(Clone, Copy, Debug, Serialize)]
pub struct ParamSpec {
    pub name: &'static str,
    pub kind: ParamKind,
    pub required: bool,
    pub description: &'static str,
    pub min: Option<f64>,
    pub max: Option<f64>,
    /// What happens when the parameter is left out, in words.
    pub default: Option<&'static str>,
}

impl ParamSpec {
    pub const fn new(
        name: &'static str,
        kind: ParamKind,
        required: bool,
        description: &'static str,
    ) -> Self {
        Self {
            name,
            kind,
            required,
            description,
            min: None,
            max: None,
            default: None,
        }
    }

    /// Inclusive limits for `Integer` and `Number` parameters.
    pub const fn range(mut self, min: f64, max: f64) -> Self {
        self.min = Some(min);
        self.max = Some(max);
        self
    }

    pub const fn default_is(mut self, text: &'static str) -> Self {
        self.default = Some(text);
        self
    }
}

/// Validated parameters. Every value has already been checked against the schema, so
/// commands only decide what to do with them.
#[derive(Clone, Debug, Default)]
pub struct Params {
    values: BTreeMap<String, Value>,
}

impl Params {
    /// Check `value` (a JSON object, or null for none) against `schema`.
    pub fn parse(schema: &[ParamSpec], value: &Value) -> Result<Self, CommandError> {
        let empty = serde_json::Map::new();
        let object = match value {
            Value::Null => &empty,
            Value::Object(o) => o,
            _ => return Err(bad("parameters must be a JSON object")),
        };
        for key in object.keys() {
            if !schema.iter().any(|p| p.name == key) {
                let known: Vec<&str> = schema.iter().map(|p| p.name).collect();
                let known = if known.is_empty() {
                    "none".to_string()
                } else {
                    known.join(", ")
                };
                return Err(bad(format!(
                    "unknown parameter `{}` (this command takes: {known})",
                    shorten(key)
                )));
            }
        }
        let mut values = BTreeMap::new();
        for spec in schema {
            match object.get(spec.name) {
                None | Some(Value::Null) if spec.required => {
                    return Err(bad(format!("missing required parameter `{}`", spec.name)))
                }
                None | Some(Value::Null) => {}
                Some(v) => {
                    values.insert(spec.name.to_string(), check(spec, v)?);
                }
            }
        }
        Ok(Self { values })
    }

    pub fn string(&self, name: &str) -> Option<&str> {
        self.values.get(name)?.as_str()
    }

    pub fn integer(&self, name: &str) -> Option<i64> {
        self.values.get(name)?.as_i64()
    }

    pub fn number(&self, name: &str) -> Option<f64> {
        self.values.get(name)?.as_f64()
    }

    pub fn boolean(&self, name: &str) -> Option<bool> {
        self.values.get(name)?.as_bool()
    }

    pub fn has(&self, name: &str) -> bool {
        self.values.contains_key(name)
    }
}

fn bad(message: impl Into<String>) -> CommandError {
    CommandError::BadParams(message.into())
}

fn shorten(s: &str) -> String {
    if s.chars().count() > 40 {
        let head: String = s.chars().take(40).collect();
        format!("{head}...")
    } else {
        s.to_string()
    }
}

fn check(spec: &ParamSpec, v: &Value) -> Result<Value, CommandError> {
    let name = spec.name;
    match spec.kind {
        ParamKind::Path | ParamKind::String => {
            let Value::String(s) = v else {
                return Err(bad(format!("`{name}` must be a string")));
            };
            if s.is_empty() {
                return Err(bad(format!("`{name}` must not be empty")));
            }
            if s.len() > MAX_STRING_BYTES {
                return Err(bad(format!(
                    "`{name}` is longer than {MAX_STRING_BYTES} bytes"
                )));
            }
            if s.contains('\0') {
                return Err(bad(format!("`{name}` must not contain a NUL character")));
            }
            Ok(v.clone())
        }
        ParamKind::Integer => {
            let n = v
                .as_i64()
                .or_else(|| {
                    // Accept 3.0 as 3, but nothing fractional or beyond exact f64 range.
                    let f = v.as_f64()?;
                    (f.is_finite() && f.fract() == 0.0 && f.abs() <= 9.0e15).then_some(f as i64)
                })
                .ok_or_else(|| bad(format!("`{name}` must be a whole number")))?;
            in_range(spec, n as f64)?;
            Ok(Value::from(n))
        }
        ParamKind::Number => {
            let f = v
                .as_f64()
                .filter(|f| f.is_finite())
                .ok_or_else(|| bad(format!("`{name}` must be a finite number")))?;
            in_range(spec, f)?;
            Ok(v.clone())
        }
        ParamKind::Bool => match v {
            Value::Bool(_) => Ok(v.clone()),
            _ => Err(bad(format!("`{name}` must be true or false"))),
        },
    }
}

fn in_range(spec: &ParamSpec, x: f64) -> Result<(), CommandError> {
    let below = spec.min.is_some_and(|m| x < m);
    let above = spec.max.is_some_and(|m| x > m);
    if below || above {
        let lo = spec.min.map_or("-inf".to_string(), |m| m.to_string());
        let hi = spec.max.map_or("inf".to_string(), |m| m.to_string());
        return Err(bad(format!(
            "`{}` must be between {lo} and {hi}",
            spec.name
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const SCHEMA: &[ParamSpec] = &[
        ParamSpec::new("path", ParamKind::Path, true, "a file"),
        ParamSpec::new("n", ParamKind::Integer, false, "count").range(1.0, 10.0),
        ParamSpec::new("t", ParamKind::Number, false, "time").range(0.0, 1.0),
        ParamSpec::new("flag", ParamKind::Bool, false, "switch"),
    ];

    fn parse(v: Value) -> Result<Params, CommandError> {
        Params::parse(SCHEMA, &v)
    }

    #[test]
    fn valid_params_round_trip_to_getters() {
        let p = parse(json!({"path": "a.png", "n": 3, "t": 0.25, "flag": true})).unwrap();
        assert_eq!(p.string("path"), Some("a.png"));
        assert_eq!(p.integer("n"), Some(3));
        assert_eq!(p.number("t"), Some(0.25));
        assert_eq!(p.boolean("flag"), Some(true));
        assert!(!p.has("missing"));
    }

    #[test]
    fn optional_and_null_params_may_be_absent() {
        let p = parse(json!({"path": "a", "n": null})).unwrap();
        assert!(!p.has("n"));
        assert!(Params::parse(&[], &Value::Null).is_ok());
        assert!(Params::parse(&[], &json!({})).is_ok());
    }

    #[test]
    fn integral_floats_are_accepted_fractions_are_not() {
        assert_eq!(
            parse(json!({"path": "a", "n": 4.0})).unwrap().integer("n"),
            Some(4)
        );
        assert!(parse(json!({"path": "a", "n": 4.5})).is_err());
    }

    #[test]
    fn bad_params_are_reported() {
        let cases = [
            json!({}),
            json!([]),
            json!("x"),
            json!(5),
            json!({"path": 5}),
            json!({"path": ""}),
            json!({"path": "a\u{0}b"}),
            json!({"path": "x".repeat(MAX_STRING_BYTES + 1)}),
            json!({"path": "a", "extra": 1}),
            json!({"path": "a", "n": 0}),
            json!({"path": "a", "n": 11}),
            json!({"path": "a", "n": "3"}),
            json!({"path": "a", "n": 1e300}),
            json!({"path": "a", "n": u64::MAX}),
            json!({"path": "a", "t": 1.5}),
            json!({"path": "a", "t": -0.1}),
            json!({"path": "a", "t": "0.5"}),
            json!({"path": "a", "flag": 1}),
            json!({"path": ["a"]}),
            json!({"path": {"x": 1}}),
        ];
        for c in cases {
            assert!(
                matches!(parse(c.clone()), Err(CommandError::BadParams(_))),
                "accepted {c}"
            );
        }
        assert!(matches!(
            Params::parse(SCHEMA, &Value::Null),
            Err(CommandError::BadParams(_))
        ));
    }

    #[test]
    fn unknown_parameter_error_lists_the_known_ones() {
        let e = parse(json!({"path": "a", "bogus": 1}))
            .unwrap_err()
            .to_string();
        assert!(e.contains("bogus") && e.contains("path, n, t, flag"), "{e}");
    }

    #[test]
    fn schema_serialises_for_tools() {
        let v = serde_json::to_value(SCHEMA[1]).unwrap();
        assert_eq!(v["name"], "n");
        assert_eq!(v["kind"], "integer");
        assert_eq!(v["min"], 1.0);
        assert_eq!(v["required"], false);
    }
}
