//! Everything a running script can reach outside itself: output, input, the
//! current folder, environment variables, modules. The engine owns it, so
//! two engines never share anything, and a script never changes the process
//! of the program that embeds it.

use std::collections::{BTreeMap, HashMap};
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use crate::modules::Loader;
use crate::value::{RegexValue, Value};

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
    /// The built-in functions and the ones the host registered, by number, and their numbers by name.
    pub builtins: Vec<Value>,
    pub builtin_names: HashMap<String, u32>,
    /// The state of the random number generator, for `random()` and `shuffle()`.
    pub rng: u64,
    /// Set by the program that runs the script when Ctrl+C is pressed: the
    /// VM throws an `Interrupted` where the script is *(see `Engine::interrupt_flag`)*.
    pub interrupt: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl Host {
    /// A random number, from xorshift64*. Good enough for scripts, not for secrets.
    pub fn random(&mut self) -> u64 {
        let mut x = self.rng;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.rng = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// A random number from `0` to `n - 1`.
    pub fn random_below(&mut self, n: usize) -> usize {
        (self.random() % n as u64) as usize
    }
}

/// A seed that's different on every run.
pub fn seed() -> u64 {
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(0);
    let local = 0u8;
    let address = &local as *const u8 as u64;
    (nanos ^ (std::process::id() as u64).rotate_left(32) ^ address) | 1
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

impl Host {
    /// One of the interfaces behind the operators *(i.e. `IAdd`)*.
    pub fn builtin_interface(&self, name: &str) -> Option<Rc<crate::value::Class>> {
        match self.builtins.get(*self.builtin_names.get(name)? as usize)? {
            Value::Class(c) if c.interface => Some(c.clone()),
            _ => None,
        }
    }
}
