//! A scratch directory that is removed when the test passes and kept,
//! with its path printed, when the test fails: a failing input stays on
//! disk for the person who has to look at it.

use std::path::{Path, PathBuf};

pub struct TempDir(PathBuf);

impl TempDir {
    /// A fresh, empty directory named after the process and `label`, so
    /// two tests of one run never share one.
    pub fn new(label: &str) -> Self {
        let path =
            std::env::temp_dir().join(format!("fibc-bootstrap-{}-{label}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("a scratch directory can be created");
        TempDir(path)
    }

    pub fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        if std::thread::panicking() {
            eprintln!("the scratch files are kept in {}", self.0.display());
        } else {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_directory_is_removed_on_drop_and_distinct_labels_do_not_collide() {
        let (a, b) = (TempDir::new("tmp-a"), TempDir::new("tmp-b"));
        assert_ne!(a.path(), b.path());
        std::fs::write(a.path().join("x"), "x").expect("writable");
        let kept = a.path().to_path_buf();
        drop(a);
        assert!(!kept.exists());
        assert!(b.path().exists());
    }
}
