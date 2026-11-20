//! Finding and loading modules *(see `ideas/language/modules.md`)*.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::rc::Rc;

use crate::value::Value;

/// A loaded module: its globals, and the names it makes available with `pub`.
pub struct Module {
    pub path: String,
    pub globals: Rc<Globals>,
    /// Function names, and constant names with their `$`.
    pub public: HashSet<String>,
}

/// A variable or function declared at the top level of a script or module.
pub struct Global {
    /// Variables with their `$`, functions without.
    pub key: Rc<str>,
    pub value: Value,
    /// Whether its declaration has run. A global gets its number when the
    /// compiler first sees its name, which can be before it's declared.
    pub declared: bool,
    pub constant: bool,
}

/// The top level of a script or a module. The compiler gives every name a
/// number, and the code reaches it by that number.
#[derive(Default)]
pub struct Globals {
    pub slots: RefCell<Vec<Global>>,
    names: RefCell<HashMap<Rc<str>, u32>>,
    /// The modules imported whole with `use`, by number. `None` until the `use` runs.
    pub modules: RefCell<Vec<Option<ModuleSlot>>>,
    module_names: RefCell<HashMap<String, u32>>,
}

impl Globals {
    /// The number of a global, giving it one if it has none.
    pub fn slot(&self, key: &str) -> u32 {
        if let Some(&i) = self.names.borrow().get(key) {
            return i;
        }
        let mut slots = self.slots.borrow_mut();
        let i = slots.len() as u32;
        let key: Rc<str> = Rc::from(key);
        slots.push(Global { key: key.clone(), value: Value::Null, declared: false, constant: false });
        self.names.borrow_mut().insert(key, i);
        i
    }

    pub fn find(&self, key: &str) -> Option<u32> {
        self.names.borrow().get(key).copied()
    }

    pub fn is_declared(&self, key: &str) -> bool {
        self.find(key).is_some_and(|i| self.slots.borrow()[i as usize].declared)
    }

    /// The value of a declared global.
    pub fn get(&self, key: &str) -> Option<Value> {
        let i = self.find(key)?;
        let slots = self.slots.borrow();
        let global = &slots[i as usize];
        global.declared.then(|| global.value.clone())
    }

    /// Declares a global. Returns `false` if it already exists.
    pub fn define(&self, key: &str, value: Value, constant: bool) -> bool {
        let i = self.slot(key) as usize;
        let mut slots = self.slots.borrow_mut();
        if slots[i].declared {
            return false;
        }
        slots[i].value = value;
        slots[i].declared = true;
        slots[i].constant = constant;
        true
    }

    /// The number of a module imported whole, giving it one if it has none.
    pub fn module_slot(&self, name: &str) -> u32 {
        if let Some(&i) = self.module_names.borrow().get(name) {
            return i;
        }
        let mut modules = self.modules.borrow_mut();
        let i = modules.len() as u32;
        modules.push(None);
        self.module_names.borrow_mut().insert(name.to_string(), i);
        i
    }

    pub fn find_module(&self, name: &str) -> Option<u32> {
        self.module_names.borrow().get(name).copied()
    }
}

#[derive(Clone, Debug)]
pub enum ModuleSource {
    /// A module of the standard library, like `std/fs`.
    Std(String),
    File(PathBuf),
}

/// What a `use` of a whole module leaves in the scope. It's loaded the first
/// time something in it is used.
#[derive(Clone)]
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
