def sub(p, pairs):
    s = open(p, encoding='utf-8').read()
    for a, b in pairs:
        assert a in s, (p, a[:60])
        s = s.replace(a, b, 1)
    open(p, 'w', encoding='utf-8').write(s)

# params.rs
sub('crates/engine/src/params.rs', [
('''/// Longest string or path parameter accepted, in bytes.
pub const MAX_STRING_BYTES: usize = 4096;
''', '''/// Longest string or path parameter accepted, in bytes.
pub const MAX_STRING_BYTES: usize = 4096;
/// Most entries a path-list parameter accepts.
pub const MAX_LIST_ENTRIES: usize = 1000;
'''),
('''    /// A file or directory path.
    Path,
''', '''    /// A file or directory path.
    Path,
    /// A JSON array of paths, in order. `min` and `max` bound the number of entries.
    PathList,
'''),
('''    /// Inclusive limits for `Integer` and `Number` parameters.''', '''    /// A list of paths to existing files with one of `extensions`.
    pub const fn open_files(mut self, extensions: &'static [&'static str]) -> Self {
        self.path_use = Some(PathUse::OpenFile);
        self.extensions = extensions;
        self
    }

    /// Inclusive limits for `Integer` and `Number` parameters (and the entry count of a
    /// `PathList`).'''),
('''    pub fn integer(&self, name: &str) -> Option<i64> {''', '''    /// The entries of a `PathList` parameter, in order.
    pub fn strings(&self, name: &str) -> Option<Vec<&str>> {
        self.values
            .get(name)?
            .as_array()
            .map(|a| a.iter().filter_map(Value::as_str).collect())
    }

    pub fn integer(&self, name: &str) -> Option<i64> {'''),
('''        ParamKind::Integer => {
            let n = v''', '''        ParamKind::PathList => {
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
                if s.contains('\\0') {
                    return Err(bad(format!(
                        "an entry of `{name}` must not contain a NUL character"
                    )));
                }
            }
            Ok(v.clone())
        }
        ParamKind::Integer => {
            let n = v'''),
])
s = open('crates/engine/src/params.rs', encoding='utf-8').read()
# tests: extend the test schema
i = s.index('    #[test]\n    fn schema_serialises_for_tools()')
s = s[:i] + '''    #[test]
    fn path_lists_are_validated() {
        const LIST: &[ParamSpec] = &[ParamSpec::new("files", ParamKind::PathList, true, "files")
            .open_files(&["png"])
            .range(2.0, 4.0)];
        let ok = Params::parse(LIST, &json!({"files": ["a.png", "b.png"]})).unwrap();
        assert_eq!(ok.strings("files"), Some(vec!["a.png", "b.png"]));
        assert_eq!(ok.strings("missing"), None);
        for bad in [
            json!({"files": "a.png"}),
            json!({"files": ["a.png"]}),                       // too few
            json!({"files": ["a", "b", "c", "d", "e"]}),       // too many for the schema
            json!({"files": ["a", 5]}),
            json!({"files": ["a", ""]}),
            json!({"files": ["a", "b\\u{0}"]}),
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

''' + s[i:]
open('crates/engine/src/params.rs', 'w', encoding='utf-8').write(s)

sub('crates/engine/src/lib.rs', [('pub use params::{ParamKind, ParamSpec, Params, PathUse, MAX_STRING_BYTES};', 'pub use params::{ParamKind, ParamSpec, Params, PathUse, MAX_LIST_ENTRIES, MAX_STRING_BYTES};')])

# project_cmds: share absolute/stem
sub('crates/engine/src/project_cmds.rs', [('''    /// An absolute path as a string, whether or not the file exists yet.
    fn absolute(path: &str) -> Result<String, CommandError> {
        std::path::absolute(path)
            .map(|p| p.display().to_string())
            .map_err(|e| CommandError::Io(format!("{path}: {e}")))
    }

    fn stem(path: &str) -> String {
        Path::new(path)
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default()
    }

''', '''    use crate::file_cmds::{absolute, stem};

''')])
