//! Scopes: where variables and functions live.
//!
//! Every block, loop pass and function call gets a new scope whose parent is
//! the scope around it. Functions keep the scope they were defined in, so
//! they see the variables around them, and see later changes to them, like
//! closures in JavaScript.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::value::{Function, Value};

pub type Env = Rc<RefCell<Scope>>;

#[derive(Default)]
pub struct Scope {
    vars: HashMap<String, Binding>,
    functions: HashMap<String, Rc<Function>>,
    parent: Option<Env>,
}

struct Binding {
    value: Value,
    constant: bool,
}

impl Scope {
    pub fn remove_function(&mut self, name: &str) {
        self.functions.remove(name);
    }
}

pub enum SetError {
    Undeclared,
    Constant,
}

pub fn new_scope(parent: &Env) -> Env {
    Rc::new(RefCell::new(Scope { parent: Some(parent.clone()), ..Default::default() }))
}

pub fn root_scope() -> Env {
    Rc::new(RefCell::new(Scope::default()))
}

/// Declares a variable. Returns `false` if a variable with that name already
/// exists in this scope or in one around it: there is no shadowing.
pub fn declare(env: &Env, name: &str, value: Value, constant: bool) -> bool {
    if lookup(env, name).is_some() {
        return false;
    }
    env.borrow_mut().vars.insert(name.to_string(), Binding { value, constant });
    true
}

pub fn lookup(env: &Env, name: &str) -> Option<Value> {
    let mut current = Some(env.clone());
    while let Some(scope) = current {
        let scope = scope.borrow();
        if let Some(binding) = scope.vars.get(name) {
            return Some(binding.value.clone());
        }
        current = scope.parent.clone();
    }
    None
}

pub fn assign(env: &Env, name: &str, value: Value) -> Result<(), SetError> {
    let mut current = Some(env.clone());
    while let Some(scope) = current {
        let mut scope = scope.borrow_mut();
        if let Some(binding) = scope.vars.get_mut(name) {
            if binding.constant {
                return Err(SetError::Constant);
            }
            binding.value = value;
            return Ok(());
        }
        current = scope.parent.clone();
    }
    Err(SetError::Undeclared)
}

/// Declares a function. Returns `false` if one with that name already exists:
/// there is no overloading.
pub fn declare_function(env: &Env, name: &str, function: Rc<Function>) -> bool {
    if lookup_function(env, name).is_some() {
        return false;
    }
    env.borrow_mut().functions.insert(name.to_string(), function);
    true
}

pub fn lookup_function(env: &Env, name: &str) -> Option<Rc<Function>> {
    let mut current = Some(env.clone());
    while let Some(scope) = current {
        let scope = scope.borrow();
        if let Some(function) = scope.functions.get(name) {
            return Some(function.clone());
        }
        current = scope.parent.clone();
    }
    None
}
