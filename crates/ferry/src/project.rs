//! What a project is: its `ferry.toml`, its ID, and the packages it names.

use std::collections::BTreeMap;
use std::fmt::{self, Write as _};
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
    #[serde(default)]
    pub check: Checks,
}

/// What `ferry check` also looks at, when the project asks for it.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Checks {
    /// A warning for each `pub` item without a `///` comment.
    #[serde(default)]
    pub docs: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Info {
    pub author: String,
    pub name: String,
    pub version: String,
    pub kind: Kind,
    /// The oldest Lakescript the project needs *(i.e. `0.13.0`)*. An older
    /// `ferry` warns.
    #[serde(default)]
    pub lks: Option<String>,
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
        if let Some(lks) = &manifest.project.lks {
            check_version(lks).map_err(|e| format!("{}: the version of Lakescript it needs, `lks`, {e}", file.display()))?;
        }
        let mut dependencies = Vec::new();
        for (key, version) in &manifest.dependencies {
            let package = Package::parse(key).map_err(|e| format!("{}: {e}", file.display()))?;
            check_requirement(version).map_err(|e| format!("{}: the version of {package} {e}", file.display()))?;
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
        self.scripts_in("bin")
    }

    /// The project's own scripts, in `scripts/`, which `ferry task` runs.
    pub fn tasks(&self) -> Vec<String> {
        self.scripts_in("scripts")
    }

    pub fn task_file(&self, task: &str) -> PathBuf {
        self.dir.join("scripts").join(format!("{task}.lks"))
    }

    fn scripts_in(&self, folder: &str) -> Vec<String> {
        let mut commands: Vec<String> = std::fs::read_dir(self.dir.join(folder))
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_file() && p.extension().is_some_and(|e| e == "lks"))
            .filter(|p| !p.file_name().is_some_and(|n| n.to_string_lossy().starts_with('.')))
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

/// The files under a folder, without the hidden ones *(i.e. `.DS_Store`, an
/// editor's `.list.lks.swp`)*, which aren't modules, tests or commands.
fn walk(dir: &Path, files: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        if entry.file_name().to_string_lossy().starts_with('.') {
            continue;
        }
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

    /// The error for a package whose project has another ID than it was
    /// asked for, with how to write it when only the case is different
    /// *(i.e. `github:Folgue02/text` for `folgue02/text`)*.
    pub fn wrong_id(&self, version: &str, found: &str) -> String {
        let id = self.id();
        let mut text = format!("{self} {version} says it's {found}, not {id}");
        if found.eq_ignore_ascii_case(&id)
            && let Some((author, name)) = found.split_once('/')
        {
            let _ = write!(text, ". IDs are written as the project's ferry.toml has them, so it's `{}:{author}/{name}`", self.repository);
        }
        text
    }
}

impl fmt::Display for Package {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}:{}/{}", self.repository, self.author, self.name)
    }
}

/// `repository:author/name@version` with an exact version, as `ferry install`
/// and `ferry doc` take it.
pub fn parse_exact_spec(text: &str) -> Result<(Package, String), String> {
    let (package, version) = parse_spec(text)?;
    check_version(&version).map_err(|e| format!("the version of {package} {e}"))?;
    Ok((package, version))
}

/// `repository:author/name@version`, as `ferry add` takes it: the version can be a range.
pub fn parse_spec(text: &str) -> Result<(Package, String), String> {
    // The package first, so the example of a missing version is a right one
    let (package, version) = match text.rsplit_once('@') {
        Some((package, version)) => (package, Some(version)),
        None => (text, None),
    };
    let package = Package::parse(package)?;
    let Some(version) = version else {
        return Err(format!("`{package}` needs a version, i.e. `{package}@1.0.0`"));
    };
    check_requirement(version).map_err(|e| format!("the version of {package} {e}"))?;
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
    match numbers(version) {
        Some(_) => Ok(()),
        None => Err(format!("`{version}` must be an exact version, like `1.4.2`")),
    }
}

/// The three numbers of a version *(i.e. `1.4.2`)*, without leading zeros.
pub fn numbers(version: &str) -> Option<(u64, u64, u64)> {
    let parts: Vec<&str> = version.split('.').collect();
    let number = |p: &&str| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()) && (p.len() == 1 || !p.starts_with('0'));
    if parts.len() != 3 || !parts.iter().all(number) {
        return None;
    }
    Some((parts[0].parse().ok()?, parts[1].parse().ok()?, parts[2].parse().ok()?))
}

/// What a dependency asks for: an exact version *(i.e. `1.4.2`)*, or a
/// range like Cargo's: `^1.4.2` is that or newer below `2.0.0` *(below
/// `0.5.0` for `^0.4.2`)*, and `~1.4.2` below `1.5.0`.
pub fn check_requirement(requirement: &str) -> Result<(), String> {
    let exact = requirement.trim_start_matches(['^', '~']);
    if requirement.len() - exact.len() <= 1 && numbers(exact).is_some() {
        Ok(())
    } else {
        Err(format!("`{requirement}` isn't a version: it's an exact one like `1.4.2`, or a range like `^1.4.2` or `~1.4.2`"))
    }
}

/// Whether `version` is one that `requirement` takes.
pub fn satisfies(requirement: &str, version: &str) -> bool {
    let (Some(v), Some(low)) = (numbers(version), numbers(requirement.trim_start_matches(['^', '~']))) else { return false };
    let high = match requirement.chars().next() {
        Some('^') if low.0 > 0 => (low.0 + 1, 0, 0),
        Some('^') if low.1 > 0 => (0, low.1 + 1, 0),
        Some('^') => (0, 0, low.2 + 1),
        Some('~') => (low.0, low.1 + 1, 0),
        _ => return v == low,
    };
    v >= low && v < high
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranges() {
        let fits = |r: &str, v: &str| satisfies(r, v);
        assert!(fits("1.4.2", "1.4.2") && !fits("1.4.2", "1.4.3"));
        assert!(fits("^1.4.2", "1.4.2") && fits("^1.4.2", "1.9.0") && !fits("^1.4.2", "2.0.0") && !fits("^1.4.2", "1.4.1"));
        // Below 1.0.0, the first number that isn't 0 is the one that can't change
        assert!(fits("^0.4.2", "0.4.9") && !fits("^0.4.2", "0.5.0"));
        assert!(fits("^0.0.3", "0.0.3") && !fits("^0.0.3", "0.0.4"));
        assert!(fits("~1.4.2", "1.4.9") && !fits("~1.4.2", "1.5.0") && !fits("~1.4.2", "1.4.1"));
        assert!(fits("~0.4.2", "0.4.5") && !fits("~0.4.2", "0.5.0"));
        assert!(!fits("^1.0.0", "1.0") && !fits("^1.0", "1.0.0"));
        for good in ["1.4.2", "^1.4.2", "~0.0.1"] {
            assert!(check_requirement(good).is_ok(), "{good}");
        }
        for bad in ["1.4", "^^1.4.2", "^~1.4.2", ">=1.0.0", "01.0.0", "*", ""] {
            assert!(check_requirement(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn an_id_in_another_case_says_how_its_written() {
        let package = Package::parse("github:Folgue02/text").unwrap();
        assert_eq!(
            package.wrong_id("0.1.0", "folgue02/text"),
            "github:Folgue02/text 0.1.0 says it's folgue02/text, not Folgue02/text. IDs are written as the project's ferry.toml has them, so it's `github:folgue02/text`"
        );
        assert_eq!(package.wrong_id("0.1.0", "acme/other"), "github:Folgue02/text 0.1.0 says it's acme/other, not Folgue02/text");
    }
}
