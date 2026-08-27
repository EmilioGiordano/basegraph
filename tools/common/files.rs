//! Small filesystem helpers shared by the tools.

#![allow(dead_code)]

use std::path::Path;

use anyhow::{Context, Result};

/// Copy `src` over `dst` through read + write, so `dst` gets a fresh mtime.
/// `std::fs::copy` preserves the source timestamp on Windows, which makes
/// cargo consider a just-replaced source file older than its last build and
/// skip recompiling it.
pub fn copy_fresh(src: &Path, dst: &Path) -> Result<()> {
    let bytes = std::fs::read(src).with_context(|| format!("reading {}", src.display()))?;
    if let Some(parent) = dst.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(dst, bytes).with_context(|| format!("writing {}", dst.display()))?;
    Ok(())
}

/// Recursively copy a directory tree (files only, no symlink handling).
pub fn copy_tree(src: &Path, dst: &Path) -> Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src).with_context(|| format!("reading {}", src.display()))? {
        let entry = entry?;
        let target = dst.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_tree(&entry.path(), &target)?;
        } else {
            copy_fresh(&entry.path(), &target)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, SystemTime};

    #[test]
    fn copy_fresh_resets_the_timestamp() {
        let dir = std::env::temp_dir().join(format!("cg_files_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp dir");
        let src = dir.join("src.txt");
        std::fs::write(&src, "x").expect("write");
        let old = SystemTime::now() - Duration::from_secs(3600);
        let file = std::fs::File::options()
            .write(true)
            .open(&src)
            .expect("open");
        file.set_modified(old).expect("set mtime");
        drop(file);

        let dst = dir.join("nested").join("dst.txt");
        copy_fresh(&src, &dst).expect("copy");
        let mtime = std::fs::metadata(&dst)
            .expect("meta")
            .modified()
            .expect("mtime");
        assert!(
            mtime > old + Duration::from_secs(1800),
            "mtime should be fresh"
        );
        assert_eq!(std::fs::read_to_string(&dst).unwrap(), "x");

        let tree = dir.join("tree");
        copy_tree(&dir.join("nested"), &tree).expect("copy tree");
        assert!(tree.join("dst.txt").is_file());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
