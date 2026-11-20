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
        let dir = match std::env::var_os("FERRY_HOME") {
            Some(dir) if !dir.is_empty() => PathBuf::from(dir),
            _ => match std::env::var_os("HOME") {
                Some(home) => PathBuf::from(home).join(".ferry"),
                None => return Err("can't find the home folder: set HOME, or FERRY_HOME for where ferry keeps its files".into()),
            },
        };
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
    pub fn fetch(&self, package: &Package, version: &str) -> Result<(PathBuf, String), Fail> {
        let dir = self.dir.join("repository").join(&package.repository).join(&package.author).join(&package.name).join(version);
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
                return Err(format!("{package} has no version {version}: there's no tag {tag} in {url}").into());
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

fn git(dir: Option<&Path>) -> Command {
    let mut command = Command::new("git");
    if let Some(dir) = dir {
        command.arg("-C").arg(dir);
    }
    // Never wait for a password nobody will type
    command.env("GIT_TERMINAL_PROMPT", "0");
    command
}
