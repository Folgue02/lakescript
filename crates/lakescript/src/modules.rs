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
    /// Drops every value, when the engine is done with them. A function
    /// keeps the top level it was declared in, and the top level keeps the
    /// function, so without this neither would ever be freed.
    pub fn clear(&self) {
        // Taken out first: dropping a value can drop others that look in here
        let slots = std::mem::take(&mut *self.slots.borrow_mut());
        let modules = std::mem::take(&mut *self.modules.borrow_mut());
        drop(slots);
        drop(modules);
    }

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
    /// The modules being loaded right now. A module that a module it uses
    /// uses back gets it while it's still loading.
    pub loading: Vec<PathBuf>,
    pub in_progress: HashMap<PathBuf, Rc<Module>>,
    /// Names imported from a module that was still loading, given their
    /// values when it finishes: the module, where they go, and their names
    /// and the globals they go into.
    pub pending: Vec<(PathBuf, Rc<Globals>, Vec<(String, u32)>)>,
    /// The text of files that an editor has open, read instead of the disk,
    /// so what isn't saved yet is checked *(see `ideas/tooling/lsp.md`)*.
    pub overlay: HashMap<PathBuf, String>,
}

impl Loader {
    pub fn new() -> Self {
        Loader { files: vec!["<script>".to_string()], ..Default::default() }
    }

    /// The text of a module: from the overlay, or from the disk. Modules of
    /// the standard library written in Lakescript come with the interpreter.
    pub fn read(&self, path: &std::path::Path) -> std::io::Result<String> {
        if let Some(text) = path.to_str().and_then(crate::stdlib::embedded) {
            return Ok(text.to_string());
        }
        if let Some(text) = self.overlay.get(path) {
            return Ok(text.clone());
        }
        std::fs::read_to_string(path)
    }

    /// Finds the module for a `use` path *(i.e. `["config", "paths"]`)*, in
    /// the file `importer` *(as the engine shows it)*.
    pub fn resolve(&self, path: &[String], importer: &str) -> Result<ModuleSource, String> {
        let joined = path.join("/");
        if path.first().is_some_and(|p| p == "std") {
            // The Rust parts of modules of the standard library are only for them
            let hidden = crate::stdlib::native(&joined) && !importer.starts_with(crate::stdlib::EMBEDDED);
            if let Some(feature) = crate::stdlib::left_out(&joined) {
                return Err(format!("`{joined}` isn't in this program: it was built without the `{feature}` feature of Lakescript"));
            }
            if !crate::stdlib::exists(&joined) || hidden {
                return Err(format!("there's no module `{joined}` in the standard library"));
            }
            if self.disabled.contains(&joined) {
                return Err(format!("`{joined}` isn't available here"));
            }
            if let Some(file) = crate::stdlib::embedded_path(&joined) {
                return Ok(ModuleSource::File(file));
            }
            return Ok(ModuleSource::Std(joined));
        }
        let relative: PathBuf = path.iter().collect::<PathBuf>().with_extension("lks");
        let found: Vec<PathBuf> = self
            .roots
            .iter()
            .map(|r| r.join(&relative))
            .filter(|p| p.is_file() || self.overlay.contains_key(p))
            .collect();
        match found.as_slice() {
            [] => {
                // On a file system that sees case, a name in another case isn't found: say why
                for root in &self.roots {
                    check_case(root, path)?;
                }
                let roots: Vec<String> = self.roots.iter().map(|r| r.display().to_string()).collect();
                if roots.is_empty() {
                    Err(format!("module `{joined}` not found: there's nowhere to look for modules"))
                } else {
                    Err(format!("module `{joined}` not found in {}", roots.join(", ")))
                }
            }
            [one] => {
                let root = self.roots.iter().find(|r| r.join(&relative) == *one).cloned().unwrap_or_default();
                check_case(&root, path)?;
                Ok(ModuleSource::File(one.canonicalize().unwrap_or_else(|_| one.clone())))
            }
            [first, second, ..] => Err(format!(
                "module `{joined}` is in two places: {} and {}",
                first.display(),
                second.display()
            )),
        }
    }
}

/// The path of a `use` must match the files' names exactly, even where the
/// file system ignores case *(i.e. macOS)*, so a script that works on one
/// machine works on all. And two modules in a folder can't have names that
/// only differ in case or in `_` *(i.e. `ChildClass.lks` and `child_class.lks`)*.
fn check_case(root: &std::path::Path, path: &[String]) -> Result<(), String> {
    let joined = path.join("/");
    let simple = |name: &str| name.to_ascii_lowercase().replace('_', "");
    let mut dir = root.to_path_buf();
    for (i, part) in path.iter().enumerate() {
        let last = i == path.len() - 1;
        let wanted = if last { format!("{part}.lks") } else { part.clone() };
        let Ok(entries) = std::fs::read_dir(&dir) else { return Ok(()) };
        let names: Vec<String> = entries.filter_map(|e| e.ok()).map(|e| e.file_name().to_string_lossy().into_owned()).collect();
        if !names.iter().any(|n| *n == wanted) {
            if let Some(real) = names.iter().find(|n| n.eq_ignore_ascii_case(&wanted)) {
                return Err(format!("`use {joined}` must match the name of the file exactly, which is `{real}`"));
            }
        }
        if last {
            let stem = simple(part);
            let clash = names.iter().find(|n| *n != &wanted && n.ends_with(".lks") && simple(n.trim_end_matches(".lks")) == stem);
            if let Some(other) = clash {
                return Err(format!("`{wanted}` and `{other}` have the same name in another case: rename one of them"));
            }
        }
        dir.push(part);
    }
    Ok(())
}
