//! Everything a running script can reach outside itself: output, input, the
//! current folder, environment variables, modules. The engine owns it, so
//! two engines never share anything, and a script never changes the process
//! of the program that embeds it.

use std::collections::{BTreeMap, HashMap};
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use crate::modules::Loader;
use crate::scope::Env;
use crate::value::RegexValue;

pub struct Host {
    pub output: Box<dyn Write>,
    pub error: Box<dyn Write>,
    pub input: Box<dyn BufRead>,
    /// `true` when stdin is a pipe or a file, not the terminal.
    pub input_piped: bool,
    /// `true` when output goes to the real stdout and stderr, so commands can
    /// write to them directly. Otherwise their output is copied to `output`.
    pub inherit_stdio: bool,
    pub exit_allowed: bool,
    pub regexes: HashMap<String, Rc<RegexValue>>,
    pub state: ScriptState,
    pub loader: Loader,
    pub builtins: Env,
}

/// The part of the outside world a script can change. Changes stay in here:
/// the real process's folder and environment are never touched.
pub struct ScriptState {
    pub args: Vec<String>,
    pub script_path: Option<PathBuf>,
    pub cwd: PathBuf,
    /// Variables set (`Some`) or removed (`None`) by the script, over the real environment.
    pub env: HashMap<String, Option<String>>,
}

impl ScriptState {
    pub fn new() -> Self {
        ScriptState {
            args: Vec::new(),
            script_path: None,
            cwd: std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/")),
            env: HashMap::new(),
        }
    }

    /// A path from a script, relative to the script's current folder.
    pub fn resolve(&self, path: &str) -> PathBuf {
        let p = Path::new(path);
        if p.is_absolute() { p.to_path_buf() } else { self.cwd.join(p) }
    }

    pub fn env_get(&self, name: &str) -> Option<String> {
        match self.env.get(name) {
            Some(value) => value.clone(),
            None => std::env::var(name).ok(),
        }
    }

    pub fn env_all(&self) -> BTreeMap<String, String> {
        let mut all: BTreeMap<String, String> = std::env::vars().collect();
        for (name, value) in &self.env {
            match value {
                Some(v) => {
                    all.insert(name.clone(), v.clone());
                }
                None => {
                    all.remove(name);
                }
            }
        }
        all
    }

    /// Applies the script's folder and environment to a command it runs.
    pub fn prepare(&self, command: &mut std::process::Command) {
        command.current_dir(&self.cwd);
        for (name, value) in &self.env {
            match value {
                Some(v) => command.env(name, v),
                None => command.env_remove(name),
            };
        }
    }
}

impl Default for ScriptState {
    fn default() -> Self {
        Self::new()
    }
}
