//! Files: a temporary directory that removes itself, a copy of a tree, and
//! the paths of a run made relative to the directory it starts in.

use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// A directory under the system's temporary directory, removed when dropped.
pub struct TempDir(PathBuf);

impl TempDir {
    /// A new empty directory whose name starts with `tag`.
    pub fn new(tag: &str) -> io::Result<TempDir> {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        for attempt in 0u32.. {
            let name = format!("{tag}-{}-{nanos}-{attempt}", std::process::id());
            let path = std::env::temp_dir().join(name);
            match fs::create_dir(&path) {
                Ok(()) => return Ok(TempDir(path)),
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(e),
            }
        }
        unreachable!("the attempts are unbounded")
    }

    pub fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        // Nothing to do about a failure here: the directory is only scratch.
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Copies the files of `from` under `to`, creating directories. A link to a
/// file is copied as that file; a link to a directory is not followed.
pub fn copy_tree(from: &Path, to: &Path) -> io::Result<()> {
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        let kind = entry.file_type()?;
        if kind.is_dir() {
            copy_tree(&entry.path(), &target)?;
        } else if kind.is_file() || fs::metadata(entry.path())?.is_file() {
            fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}

/// `path` as a path below `base` with no `..` and no root: the form in
/// which a run lays its files out in the working copy, so that the
/// relative `roots` of a case header find the same files there.
pub fn below(path: &Path, base: &Path) -> Result<PathBuf, String> {
    let rest = if path.is_absolute() {
        path.strip_prefix(base)
            .map_err(|_| format!("{} is not below {}", path.display(), base.display()))?
    } else {
        path
    };
    let mut out = PathBuf::new();
    for part in rest.components() {
        match part {
            Component::Normal(p) => out.push(p),
            Component::CurDir => {}
            _ => {
                return Err(format!(
                    "{}: a path must stay below the current directory",
                    path.display()
                ))
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_temp_dir_is_unique_and_removes_itself() {
        let (a, b) = (
            TempDir::new("fibmut-t").unwrap(),
            TempDir::new("fibmut-t").unwrap(),
        );
        assert_ne!(a.path(), b.path());
        let gone = a.path().to_path_buf();
        fs::write(gone.join("f"), "x").unwrap();
        drop(a);
        assert!(!gone.exists());
        assert!(b.path().is_dir());
    }

    #[test]
    fn a_copy_has_the_files_and_the_subdirectories() {
        let src = TempDir::new("fibmut-t").unwrap();
        fs::create_dir_all(src.path().join("a/b")).unwrap();
        fs::write(src.path().join("a/b/f.fib"), "one").unwrap();
        fs::write(src.path().join("g.fib"), "two").unwrap();
        let dst = TempDir::new("fibmut-t").unwrap();
        copy_tree(src.path(), &dst.path().join("copy")).unwrap();
        assert_eq!(
            fs::read_to_string(dst.path().join("copy/a/b/f.fib")).unwrap(),
            "one"
        );
        assert_eq!(
            fs::read_to_string(dst.path().join("copy/g.fib")).unwrap(),
            "two"
        );
        assert!(copy_tree(&src.path().join("missing"), &dst.path().join("x")).is_err());
    }

    #[test]
    fn paths_are_taken_below_the_base() {
        let base = Path::new("/w");
        assert_eq!(
            below(Path::new("lib/x.fib"), base).unwrap(),
            PathBuf::from("lib/x.fib")
        );
        assert_eq!(
            below(Path::new("./lib"), base).unwrap(),
            PathBuf::from("lib")
        );
        assert_eq!(
            below(Path::new("/w/lib"), base).unwrap(),
            PathBuf::from("lib")
        );
        assert!(below(Path::new("/other/lib"), base).is_err());
        assert!(below(Path::new("../lib"), base).is_err());
        assert!(below(Path::new("a/../../lib"), base).is_err());
    }
}
