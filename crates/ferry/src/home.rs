//! The local repository, `~/.ferry/`: downloaded packages, installed
//! commands, and the config with the repositories.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Deserialize;

use crate::Fail;
use crate::project::Package;

const GITHUB: &str = "https://github.com/{author}/{name}.git";

pub struct Home {
    pub dir: PathBuf,
    repositories: BTreeMap<String, String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Config {
    #[serde(default)]
    repositories: BTreeMap<String, String>,
}

impl Home {
    /// `FERRY_HOME`, or `~/.ferry`.
    pub fn open() -> Result<Home, Fail> {
        let dir = folder(|name| std::env::var_os(name), cfg!(windows))?;
        let mut repositories = BTreeMap::from([("github".to_string(), GITHUB.to_string())]);
        let config = dir.join("config.toml");
        if config.is_file() {
            let text = std::fs::read_to_string(&config).map_err(|e| format!("can't read {}: {e}", config.display()))?;
            let parsed: Config = toml::from_str(&text).map_err(|e| format!("{} is wrong: {}", config.display(), e.message()))?;
            for (name, url) in parsed.repositories {
                if !url.contains("{author}") || !url.contains("{name}") {
                    return Err(format!("{}: the URL of the repository `{name}` needs `{{author}}` and `{{name}}` in it", config.display()).into());
                }
                repositories.insert(name, url);
            }
        }
        Ok(Home { dir, repositories })
    }

    pub fn bin(&self) -> PathBuf {
        self.dir.join("bin")
    }

    pub fn installed(&self, id: &str) -> PathBuf {
        self.dir.join("installed").join(id)
    }

    fn url(&self, package: &Package) -> Result<String, Fail> {
        match self.repositories.get(&package.repository) {
            Some(url) => Ok(url.replace("{author}", &package.author).replace("{name}", &package.name)),
            None => {
                let known: Vec<&str> = self.repositories.keys().map(|k| k.as_str()).collect();
                Err(format!(
                    "there's no repository named `{}` for {package}. The ones there are: {}. Others go in {}",
                    package.repository,
                    known.join(", "),
                    self.dir.join("config.toml").display()
                )
                .into())
            }
        }
    }

    /// Checks that a repository is known, without downloading anything.
    pub fn knows(&self, package: &Package) -> Result<(), Fail> {
        self.url(package).map(|_| ())
    }

    /// The folder of a version of a package, downloaded the first time, and the commit it is.
    /// Where a version of a package is kept once it's downloaded.
    pub fn package_dir(&self, package: &Package, version: &str) -> PathBuf {
        self.dir.join("repository").join(&package.repository).join(&package.author).join(&package.name).join(version)
    }

    pub fn fetch(&self, package: &Package, version: &str) -> Result<(PathBuf, String), Fail> {
        let dir = self.package_dir(package, version);
        let marker = dir.join(".ferry-commit");
        if let Ok(commit) = std::fs::read_to_string(&marker) {
            return Ok((dir, commit.trim().to_string()));
        }
        let url = self.url(package)?;
        crate::status("Downloading", &format!("{package} {version}"));
        let parent = dir.parent().expect("the folder has parents");
        std::fs::create_dir_all(parent).map_err(|e| format!("can't create {}: {e}", parent.display()))?;
        let temporary = parent.join(format!(".{version}.download-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&temporary);
        let tag = format!("v{version}");
        let output = git(None)
            .args(["clone", "--quiet", "--depth", "1", "--branch", &tag, "-c", "advice.detachedHead=false", &url])
            .arg(&temporary)
            .output()
            .map_err(|e| format!("can't run git, which ferry needs to download packages: {e}"))?;
        if !output.status.success() {
            let _ = std::fs::remove_dir_all(&temporary);
            let said = String::from_utf8_lossy(&output.stderr);
            let said = said.trim();
            if said.contains(&tag) && said.contains("not found") {
                let versions = match versions(&url).as_slice() {
                    [] => "It has no versions yet: a version is a tag like `v1.0.0`".to_string(),
                    found => format!("Its versions are: {}", found.join(", ")),
                };
                return Err(format!("{package} has no version {version}: there's no tag {tag} in {url}. {versions}").into());
            }
            // A repository that isn't there, or that's private: git can't ask for a password here
            let missing = ["Repository not found", "could not read Username", "Authentication failed", "terminal prompts disabled", "does not appear to be a git repository"];
            if missing.iter().any(|m| said.contains(m)) {
                return Err(format!(
                    "can't download {package} {version}: there's no repository at {url}, or it's private and needs a password, which ferry can't ask for. A private one can be reached through ssh, with a repository of its own in {}",
                    self.dir.join("config.toml").display()
                )
                .into());
            }
            return Err(format!("can't download {package} {version} from {url}:\n{said}").into());
        }
        let output = git(Some(&temporary)).args(["rev-parse", "HEAD"]).output().map_err(|e| format!("can't run git: {e}"))?;
        let commit = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let _ = std::fs::remove_dir_all(temporary.join(".git"));
        std::fs::write(temporary.join(".ferry-commit"), format!("{commit}\n")).map_err(|e| format!("can't write in {}: {e}", temporary.display()))?;
        if let Err(e) = std::fs::rename(&temporary, &dir) {
            let _ = std::fs::remove_dir_all(&temporary);
            // Another ferry may have downloaded it at the same time
            if !marker.is_file() {
                return Err(format!("can't move the download to {}: {e}", dir.display()).into());
            }
        }
        Ok((dir, commit))
    }
}

/// The versions of the package at `url`, from its tags *(i.e. `v1.4.2` is
/// `1.4.2`)*, oldest first. Empty when git can't list them.
fn versions(url: &str) -> Vec<String> {
    let Ok(output) = git(None).args(["ls-remote", "--tags", "--refs", url]).output() else { return Vec::new() };
    let text = String::from_utf8_lossy(&output.stdout);
    let mut found: Vec<String> = text
        .lines()
        .filter_map(|line| line.rsplit_once("refs/tags/v").map(|(_, v)| v.to_string()))
        .filter(|v| crate::project::check_version(v).is_ok())
        .collect();
    let key = |v: &String| v.split('.').map(|p| p.parse::<u64>().unwrap_or(0)).collect::<Vec<_>>();
    found.sort_by_key(key);
    found
}

fn git(dir: Option<&Path>) -> Command {
    let mut command = Command::new("git");
    if let Some(dir) = dir {
        command.arg("-C").arg(dir);
    }
    // Never wait for a password nobody will type
    command.env("GIT_TERMINAL_PROMPT", "0");
    command
}

/// Where ferry keeps its files: `FERRY_HOME`, or `.ferry` in the person's
/// folder. On Windows that's `%USERPROFILE%`, since Windows doesn't set
/// `HOME`, and also in Git Bash, which does, so every shell finds the same
/// folder. `HOME` is the fallback there.
fn folder(var: impl Fn(&str) -> Option<std::ffi::OsString>, windows: bool) -> Result<PathBuf, Fail> {
    let given = |name: &str| var(name).filter(|v| !v.is_empty());
    if let Some(dir) = given("FERRY_HOME") {
        return Ok(PathBuf::from(dir));
    }
    let home = if windows { given("USERPROFILE").or_else(|| given("HOME")) } else { given("HOME") };
    match home {
        Some(home) => Ok(PathBuf::from(home).join(".ferry")),
        None if windows => Err("can't find the home folder: set USERPROFILE, or FERRY_HOME for where ferry keeps its files".into()),
        None => Err("can't find the home folder: set HOME, or FERRY_HOME for where ferry keeps its files".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::folder;
    use std::ffi::OsString;
    use std::path::PathBuf;

    fn with(vars: &[(&str, &str)], windows: bool) -> Result<PathBuf, String> {
        let vars: Vec<(String, OsString)> = vars.iter().map(|(k, v)| (k.to_string(), OsString::from(v))).collect();
        folder(|name| vars.iter().find(|(k, _)| k == name).map(|(_, v)| v.clone()), windows).map_err(|e| e.0)
    }

    #[test]
    fn the_folder_of_each_system() {
        assert_eq!(with(&[("HOME", "/home/ana")], false).unwrap(), PathBuf::from("/home/ana/.ferry"));
        assert_eq!(with(&[("FERRY_HOME", "/x"), ("HOME", "/home/ana")], false).unwrap(), PathBuf::from("/x"));
        // Windows: USERPROFILE, also when Git Bash sets HOME
        assert_eq!(with(&[("USERPROFILE", r"C:\Users\ana")], true).unwrap(), PathBuf::from(r"C:\Users\ana").join(".ferry"));
        assert_eq!(with(&[("USERPROFILE", r"C:\Users\ana"), ("HOME", "/c/Users/ana")], true).unwrap(), PathBuf::from(r"C:\Users\ana").join(".ferry"));
        assert_eq!(with(&[("HOME", "/c/Users/ana")], true).unwrap(), PathBuf::from("/c/Users/ana/.ferry"));
        assert!(with(&[], true).unwrap_err().contains("set USERPROFILE"));
        assert!(with(&[("HOME", "")], false).unwrap_err().contains("set HOME"));
    }
}
