//! `std/test`: what tests need besides `assert` *(see `ideas/stdlib/test.md`)*.

use super::{Args, Exports};
use crate::exceptions::{ASSERTION_ERROR, TYPE_ERROR};
use crate::interpreter::{Exec, Flow, Interpreter, flow_error_as, native_error};
use crate::token::Span;
use crate::value::Value;

pub fn register(m: &mut Exports) {
    m.vm_function("throws", &["code"], throws);
}

/// Runs the function, and returns the exception it threw. Throwing nothing is
/// an `AssertionError`, so the test fails.
fn throws(vm: &mut Interpreter, values: Vec<Value>, span: Span) -> Exec<Value> {
    let a = Args::new("test::throws", values, 1, 1).map_err(|e| native_error(e, span))?;
    let Value::Function(code) = a.value(0) else {
        return Err(flow_error_as(TYPE_ERROR, format!("`test::throws` takes a function, not a {}", a.value(0).type_name()), span));
    };
    match vm.call(&code, Vec::new(), Vec::new(), span) {
        Ok(_) => Err(flow_error_as(ASSERTION_ERROR, "the code given to `test::throws` threw nothing", span)),
        // Ctrl+C isn't what the test waits for
        Err(Flow::Throw(e)) if &*e.kind != crate::exceptions::INTERRUPTED => Ok(Value::Exception(e)),
        Err(exit) => Err(exit),
    }
}
