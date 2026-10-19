def sub(p, pairs):
    s = open(p, encoding='utf-8').read()
    for a, b in pairs:
        assert a in s, (p, a[:60])
        s = s.replace(a, b, 1)
    open(p, 'w', encoding='utf-8').write(s)

sub('crates/ui-egui/src/form.rs', [
('''    /// For `Bool` parameters.
    pub flag: bool,
}''', '''    /// For `Bool` parameters.
    pub flag: bool,
    /// For `PathList` parameters: the paths, in order.
    pub list: Vec<String>,
}

impl Field {
    /// A boolean that is on unless said otherwise has to send `false` explicitly.
    fn defaults_on(&self) -> bool {
        self.spec.default == Some("true")
    }
}'''),
('''                .map(|p| Field {
                    spec: *p,
                    text: String::new(),
                    flag: false,
                })''', '''                .map(|p| {
                    let mut field = Field {
                        spec: *p,
                        text: String::new(),
                        flag: false,
                        list: Vec::new(),
                    };
                    field.flag = field.defaults_on();
                    field
                })'''),
('''            if field.spec.kind == ParamKind::Bool {
                if field.flag {
                    object.insert(name.to_string(), Value::Bool(true));
                }
                continue;
            }''', '''            if field.spec.kind == ParamKind::Bool {
                if field.flag || field.defaults_on() {
                    object.insert(name.to_string(), Value::Bool(field.flag));
                }
                continue;
            }
            if field.spec.kind == ParamKind::PathList {
                if field.list.is_empty() {
                    if field.spec.required {
                        return Err(format!("`{name}` is required"));
                    }
                    continue;
                }
                let items = field.list.iter().map(|p| Value::String(p.clone())).collect();
                object.insert(name.to_string(), Value::Array(items));
                continue;
            }'''),
('''                ParamKind::Bool => continue,
            };''', '''                ParamKind::Bool | ParamKind::PathList => continue,
            };'''),
('''    /// Set a field's text, e.g. from a file dialog.''', '''    /// Set a path-list field, e.g. from dropped files.
    pub fn set_list(&mut self, name: &str, paths: Vec<String>) {
        if let Some(f) = self.fields.iter_mut().find(|f| f.spec.name == name) {
            f.list = paths;
        }
    }

    /// Set a field's text, e.g. from a file dialog.'''),
])
s = open('crates/ui-egui/src/form.rs', encoding='utf-8').read()
i = s.rindex('}')
s = s[:i] + '''
    #[test]
    fn path_lists_become_json_arrays() {
        let mut f = form("file.wizard");
        assert_eq!(f.to_params().unwrap_err(), "`images` is required");
        f.set_list("images", vec!["a.png".into(), "b.png".into()]);
        let p = f.to_params().unwrap();
        assert_eq!(p["images"], serde_json::json!(["a.png", "b.png"]));
        // `loop` is off by default and left out; the engine accepts the result.
        assert!(p.get("loop").is_none());
        let spec = Registry::standard().get("file.wizard").copied().unwrap();
        assert!(inbetween_engine::Params::parse(spec.params, &p).is_ok());
    }

    #[test]
    fn booleans_that_default_on_send_an_explicit_choice() {
        let mut f = form("project.saveAs");
        f.set("path", "x.inbetween");
        assert_eq!(f.to_params().unwrap()["thumbnail"], true);
        if let Some(field) = f.fields.iter_mut().find(|f| f.spec.name == "thumbnail") {
            field.flag = false;
        }
        assert_eq!(f.to_params().unwrap()["thumbnail"], false);
    }
}
'''
open('crates/ui-egui/src/form.rs', 'w', encoding='utf-8').write(s)

sub('crates/ui-egui/src/app.rs', [
('''                            ui.horizontal(|ui| {
                                if field.spec.kind == ParamKind::Bool {
                                    ui.checkbox(&mut field.flag, "");
                                } else {''', '''                            ui.horizontal(|ui| {
                                if field.spec.kind == ParamKind::PathList {
                                    path_list_editor(ui, field);
                                } else if field.spec.kind == ParamKind::Bool {
                                    ui.checkbox(&mut field.flag, "");
                                } else {'''),
('''fn browse(spec: &inbetween_engine::ParamSpec) -> Option<String> {''', '''/// The editor for a list of paths: one row per path with move and remove buttons, and an
/// Add button that opens a multi-select file dialog.
fn path_list_editor(ui: &mut egui::Ui, field: &mut crate::form::Field) {
    ui.vertical(|ui| {
        let mut remove = None;
        let mut swap = None;
        let count = field.list.len();
        for (i, path) in field.list.iter().enumerate() {
            ui.horizontal(|ui| {
                ui.label(format!("{}.", i + 1));
                let name = std::path::Path::new(path)
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| path.clone());
                ui.label(name).on_hover_text(path);
                if ui.add_enabled(i > 0, egui::Button::new("Up")).clicked() {
                    swap = Some((i - 1, i));
                }
                if ui
                    .add_enabled(i + 1 < count, egui::Button::new("Down"))
                    .clicked()
                {
                    swap = Some((i, i + 1));
                }
                if ui.button("Remove").clicked() {
                    remove = Some(i);
                }
            });
        }
        if let Some((a, b)) = swap {
            field.list.swap(a, b);
        }
        if let Some(i) = remove {
            field.list.remove(i);
        }
        if ui.button("Add...").clicked() {
            field.list.extend(browse_many(&field.spec));
        }
    });
}

fn browse_many(spec: &inbetween_engine::ParamSpec) -> Vec<String> {
    let mut dialog = rfd::FileDialog::new();
    if !spec.extensions.is_empty() {
        dialog = dialog.add_filter("Files", spec.extensions);
    }
    dialog
        .pick_files()
        .unwrap_or_default()
        .into_iter()
        .map(|p| p.display().to_string())
        .collect()
}

fn browse(spec: &inbetween_engine::ParamSpec) -> Option<String> {'''),
('''                if matches!(id, "project.open" | "project.new") {''', '''                if matches!(
                    id,
                    "project.open" | "project.new" | "file.new" | "file.wizard"
                ) {'''),
])
