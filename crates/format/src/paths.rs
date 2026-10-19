use crate::FormatError;

/// Longest image path accepted, in characters.
pub const MAX_PATH_CHARS: usize = 4096;

/// Accept an image path as stored in a project file (an empty path means "not set yet").
///
/// Paths are relative to the project's folder where possible and absolute only when no
/// relative path exists (another drive). Rejected: NUL bytes and over-long paths. `..` is
/// allowed so a project can sit beside an `images` folder; callers resolve against the
/// project's directory and decide whether to follow it.
pub fn check_image_path(path: &str) -> Result<(), FormatError> {
    let bad = |reason| {
        Err(FormatError::BadPath {
            path: path.chars().take(80).collect(),
            reason,
        })
    };
    if path.contains('\0') {
        return bad("contains a NUL character");
    }
    if path.chars().count() > MAX_PATH_CHARS {
        return bad("is too long");
    }
    Ok(())
}

/// The path of `target` relative to the directory `base`, using `/` separators, for storing
/// in a project file. Both must be absolute or both relative. Pure string work: nothing
/// touches the file system. Returns `None` if no relative path exists (different drives, or
/// `base` goes up through `..`).
pub fn make_relative(base: &str, target: &str) -> Option<String> {
    let split = |s: &str| -> Option<(String, Vec<String>)> {
        let s = s.replace('\\', "/");
        let (root, rest) = match s.split_once(':') {
            Some((d, rest)) if d.len() == 1 => {
                (format!("{}:", d.to_ascii_lowercase()), rest.to_string())
            }
            _ => (String::new(), s.clone()),
        };
        let absolute = rest.starts_with('/');
        let mut parts: Vec<String> = Vec::new();
        for part in rest.split('/') {
            match part {
                "" | "." => {}
                ".." => {
                    parts.pop()?;
                }
                p => parts.push(p.to_string()),
            }
        }
        Some((format!("{root}{}", if absolute { "/" } else { "" }), parts))
    };
    let (base_root, base_parts) = split(base)?;
    let (target_root, target_parts) = split(target)?;
    if base_root != target_root {
        return None;
    }
    let common = base_parts
        .iter()
        .zip(&target_parts)
        .take_while(|(a, b)| a == b)
        .count();
    let mut out: Vec<&str> = Vec::new();
    out.extend(std::iter::repeat_n("..", base_parts.len() - common));
    out.extend(target_parts.iter().skip(common).map(String::as_str));
    Some(out.join("/"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_relative_absolute_and_empty_paths() {
        for p in [
            "",
            "a.png",
            "images/a.png",
            "../images/a.png",
            "./a.png",
            "dir\\a.png",
            "/etc/a.png",
            "C:\\a.png",
            "d:/x/a.png",
            "\\\\server\\share\\a.png",
        ] {
            assert!(check_image_path(p).is_ok(), "{p}");
        }
    }

    #[test]
    fn rejects_nul_and_over_long_paths() {
        for p in ["a\0b".to_string(), "a".repeat(MAX_PATH_CHARS + 1)] {
            assert!(
                matches!(check_image_path(&p), Err(FormatError::BadPath { .. })),
                "{}",
                p.len()
            );
        }
        assert!(check_image_path(&"a".repeat(MAX_PATH_CHARS)).is_ok());
    }

    #[test]
    fn relative_paths_between_files() {
        let r = |b, t| make_relative(b, t);
        assert_eq!(r("/p/proj", "/p/proj/a.png").as_deref(), Some("a.png"));
        assert_eq!(
            r("/p/proj", "/p/images/a.png").as_deref(),
            Some("../images/a.png")
        );
        assert_eq!(
            r("/p/proj", "/p/proj/img/sub/a.png").as_deref(),
            Some("img/sub/a.png")
        );
        assert_eq!(
            r("C:\\work\\p", "C:\\work\\imgs\\a.png").as_deref(),
            Some("../imgs/a.png")
        );
        assert_eq!(r("c:/work", "C:/work/a.png").as_deref(), Some("a.png"));
        assert_eq!(
            r("/p/./x/../proj", "/p/proj/a.png").as_deref(),
            Some("a.png")
        );
        assert_eq!(r("/p", "/p").as_deref(), Some(""));
    }

    #[test]
    fn no_relative_path_across_roots_or_through_the_top() {
        assert_eq!(make_relative("C:/a", "D:/a/x.png"), None);
        assert_eq!(make_relative("/a", "rel/x.png"), None);
        assert_eq!(make_relative("/../a", "/a/x.png"), None);
    }

    #[test]
    fn results_pass_the_path_check() {
        let rel = make_relative("/p/proj", "/p/images/a.png").unwrap();
        assert!(check_image_path(&rel).is_ok());
    }
}
