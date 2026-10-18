use crate::{
    dots_from_bytes, dots_to_string, project_from_bytes, project_to_string, FormatError,
    MAX_FILE_BYTES,
};
use inbetween_doc::{DotSet, Project};
use std::io::Read;
use std::path::Path;

fn io(path: &Path, e: std::io::Error) -> FormatError {
    FormatError::Io(format!("{}: {e}", path.display()))
}

/// Read at most the size limit (plus one byte, to notice an oversized file) without ever
/// loading a huge file into memory.
fn read_limited(path: &Path) -> Result<Vec<u8>, FormatError> {
    // Only regular files: opening a device (CON or NUL on Windows, /dev/zero or a
    // FIFO elsewhere) could block forever or never end.
    let meta = std::fs::metadata(path).map_err(|e| io(path, e))?;
    if !meta.is_file() {
        return Err(FormatError::Io(format!(
            "{}: not a regular file",
            path.display()
        )));
    }
    let file = std::fs::File::open(path).map_err(|e| io(path, e))?;
    let mut bytes = Vec::new();
    file.take(MAX_FILE_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| io(path, e))?;
    Ok(bytes)
}

/// Write via a temporary file in the same directory, then rename, so a crash or a full
/// disk never leaves a half-written file where a good one used to be.
fn write_atomic(path: &Path, text: &str) -> Result<(), FormatError> {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("file");
    let tmp = path.with_file_name(format!(".{name}.{}.tmp", std::process::id()));
    let result = std::fs::write(&tmp, text).and_then(|()| std::fs::rename(&tmp, path));
    if let Err(e) = result {
        let _ = std::fs::remove_file(&tmp);
        return Err(io(path, e));
    }
    Ok(())
}

/// Load a `.inbetween` project file.
pub fn read_project(path: &Path) -> Result<Project, FormatError> {
    project_from_bytes(&read_limited(path)?)
}

/// Save a project atomically. Nothing is written if the project is invalid.
pub fn write_project(path: &Path, project: &Project) -> Result<(), FormatError> {
    write_atomic(path, &project_to_string(project)?)
}

/// Load a `.inbetweendots` file.
pub fn read_dots(path: &Path) -> Result<DotSet, FormatError> {
    dots_from_bytes(&read_limited(path)?)
}

/// Save a dot set atomically.
pub fn write_dots(path: &Path, dots: &DotSet) -> Result<(), FormatError> {
    write_atomic(path, &dots_to_string(dots)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use inbetween_doc::{DotPair, ImageRef, Sequence};
    use inbetween_geom::Point;

    struct TempDir(std::path::PathBuf);

    impl TempDir {
        fn new(tag: &str) -> Self {
            let dir =
                std::env::temp_dir().join(format!("inbetween-format-{tag}-{}", std::process::id()));
            std::fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn project() -> Project {
        Project {
            sequences: vec![Sequence {
                image1: ImageRef::new("a.png"),
                image2: ImageRef::new("sub/b.png"),
                dots: DotSet::new(vec![DotPair::new(
                    Point::new(1.0, 2.0),
                    Point::new(3.0, 4.0),
                )]),
                ..Sequence::default()
            }],
            ..Project::default()
        }
    }

    #[test]
    fn project_file_round_trip() {
        let dir = TempDir::new("proj");
        let path = dir.0.join("movie.inbetween");
        write_project(&path, &project()).unwrap();
        assert_eq!(read_project(&path).unwrap(), project());
        // Overwriting works and leaves no temporary file behind.
        let mut p = project();
        p.sequences[0].frames = 7;
        write_project(&path, &p).unwrap();
        assert_eq!(read_project(&path).unwrap(), p);
        let leftovers: Vec<_> = std::fs::read_dir(&dir.0).unwrap().collect();
        assert_eq!(leftovers.len(), 1);
    }

    #[test]
    fn dots_file_round_trip() {
        let dir = TempDir::new("dots");
        let path = dir.0.join("set.inbetweendots");
        let dots = project().sequences[0].dots.clone();
        write_dots(&path, &dots).unwrap();
        assert_eq!(read_dots(&path).unwrap(), dots);
    }

    #[test]
    fn io_problems_are_errors() {
        let dir = TempDir::new("io");
        assert!(matches!(
            read_project(&dir.0.join("missing.inbetween")),
            Err(FormatError::Io(_))
        ));
        assert!(matches!(
            read_project(&dir.0),
            Err(FormatError::Io(_)) | Err(FormatError::Parse(_))
        ));
        let nested = dir.0.join("no/such/dir/x.inbetween");
        assert!(matches!(
            write_project(&nested, &project()),
            Err(FormatError::Io(_))
        ));
        assert!(matches!(
            read_dots(&dir.0.join("missing.inbetweendots")),
            Err(FormatError::Io(_))
        ));
    }

    #[test]
    fn invalid_projects_do_not_touch_the_existing_file() {
        let dir = TempDir::new("keep");
        let path = dir.0.join("movie.inbetween");
        write_project(&path, &project()).unwrap();
        let mut bad = project();
        bad.sequences[0].frames = 0;
        assert!(write_project(&path, &bad).is_err());
        assert_eq!(read_project(&path).unwrap(), project());
    }

    #[test]
    fn corrupt_files_on_disk_are_errors() {
        let dir = TempDir::new("corrupt");
        let path = dir.0.join("bad.inbetween");
        std::fs::write(&path, b"\x00\x01\x02 not a project").unwrap();
        assert!(read_project(&path).is_err());
        std::fs::write(&path, b"").unwrap();
        assert!(read_project(&path).is_err());
    }
}

#[cfg(test)]
mod device_tests {
    use super::*;

    /// Regression: on Windows `CON` is the console, and reading it blocked forever.
    #[test]
    fn devices_and_directories_are_errors_not_hangs() {
        let mut paths: Vec<&str> = vec!["."];
        if cfg!(windows) {
            paths.extend(["CON", "NUL", "AUX", "PRN", "COM1"]);
        } else {
            paths.extend(["/dev/null", "/dev/zero"]);
        }
        for p in paths {
            assert!(
                matches!(read_project(Path::new(p)), Err(FormatError::Io(_))),
                "{p}"
            );
            assert!(
                matches!(read_dots(Path::new(p)), Err(FormatError::Io(_))),
                "{p}"
            );
        }
    }
}
