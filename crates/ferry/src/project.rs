//! What a project is: its `ferry.toml`, its ID, and the packages it names.

use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::Fail;

pub const MANIFEST: &str = "ferry.toml";
pub const LOCK: &str = "ferry.lock";

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub project: Info,
    #[serde(default)]
    pub dependencies: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Info {
    pub author: String,
    pub name: String,
    pub version: String,
    pub kind: Kind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Library,
    Application,
}

/// A project read from its folder.
pub struct Project {
    pub dir: PathBuf,
    pub manifest: Manifest,
    /// The ID modules are written with *(i.e. `some_user/my_tool`)*.
    pub id: String,
    pub dependencies: Vec<(Package, String)>,
}

impl Project {
    pub fn read(dir: &Path) -> Result<Project, Fail> {
        let file = dir.join(MANIFEST);
        let text = std::fs::read_to_string(&file).map_err(|e| format!("can't read {}: {e}", file.display()))?;
        let manifest: Manifest = toml::from_str(&text).map_err(|e| format!("{} is wrong: {}", file.display(), e.message()))?;
        let id = id_of(&manifest.project.author, &manifest.project.name).map_err(|e| format!("{}: {e}", file.display()))?;
        check_version(&manifest.project.version).map_err(|e| format!("{}: {e}", file.display()))?;
        let mut dependencies = Vec::new();
        for (key, version) in &manifest.dependencies {
            let package = Package::parse(key).map_err(|e| format!("{}: {e}", file.display()))?;
            check_version(version).map_err(|e| format!("{}: the version of {package} {e}", file.display()))?;
            dependencies.push((package, version.clone()));
        }
        Ok(Project { dir: dir.to_path_buf(), manifest, id, dependencies })
    }

    /// The nearest folder, from `start` up, with a `ferry.toml`.
    pub fn find(start: &Path) -> Result<Project, Fail> {
        let mut dir = Some(start);
        while let Some(d) = dir {
            if d.join(MANIFEST).is_file() {
                return Project::read(d);
            }
            dir = d.parent();
        }
        Err(format!("there's no {MANIFEST} in {} or the folders above it. `ferry new` creates a project", start.display()).into())
    }

    pub fn src(&self) -> PathBuf {
        self.dir.join("src")
    }

    /// The commands in `bin/`, sorted.
    pub fn commands(&self) -> Vec<String> {
        let mut commands: Vec<String> = std::fs::read_dir(self.dir.join("bin"))
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_file() && p.extension().is_some_and(|e| e == "lks"))
            .filter_map(|p| p.file_stem().map(|s| s.to_string_lossy().into_owned()))
            .collect();
        commands.sort();
        commands
    }

    /// The files of tests in `tests/` and its folders, sorted *(see `ideas/tooling/testing.md`)*.
    pub fn test_files(&self) -> Vec<PathBuf> {
        let mut files = Vec::new();
        walk(&self.dir.join("tests"), &mut files);
        files.retain(|f| f.extension().is_some_and(|e| e == "lks"));
        files.sort();
        files
    }

    pub fn command_file(&self, command: &str) -> PathBuf {
        self.dir.join("bin").join(format!("{command}.lks"))
    }

    /// Every module of the project, as `use` paths. Files in `src/` outside
    /// `src/<ID>/` are an error, so two packages never provide the same module.
    pub fn modules(&self) -> Result<Vec<String>, Fail> {
        let src = self.src();
        let mut files = Vec::new();
        walk(&src, &mut files);
        let own = src.join(&self.id);
        let mut modules = Vec::new();
        for file in files {
            let relative = file.strip_prefix(&src).unwrap_or(&file);
            if !file.starts_with(&own) {
                return Err(format!(
                    "{} is outside src/{}/. Every module of {} must be in that folder",
                    Path::new("src").join(relative).display(),
                    self.id,
                    self.id
                )
                .into());
            }
            if file.extension().is_some_and(|e| e == "lks") {
                let path = relative.with_extension("");
                modules.push(path.components().map(|c| c.as_os_str().to_string_lossy()).collect::<Vec<_>>().join("/"));
            }
        }
        modules.sort();
        Ok(modules)
    }
}

fn walk(dir: &Path, files: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk(&path, files);
        } else {
            files.push(path);
        }
    }
}

/// A package in a repository, without its version *(i.e. `github:acme/http`)*.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Package {
    pub repository: String,
    pub author: String,
    pub name: String,
}

impl Package {
    pub fn parse(text: &str) -> Result<Package, String> {
        let wrong = || format!("`{text}` isn't a package. It's written `repository:author/name` (i.e. `github:acme/http`)");
        let (repository, rest) = text.split_once(':').ok_or_else(wrong)?;
        let (author, name) = rest.split_once('/').ok_or_else(wrong)?;
        if repository.is_empty() || !repository.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') {
            return Err(wrong());
        }
        id_of(author, name)?;
        Ok(Package { repository: repository.into(), author: author.into(), name: name.into() })
    }

    pub fn id(&self) -> String {
        id_of(&self.author, &self.name).expect("checked when parsed")
    }
}

impl fmt::Display for Package {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}:{}/{}", self.repository, self.author, self.name)
    }
}

/// `repository:author/name@version`, as `ferry add` and `ferry install` take it.
pub fn parse_spec(text: &str) -> Result<(Package, String), String> {
    let Some((package, version)) = text.rsplit_once('@') else {
        return Err(format!("`{text}` needs a version, i.e. `{text}@1.0.0`"));
    };
    let package = Package::parse(package)?;
    check_version(version).map_err(|e| format!("the version of {package} {e}"))?;
    Ok((package, version.to_string()))
}

/// The ID of a project: `-` and `.` become `_`, and the rest must be valid in a name.
pub fn id_of(author: &str, name: &str) -> Result<String, String> {
    let part = |what: &str, text: &str| -> Result<String, String> {
        let id: String = text.chars().map(|c| if c == '-' || c == '.' { '_' } else { c }).collect();
        let valid = !id.is_empty()
            && !id.starts_with(|c: char| c.is_ascii_digit())
            && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
        if !valid {
            return Err(format!(
                "`{text}` can't be the {what} of a project: it can only have ASCII letters, digits, `_`, `-` and `.`, and can't start with a digit"
            ));
        }
        Ok(id)
    };
    let author = part("author", author)?;
    if author == "std" {
        return Err("the author of a project can't be `std`, since `std/` is the standard library".into());
    }
    Ok(format!("{author}/{}", part("name", name)?))
}

/// Versions are exact for now: `MAJOR.MINOR.PATCH`.
pub fn check_version(version: &str) -> Result<(), String> {
    let parts: Vec<&str> = version.split('.').collect();
    let number = |p: &&str| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()) && (p.len() == 1 || !p.starts_with('0'));
    if parts.len() == 3 && parts.iter().all(number) {
        Ok(())
    } else {
        Err(format!("`{version}` must be an exact version, like `1.4.2`. Ranges aren't supported yet"))
    }
}
