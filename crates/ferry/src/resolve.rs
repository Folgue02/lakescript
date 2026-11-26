//! Finding every package a project needs, downloading what's missing, and
//! every clash between them, before anything runs.

use std::collections::{BTreeMap, VecDeque};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::Fail;
use crate::home::Home;
use crate::project::{LOCK, Package, Project, satisfies};

/// A package the project needs, directly or through another one.
pub struct Dependency {
    pub package: Package,
    /// The version chosen, always an exact one.
    pub version: String,
    /// What was asked for: the version, or a range *(i.e. `^1.4.2`)*.
    pub wanted: String,
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

    /// The version of a package in the lock file.
    fn version(&self, package: &Package) -> Option<&str> {
        let name = package.to_string();
        self.packages.iter().find(|l| l.name == name).map(|l| l.version.as_str())
    }

    /// What changed from `old` to this one, a line for each package
    /// *(i.e. `github:acme/http from 1.4.2 to 1.5.0`)*.
    pub fn changes(&self, old: &Lock) -> Vec<(String, String)> {
        let mut out = Vec::new();
        for new in &self.packages {
            match old.packages.iter().find(|l| l.name == new.name) {
                Some(was) if was.version != new.version => out.push(("Updated".to_string(), format!("{} from {} to {}", new.name, was.version, new.version))),
                Some(_) => {}
                None => out.push(("Added".to_string(), format!("{} {}", new.name, new.version))),
            }
        }
        for was in &old.packages {
            if !self.packages.iter().any(|l| l.name == was.name) {
                out.push(("Removed".to_string(), format!("{} {}", was.name, was.version)));
            }
        }
        out
    }
}

/// Which packages can get a newer version than the lock file's: none, for
/// everything but `ferry update`.
pub enum Update {
    None,
    All,
    Only(Package),
}

impl Update {
    fn covers(&self, package: &Package) -> bool {
        match self {
            Update::None => false,
            Update::All => true,
            Update::Only(p) => p == package,
        }
    }
}

/// The version of `package` for `wanted`: the exact one asked for; for a
/// range, the lock file's while it fits and isn't being updated, or else
/// the newest of the tags that fits.
fn choose(home: &Home, package: &Package, wanted: &str, lock: &Lock, update: &Update) -> Result<String, Fail> {
    if !wanted.starts_with(['^', '~']) {
        return Ok(wanted.to_string());
    }
    if !update.covers(package)
        && let Some(locked) = lock.version(package).filter(|v| satisfies(wanted, v))
    {
        return Ok(locked.to_string());
    }
    let versions = home.versions(package)?;
    match versions.iter().rev().find(|v| satisfies(wanted, v)) {
        Some(v) => Ok(v.clone()),
        None if versions.is_empty() => Err(format!("{package} has no version that's {wanted}: it has no versions yet, which are tags like `v1.0.0`").into()),
        None => Err(format!("{package} has no version that's {wanted}. Its versions are: {}", versions.join(", ")).into()),
    }
}

/// Every package `project` needs, sorted by ID, and the lock file for them.
/// The commits must be the ones `lock` has.
pub fn resolve(home: &Home, project: &Project, lock: &Lock) -> Result<(Vec<Dependency>, Lock), Fail> {
    resolve_updating(home, project, lock, &Update::None)
}

/// Like `resolve`, letting the packages of `update` move to the newest
/// version their range takes, for `ferry update`.
pub fn resolve_updating(home: &Home, project: &Project, lock: &Lock, update: &Update) -> Result<(Vec<Dependency>, Lock), Fail> {
    let mut found: BTreeMap<String, Dependency> = BTreeMap::new();
    let root = format!("the project {}", project.id);
    let mut queue: VecDeque<(Package, String, String)> =
        project.dependencies.iter().map(|(p, v)| (p.clone(), v.clone(), root.clone())).collect();
    while let Some((package, wanted, required_by)) = queue.pop_front() {
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
            // One version for everyone: the one chosen first must fit what each one asks for
            if !satisfies(&wanted, &other.version) {
                return Err(format!(
                    "{package} is needed in two versions that don't fit together: {} by {}, which got {}, and {wanted} by {required_by}",
                    other.wanted, other.required_by, other.version
                )
                .into());
            }
            continue;
        }
        let version = choose(home, &package, &wanted, lock, update)?;
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
        found.insert(id, Dependency { package, version, wanted, commit, project: dependency, required_by });
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
