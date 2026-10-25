//! File access limited to configured root directories.
//!
//! Every path a tool receives goes through [`Sandbox`]: it is resolved
//! against the first root when relative, symlinks are resolved with
//! `canonicalize`, and anything that lands outside every root is rejected.

use std::io;
use std::path::{Component, Path, PathBuf};

use crate::error::{ToolError, ToolResult};

#[derive(Clone, Debug)]
pub struct Sandbox {
    roots: Vec<PathBuf>,
}

fn outside(path: &Path) -> ToolError {
    ToolError::new("path_outside_roots", format!("{} is outside the allowed root directories", path.display()))
}

impl Sandbox {
    /// Canonicalize the roots. Each must be an existing directory and there
    /// must be at least one.
    pub fn new(roots: impl IntoIterator<Item = PathBuf>) -> io::Result<Self> {
        let mut canonical = Vec::new();
        for root in roots {
            let c =
                root.canonicalize().map_err(|e| io::Error::new(e.kind(), format!("root {}: {e}", root.display())))?;
            if !c.is_dir() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("root {} is not a directory", c.display()),
                ));
            }
            if !canonical.contains(&c) {
                canonical.push(c);
            }
        }
        if canonical.is_empty() {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "at least one root directory is required"));
        }
        Ok(Self { roots: canonical })
    }

    pub fn roots(&self) -> &[PathBuf] {
        &self.roots
    }

    /// True when the canonical `path` is a root or lies inside one.
    pub fn contains(&self, path: &Path) -> bool {
        self.roots.iter().any(|r| path.starts_with(r))
    }

    /// The path as given, made absolute against the first root.
    fn join(&self, input: &str) -> ToolResult<PathBuf> {
        if input.trim().is_empty() {
            return Err(ToolError::invalid("path must not be empty"));
        }
        if input.contains('\0') {
            return Err(ToolError::invalid("path must not contain NUL characters"));
        }
        let p = Path::new(input);
        Ok(if p.is_absolute() { p.to_path_buf() } else { self.roots[0].join(p) })
    }

    /// An existing file inside the roots with one of the `extensions`.
    pub fn existing_file(&self, input: &str, extensions: &[&str]) -> ToolResult<PathBuf> {
        let path = self.join(input)?;
        let canonical = path.canonicalize().map_err(|e| match e.kind() {
            io::ErrorKind::NotFound => ToolError::new("not_found", format!("{} does not exist", path.display())),
            _ => e.into(),
        })?;
        if !self.contains(&canonical) {
            return Err(outside(&path));
        }
        if !canonical.is_file() {
            return Err(ToolError::invalid(format!("{} is not a file", path.display())));
        }
        check_extension(&canonical, extensions)?;
        Ok(canonical)
    }

    /// A file inside the roots that may be written. Missing parent
    /// directories are created (inside the roots only). The file itself may
    /// or may not exist; callers decide whether replacing it needs
    /// confirmation.
    pub fn writable_file(&self, input: &str, extensions: &[&str]) -> ToolResult<PathBuf> {
        let path = self.join(input)?;
        check_extension(&path, extensions)?;
        let name = match path.components().next_back() {
            Some(Component::Normal(n)) => n.to_os_string(),
            _ => return Err(ToolError::invalid(format!("{} is not a file path", path.display()))),
        };
        let parent = path.parent().ok_or_else(|| outside(&path))?;
        let target = self.dir(parent, true)?.join(name);
        if let Ok(meta) = std::fs::symlink_metadata(&target) {
            if meta.is_dir() {
                return Err(ToolError::invalid(format!("{} is a directory", target.display())));
            }
            // Writes replace the entry itself, but a link that leads outside
            // the roots is refused anyway so nothing outside appears touched.
            if meta.file_type().is_symlink() {
                match target.canonicalize() {
                    Ok(c) if self.contains(&c) => {}
                    _ => return Err(outside(&target)),
                }
            }
        }
        Ok(target)
    }

    /// A directory inside the roots, created when missing.
    pub fn writable_dir(&self, input: &str) -> ToolResult<PathBuf> {
        let path = self.join(input)?;
        self.dir(&path, true)
    }

    /// Resolve `path` as a directory: canonicalize its nearest existing
    /// ancestor, require that to be inside the roots and the rest to be
    /// plain names, then optionally create the rest.
    fn dir(&self, path: &Path, create: bool) -> ToolResult<PathBuf> {
        let mut existing = path;
        let mut rest = Vec::new();
        while !existing.exists() {
            match (existing.file_name(), existing.parent()) {
                (Some(name), Some(parent)) => {
                    rest.push(name.to_os_string());
                    existing = parent;
                }
                // `..`, `.` or the filesystem root: give up.
                _ => return Err(outside(path)),
            }
        }
        // `file_name()` skips a trailing `..`, so check the remainder lexically.
        let remainder = path.strip_prefix(existing).map_err(|_| outside(path))?;
        if remainder.components().any(|c| !matches!(c, Component::Normal(_))) {
            return Err(outside(path));
        }
        let base = existing.canonicalize()?;
        if !self.contains(&base) {
            return Err(outside(path));
        }
        if !base.is_dir() {
            return Err(ToolError::invalid(format!("{} is not a directory", existing.display())));
        }
        let mut dir = base;
        for name in rest.into_iter().rev() {
            dir.push(name);
        }
        if create && !dir.exists() {
            std::fs::create_dir_all(&dir)?;
        }
        if dir.exists() {
            let c = dir.canonicalize()?;
            if !self.contains(&c) {
                return Err(outside(path));
            }
            return Ok(c);
        }
        Err(ToolError::new("not_found", format!("{} does not exist", path.display())))
    }

    /// `.dotify` files under the roots, as `(root, path)`, depth-limited.
    pub fn find_projects(&self, max_depth: usize, limit: usize) -> Vec<PathBuf> {
        let mut out = Vec::new();
        for root in &self.roots {
            walk(root, max_depth, limit, self, &mut out);
        }
        out.sort();
        out.dedup();
        out
    }
}

fn walk(dir: &Path, depth: usize, limit: usize, sandbox: &Sandbox, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    let mut entries: Vec<_> = entries.flatten().collect();
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        if out.len() >= limit {
            return;
        }
        let name = entry.file_name();
        if name.to_string_lossy().starts_with('.') {
            continue;
        }
        let Ok(kind) = entry.file_type() else { continue };
        let path = entry.path();
        if kind.is_dir() && depth > 0 {
            walk(&path, depth - 1, limit, sandbox, out);
        } else if kind.is_file() && has_extension(&path, &["dotify"]) {
            out.push(path);
        } else if kind.is_symlink() {
            // Only list links whose target stays inside the roots.
            if let Ok(c) = path.canonicalize() {
                if sandbox.contains(&c) && c.is_file() && has_extension(&c, &["dotify"]) {
                    out.push(path);
                }
            }
        }
    }
}

fn has_extension(path: &Path, extensions: &[&str]) -> bool {
    path.extension().and_then(|e| e.to_str()).is_some_and(|e| extensions.iter().any(|x| x.eq_ignore_ascii_case(e)))
}

fn check_extension(path: &Path, extensions: &[&str]) -> ToolResult<()> {
    if extensions.is_empty() || has_extension(path, extensions) {
        return Ok(());
    }
    let list = extensions.iter().map(|e| format!(".{e}")).collect::<Vec<_>>().join(", ");
    Err(ToolError::new("invalid_path", format!("{} must end in {list}", path.display())))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> (tempfile::TempDir, tempfile::TempDir, Sandbox) {
        let root = tempfile::tempdir().unwrap();
        let other = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("a.dotify"), b"x").unwrap();
        std::fs::write(other.path().join("secret.dotify"), b"x").unwrap();
        let sandbox = Sandbox::new([root.path().to_path_buf()]).unwrap();
        (root, other, sandbox)
    }

    #[test]
    fn requires_a_root() {
        assert!(Sandbox::new(Vec::<PathBuf>::new()).is_err());
        assert!(Sandbox::new([PathBuf::from("/definitely/not/here")]).is_err());
    }

    #[test]
    fn resolves_relative_paths_inside_the_first_root() {
        let (root, _other, sb) = setup();
        let p = sb.existing_file("a.dotify", &["dotify"]).unwrap();
        assert_eq!(p, root.path().canonicalize().unwrap().join("a.dotify"));
    }

    #[test]
    fn rejects_absolute_paths_outside() {
        let (_root, other, sb) = setup();
        let p = other.path().join("secret.dotify");
        let err = sb.existing_file(p.to_str().unwrap(), &["dotify"]).unwrap_err();
        assert_eq!(err.code, "path_outside_roots");
        let err = sb.writable_file(p.to_str().unwrap(), &["dotify"]).unwrap_err();
        assert_eq!(err.code, "path_outside_roots");
    }

    #[test]
    fn rejects_parent_traversal() {
        let (_root, other, sb) = setup();
        let name = other.path().file_name().unwrap().to_str().unwrap();
        let rel = format!("../{name}/secret.dotify");
        assert_eq!(sb.existing_file(&rel, &["dotify"]).unwrap_err().code, "path_outside_roots");
        assert_eq!(sb.writable_file(&rel, &["dotify"]).unwrap_err().code, "path_outside_roots");
        // Through a directory that does not exist yet.
        let rel = format!("new/../../{name}/x.png");
        assert_eq!(sb.writable_file(&rel, &["png"]).unwrap_err().code, "path_outside_roots");
        assert_eq!(sb.writable_dir("new/../..").unwrap_err().code, "path_outside_roots");
    }

    #[test]
    fn rejects_symlinks_leading_outside() {
        let (root, other, sb) = setup();
        std::os::unix::fs::symlink(other.path(), root.path().join("link")).unwrap();
        std::os::unix::fs::symlink(other.path().join("secret.dotify"), root.path().join("s.dotify")).unwrap();
        assert_eq!(sb.existing_file("link/secret.dotify", &["dotify"]).unwrap_err().code, "path_outside_roots");
        assert_eq!(sb.existing_file("s.dotify", &["dotify"]).unwrap_err().code, "path_outside_roots");
        assert_eq!(sb.writable_file("link/new.dotify", &["dotify"]).unwrap_err().code, "path_outside_roots");
        assert_eq!(sb.writable_file("s.dotify", &["dotify"]).unwrap_err().code, "path_outside_roots");
        assert_eq!(sb.writable_dir("link/sub").unwrap_err().code, "path_outside_roots");
        assert!(!other.path().join("sub").exists());
        assert_eq!(sb.find_projects(4, 100), vec![root.path().canonicalize().unwrap().join("a.dotify")]);
    }

    #[test]
    fn creates_missing_directories_inside() {
        let (root, _other, sb) = setup();
        let p = sb.writable_file("out/sprites/hero.png", &["png"]).unwrap();
        assert!(root.path().join("out/sprites").is_dir());
        assert!(p.ends_with("out/sprites/hero.png"));
    }

    #[test]
    fn checks_extensions() {
        let (_root, _other, sb) = setup();
        assert_eq!(sb.writable_file("notes.txt", &["dotify"]).unwrap_err().code, "invalid_path");
        assert!(sb.writable_file("UP.DOTIFY", &["dotify"]).is_ok());
        assert_eq!(sb.existing_file("missing.dotify", &["dotify"]).unwrap_err().code, "not_found");
    }
}
