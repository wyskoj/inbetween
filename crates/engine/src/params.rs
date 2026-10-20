use crate::CommandError;
use serde::Serialize;
use serde_json::Value;
use std::collections::BTreeMap;

/// Longest string or path parameter accepted, in bytes.
pub const MAX_STRING_BYTES: usize = 4096;
/// Most entries a path-list parameter accepts.
pub const MAX_LIST_ENTRIES: usize = 1000;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ParamKind {
    /// A file or directory path.
    Path,
    /// A JSON array of paths, in order. `min` and `max` bound the number of entries.
    PathList,
    /// A JSON array of whole numbers (for example dot indices). `min` and `max` bound the
    /// number of entries.
    IntegerList,
    /// A JSON array of finite numbers (for example x, y, x, y coordinates). `min` and `max`
    /// bound the number of entries.
    NumberList,
    String,
    Integer,
    Number,
    Bool,
}

/// How a `Path` parameter is used, so a shell can offer the right file dialog.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PathUse {
    /// An existing file to read.
    OpenFile,
    /// A file to create or overwrite.
    SaveFile,
    /// A folder (created if missing).
    Folder,
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
    /// For `Path` parameters: what the path is for.
    pub path_use: Option<PathUse>,
    /// For `Path` parameters: file extensions a dialog should offer, without the dot.
    pub extensions: &'static [&'static str],
    /// For `String` parameters: the only values accepted, when the set is fixed.
    pub choices: &'static [&'static str],
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
            path_use: None,
            extensions: &[],
            choices: &[],
        }
    }

    /// A path to an existing file with one of `extensions`.
    pub const fn open_file(mut self, extensions: &'static [&'static str]) -> Self {
        self.path_use = Some(PathUse::OpenFile);
        self.extensions = extensions;
        self
    }

    /// A path to a file to write, with one of `extensions`.
    pub const fn save_file(mut self, extensions: &'static [&'static str]) -> Self {
        self.path_use = Some(PathUse::SaveFile);
        self.extensions = extensions;
        self
    }

    /// A folder to write into.
    pub const fn folder(mut self) -> Self {
        self.path_use = Some(PathUse::Folder);
        self
    }

    /// Restrict a `String` parameter to these values.
    pub const fn one_of(mut self, choices: &'static [&'static str]) -> Self {
        self.choices = choices;
        self
    }

    /// A list of paths to existing files with one of `extensions`.
    pub const fn open_files(mut self, extensions: &'static [&'static str]) -> Self {
        self.path_use = Some(PathUse::OpenFile);
        self.extensions = extensions;
        self
    }

    /// Inclusive limits for `Integer` and `Number` parameters (and the entry count of a
    /// `PathList`).
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

    /// The entries of a `PathList` parameter, in order.
    pub fn strings(&self, name: &str) -> Option<Vec<&str>> {
        self.values
            .get(name)?
            .as_array()
            .map(|a| a.iter().filter_map(Value::as_str).collect())
    }

    /// The entries of an `IntegerList` parameter, in order.
    pub fn integers(&self, name: &str) -> Option<Vec<i64>> {
        self.values
            .get(name)?
            .as_array()
            .map(|a| a.iter().filter_map(Value::as_i64).collect())
    }

    /// The entries of a `NumberList` parameter, in order.
    pub fn numbers(&self, name: &str) -> Option<Vec<f64>> {
        self.values
            .get(name)?
            .as_array()
            .map(|a| a.iter().filter_map(Value::as_f64).collect())
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

/// Most entries an `IntegerList` or `NumberList` accepts.
const MAX_NUMBER_LIST: usize = 100_000;

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
            if !spec.choices.is_empty() && !spec.choices.contains(&s.as_str()) {
                return Err(bad(format!(
                    "`{name}` must be one of: {}",
                    spec.choices.join(", ")
                )));
            }
            Ok(v.clone())
        }
        ParamKind::PathList => {
            let Value::Array(items) = v else {
                return Err(bad(format!("`{name}` must be a list of paths")));
            };
            if items.len() > MAX_LIST_ENTRIES {
                return Err(bad(format!(
                    "`{name}` has {} entries; at most {MAX_LIST_ENTRIES} are accepted",
                    items.len()
                )));
            }
            in_range(spec, items.len() as f64).map_err(|_| {
                bad(format!(
                    "`{name}` needs between {} and {} entries, got {}",
                    spec.min.unwrap_or(0.0),
                    spec.max.unwrap_or(MAX_LIST_ENTRIES as f64),
                    items.len()
                ))
            })?;
            for item in items {
                let Value::String(s) = item else {
                    return Err(bad(format!("every entry of `{name}` must be a string")));
                };
                if s.is_empty() {
                    return Err(bad(format!("`{name}` must not hold an empty path")));
                }
                if s.len() > MAX_STRING_BYTES {
                    return Err(bad(format!(
                        "an entry of `{name}` is longer than {MAX_STRING_BYTES} bytes"
                    )));
                }
                if s.contains('\0') {
                    return Err(bad(format!(
                        "an entry of `{name}` must not contain a NUL character"
                    )));
                }
            }
            Ok(v.clone())
        }
        ParamKind::IntegerList | ParamKind::NumberList => {
            let whole = spec.kind == ParamKind::IntegerList;
            let Value::Array(items) = v else {
                return Err(bad(format!("`{name}` must be a list of numbers")));
            };
            if items.len() > MAX_NUMBER_LIST {
                return Err(bad(format!(
                    "`{name}` has {} entries; at most {MAX_NUMBER_LIST} are accepted",
                    items.len()
                )));
            }
            in_range(spec, items.len() as f64).map_err(|_| {
                bad(format!(
                    "`{name}` needs between {} and {} entries, got {}",
                    spec.min.unwrap_or(0.0),
                    spec.max.unwrap_or(MAX_NUMBER_LIST as f64),
                    items.len()
                ))
            })?;
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                let f = item.as_f64().filter(|f| f.is_finite()).ok_or_else(|| {
                    bad(format!("every entry of `{name}` must be a finite number"))
                })?;
                if whole {
                    if f.fract() != 0.0 || f.abs() > 9.0e15 {
                        return Err(bad(format!(
                            "every entry of `{name}` must be a whole number"
                        )));
                    }
                    out.push(Value::from(f as i64));
                } else {
                    out.push(item.clone());
                }
            }
            Ok(Value::Array(out))
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
    fn number_lists_are_validated() {
        const LISTS: &[ParamSpec] = &[
            ParamSpec::new("ids", ParamKind::IntegerList, false, "ids").range(1.0, 3.0),
            ParamSpec::new("xs", ParamKind::NumberList, false, "xs"),
        ];
        let p = Params::parse(LISTS, &json!({"ids": [1, 2.0], "xs": [0.5, -1]})).unwrap();
        assert_eq!(p.integers("ids"), Some(vec![1, 2]));
        assert_eq!(p.numbers("xs"), Some(vec![0.5, -1.0]));
        for bad in [
            json!({"ids": []}),
            json!({"ids": [1, 2, 3, 4]}),
            json!({"ids": [1.5]}),
            json!({"ids": ["a"]}),
            json!({"ids": 1}),
            json!({"xs": [f64::NAN]}),
            json!({"xs": [null]}),
        ] {
            assert!(Params::parse(LISTS, &bad).is_err(), "accepted {bad}");
        }
        assert_eq!(
            serde_json::to_value(LISTS[0]).unwrap()["kind"],
            "integer_list"
        );
    }

    #[test]
    fn path_lists_are_validated() {
        const LIST: &[ParamSpec] = &[ParamSpec::new("files", ParamKind::PathList, true, "files")
            .open_files(&["png"])
            .range(2.0, 4.0)];
        let ok = Params::parse(LIST, &json!({"files": ["a.png", "b.png"]})).unwrap();
        assert_eq!(ok.strings("files"), Some(vec!["a.png", "b.png"]));
        assert_eq!(ok.strings("missing"), None);
        for bad in [
            json!({"files": "a.png"}),
            json!({"files": ["a.png"]}),                 // too few
            json!({"files": ["a", "b", "c", "d", "e"]}), // too many for the schema
            json!({"files": ["a", 5]}),
            json!({"files": ["a", ""]}),
            json!({"files": ["a", "b\u{0}"]}),
            json!({"files": ["a", "x".repeat(MAX_STRING_BYTES + 1)]}),
            json!({"files": [["a"], ["b"]]}),
            json!({"files": {"0": "a"}}),
            json!({}),
        ] {
            assert!(
                matches!(Params::parse(LIST, &bad), Err(CommandError::BadParams(_))),
                "accepted {bad}"
            );
        }
        // The hard cap holds even for a schema with no range.
        const ANY: &[ParamSpec] = &[ParamSpec::new("files", ParamKind::PathList, true, "files")];
        let many = vec!["a"; MAX_LIST_ENTRIES + 1];
        assert!(Params::parse(ANY, &json!({ "files": many })).is_err());
        let fine = vec!["a"; MAX_LIST_ENTRIES];
        assert!(Params::parse(ANY, &json!({ "files": fine })).is_ok());
        assert_eq!(serde_json::to_value(LIST[0]).unwrap()["kind"], "path_list");
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
