use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use dotify_core::Editor;

use crate::error::{ToolError, ToolResult};

/// Projects open at the same time, to bound memory.
pub const MAX_OPEN_PROJECTS: usize = 32;

pub struct OpenProject {
    pub editor: Editor,
    /// The project's `.dotify` file, once it has one.
    pub path: Option<PathBuf>,
}

/// The projects this server has open, addressed by short handles
/// (`p1`, `p2`, …) that stay valid until the project is closed.
#[derive(Default)]
pub struct Workspace {
    projects: BTreeMap<String, OpenProject>,
    next: u32,
}

impl Workspace {
    pub fn insert(&mut self, project: OpenProject) -> ToolResult<String> {
        if self.projects.len() >= MAX_OPEN_PROJECTS {
            return Err(ToolError::new(
                "too_many_projects",
                format!("at most {MAX_OPEN_PROJECTS} projects can be open; close one first"),
            ));
        }
        self.next += 1;
        let id = format!("p{}", self.next);
        self.projects.insert(id.clone(), project);
        Ok(id)
    }

    pub fn get(&self, id: &str) -> ToolResult<&OpenProject> {
        self.projects.get(id).ok_or_else(|| not_found(id))
    }

    pub fn get_mut(&mut self, id: &str) -> ToolResult<&mut OpenProject> {
        self.projects.get_mut(id).ok_or_else(|| not_found(id))
    }

    pub fn remove(&mut self, id: &str) -> ToolResult<OpenProject> {
        self.projects.remove(id).ok_or_else(|| not_found(id))
    }

    /// The handle of the open project whose file is `path`.
    pub fn find_path(&self, path: &Path) -> Option<&str> {
        self.projects.iter().find(|(_, p)| p.path.as_deref() == Some(path)).map(|(id, _)| id.as_str())
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, &OpenProject)> {
        self.projects.iter().map(|(id, p)| (id.as_str(), p))
    }
}

fn not_found(id: &str) -> ToolError {
    ToolError::new("project_not_found", format!("no open project {id:?}; use project_open or project_list"))
}
