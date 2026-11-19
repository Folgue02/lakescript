//! Finding and loading modules *(see `ideas/language/modules.md`)*.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::rc::Rc;

use crate::scope::Env;

/// A loaded module: its scope, and the names it makes available with `pub`.
pub struct Module {
    pub path: String,
    pub env: Env,
    /// Function names, and constant names with their `$`.
    pub public: HashSet<String>,
}

#[derive(Clone)]
pub enum ModuleSource {
    /// A module of the standard library, like `std/fs`.
    Std(String),
    File(PathBuf),
}

/// What a `use` of a whole module leaves in the scope. It's loaded the first
/// time something in it is used.
pub enum ModuleSlot {
    Pending(ModuleSource),
    Loaded(Rc<Module>),
}

#[derive(Default)]
pub struct Loader {
    /// Where `use` looks for modules, in order.
    pub roots: Vec<PathBuf>,
    /// The name of each file, by its number. `0` is the script being run.
    pub files: Vec<String>,
    /// Modules of the standard library that the program embedding Lakescript left out.
    pub disabled: HashSet<String>,
    pub cache: HashMap<PathBuf, Rc<Module>>,
    pub std_cache: HashMap<String, Rc<Module>>,
    /// The modules being loaded right now, to find circular imports.
    pub loading: Vec<PathBuf>,
}

impl Loader {
    pub fn new() -> Self {
        Loader { files: vec!["<script>".to_string()], ..Default::default() }
    }

    /// Finds the module for a `use` path *(i.e. `["config", "paths"]`)*.
    pub fn resolve(&self, path: &[String]) -> Result<ModuleSource, String> {
        let joined = path.join("/");
        if path.first().is_some_and(|p| p == "std") {
            if !crate::stdlib::exists(&joined) {
                return Err(format!("there's no module `{joined}` in the standard library"));
            }
            if self.disabled.contains(&joined) {
                return Err(format!("`{joined}` isn't available here"));
            }
            return Ok(ModuleSource::Std(joined));
        }
        let relative: PathBuf = path.iter().collect::<PathBuf>().with_extension("lks");
        let found: Vec<PathBuf> = self.roots.iter().map(|r| r.join(&relative)).filter(|p| p.is_file()).collect();
        match found.as_slice() {
            [] => {
                let roots: Vec<String> = self.roots.iter().map(|r| r.display().to_string()).collect();
                if roots.is_empty() {
                    Err(format!("module `{joined}` not found: there's nowhere to look for modules"))
                } else {
                    Err(format!("module `{joined}` not found in {}", roots.join(", ")))
                }
            }
            [one] => Ok(ModuleSource::File(one.canonicalize().unwrap_or_else(|_| one.clone()))),
            [first, second, ..] => Err(format!(
                "module `{joined}` is in two places: {} and {}",
                first.display(),
                second.display()
            )),
        }
    }
}
