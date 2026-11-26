//! Finding every package a project needs, downloading what's missing, and
//! every clash between them, before anything runs.

use std::collections::{BTreeMap, VecDeque};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::Fail;
use crate::home::Home;
use crate::project::{LOCK, Package, Project};

/// A package the project needs, directly or through another one.
pub struct Dependency {
    pub package: Package,
    pub version: String,
    pub commit: String,
    pub project: Project,
    /// Who asked for it: the project, or another package and its version.
    pub required_by: String,
}

#[derive(Default, PartialEq, Serialize, Deserialize)]
pub struct Lock {
    #[serde(default, rename = "package")]
    packages: Vec<Locked>,
}

#[derive(PartialEq, Serialize, Deserialize)]
struct Locked {
    name: String,
    version: String,
    commit: String,
}

impl Lock {
    pub fn read(dir: &Path) -> Result<Lock, Fail> {
        let file = dir.join(LOCK);
        match std::fs::read_to_string(&file) {
            Ok(text) => toml::from_str(&text).map_err(|e| format!("{} is wrong: {}", file.display(), e.message()).into()),
            Err(_) => Ok(Lock::default()),
        }
    }

    /// Writes the lock file, only when something changed.
    pub fn write(&self, dir: &Path) -> Result<(), Fail> {
        let file = dir.join(LOCK);
        let unchanged = if file.is_file() { Lock::read(dir).is_ok_and(|old| old == *self) } else { self.packages.is_empty() };
        if unchanged {
            return Ok(());
        }
        let body = toml::to_string(self).map_err(|e| format!("can't write {LOCK}: {e}"))?;
        let text = format!("# Written by ferry. Commit it, and don't edit it.\n\n{body}");
        std::fs::write(&file, text).map_err(|e| format!("can't write {}: {e}", file.display()).into())
    }

    fn commit(&self, package: &Package, version: &str) -> Option<&str> {
        let name = package.to_string();
        self.packages.iter().find(|l| l.name == name && l.version == version).map(|l| l.commit.as_str())
    }
}

/// Every package `project` needs, sorted by ID, and the lock file for them.
/// The commits must be the ones `lock` has.
pub fn resolve(home: &Home, project: &Project, lock: &Lock) -> Result<(Vec<Dependency>, Lock), Fail> {
    let mut found: BTreeMap<String, Dependency> = BTreeMap::new();
    let root = format!("the project {}", project.id);
    let mut queue: VecDeque<(Package, String, String)> =
        project.dependencies.iter().map(|(p, v)| (p.clone(), v.clone(), root.clone())).collect();
    while let Some((package, version, required_by)) = queue.pop_front() {
        let id = package.id();
        if id == project.id {
            return Err(format!("{required_by} depends on {package}, which has the same ID as the project, {id}").into());
        }
        if let Some(other) = found.get(&id) {
            if other.package != package {
                return Err(format!(
                    "two packages have the ID {id}: {} (for {}) and {package} (for {required_by})",
                    other.package, other.required_by
                )
                .into());
            }
            if other.version != version {
                return Err(format!(
                    "{package} is needed in two versions: {} by {}, and {version} by {required_by}",
                    other.version, other.required_by
                )
                .into());
            }
            continue;
        }
        let (dir, commit) = home.fetch(&package, &version)?;
        if let Some(locked) = lock.commit(&package, &version)
            && locked != commit
        {
            return Err(format!(
                "{package} {version} isn't the commit {LOCK} has: the tag v{version} was moved from {locked} to {commit}. \
                 If that's expected, delete it from {LOCK}"
            )
            .into());
        }
        let dependency = Project::read(&dir).map_err(|e| format!("{package} {version}: {e}"))?;
        if dependency.id != id {
            return Err(package.wrong_id(&version, &dependency.id).into());
        }
        dependency.modules().map_err(|e| format!("{package} {version}: {e}"))?;
        let by = format!("{package} {version}");
        for (p, v) in &dependency.dependencies {
            queue.push_back((p.clone(), v.clone(), by.clone()));
        }
        found.insert(id, Dependency { package, version, commit, project: dependency, required_by });
    }
    let packages = found
        .values()
        .map(|d| Locked { name: d.package.to_string(), version: d.version.clone(), commit: d.commit.clone() })
        .collect();
    Ok((found.into_values().collect(), Lock { packages }))
}

/// The module path of a project: its `src/`, then the `src/` of each dependency.
pub fn module_path(project: &Project, dependencies: &[Dependency]) -> Vec<PathBuf> {
    std::iter::once(project.src()).chain(dependencies.iter().map(|d| d.project.src())).collect()
}

/// The module path of a project from what's already downloaded, without
/// fetching anything: `src/` and each package of the lock file that's in the
/// local repository. For tools that only read the code *(i.e. `lks-lsp`)*.
pub fn installed_module_path(home: &Home, project: &Project) -> Vec<PathBuf> {
    let mut path = vec![project.src()];
    if let Ok(lock) = Lock::read(&project.dir) {
        for locked in &lock.packages {
            let Ok(package) = Package::parse(&locked.name) else { continue };
            let dir = home.package_dir(&package, &locked.version);
            if let Ok(dependency) = Project::read(&dir) {
                path.push(dependency.src());
            }
        }
    }
    path
}
