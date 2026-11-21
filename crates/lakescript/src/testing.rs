//! Running the tests of a file: its functions named `test...` *(see
//! `ideas/tooling/testing.md`)*. `lks --test` and `ferry test` use it.

use std::io::Write;
use std::path::Path;
use std::time::Instant;

use crate::ast::{Stmt, StmtKind};
use crate::diagnostic::Diagnostic;
use crate::engine::{CapturedOutput, Engine};
use crate::exceptions;
use crate::interpreter::{Flow, Interpreter, uncaught};
use crate::terminal::Paint;
use crate::value::Value;

/// What one test did.
pub struct TestResult {
    /// The name of its function *(i.e. `testBytesToText`)*.
    pub name: String,
    /// Why it failed, or `None` when it passed.
    pub failure: Option<Diagnostic>,
    /// What it printed, to stdout and stderr.
    pub output: String,
    pub seconds: f64,
}

impl TestResult {
    pub fn passed(&self) -> bool {
        self.failure.is_none()
    }
}

/// The tests of a program: its top-level functions whose name starts with
/// `test` and that take no arguments, in the order they're written.
pub fn test_names(program: &[Stmt]) -> Vec<String> {
    program
        .iter()
        .filter_map(|stmt| {
            let inner = match &stmt.kind {
                StmtKind::Pub(inner) => &inner.kind,
                other => other,
            };
            match inner {
                StmtKind::Function(decl) if decl.name.starts_with("test") && decl.params.is_empty() => Some(decl.name.clone()),
                _ => None,
            }
        })
        .collect()
}

impl Engine {
    /// Runs the file at `path`, then each of its tests whose name has
    /// `filter` in it. The error is the file's, when it can't be checked or
    /// its top level fails, and then no test runs. What a test prints is
    /// kept in its result instead of being written.
    pub fn run_tests(&mut self, path: &Path, filter: Option<&str>) -> Result<Vec<TestResult>, Diagnostic> {
        let source = self.open_file(path)?;
        let program = crate::parse_budgeted(&source, 0, self.stack_budget)?;
        let names: Vec<String> = test_names(&program).into_iter().filter(|n| filter.is_none_or(|f| n.contains(f))).collect();
        self.run(&source)?;
        let mut results = Vec::new();
        for name in names {
            let Some(Value::Function(function)) = self.globals.get(&name) else { continue };
            let captured = CapturedOutput::new();
            let output = std::mem::replace(&mut self.host.output, Box::new(captured.clone()));
            let error = std::mem::replace(&mut self.host.error, Box::new(captured.clone()));
            let start = Instant::now();
            let result = Interpreter::new(&mut self.host, self.stack_budget).call(&function, Vec::new(), Vec::new(), Default::default());
            let seconds = start.elapsed().as_secs_f64();
            self.host.output = output;
            self.host.error = error;
            let failure = match result {
                Ok(_) => None,
                Err(Flow::Throw(e)) if &*e.kind == exceptions::ASSERTION_ERROR => Some(Diagnostic::new(e.message.clone(), e.span)),
                Err(Flow::Throw(e)) => Some(uncaught(&e)),
                Err(Flow::Exit(code)) => Some(Diagnostic::new(format!("the test called `exit({code})`"), Default::default())),
            };
            results.push(TestResult { name, failure: failure.map(|d| self.locate(d)), output: captured.text(), seconds });
        }
        let _ = self.host.output.flush();
        Ok(results)
    }
}

/// How a test is shown: `ok` or `FAIL` and its name, and for a failure,
/// where and why, and what it printed. `file` is the test file, for errors
/// that aren't in a module.
pub fn render_result(result: &TestResult, file: &str, paint: Paint) -> String {
    let indent = " ".repeat(13);
    let Some(failure) = &result.failure else {
        return format!("{} {}\n", paint.good(&format!("{:>12}", "ok")), result.name);
    };
    let mut text = format!("{} {}\n", paint.error(&format!("{:>12}", "FAIL")), result.name);
    let place = match failure.span.line {
        0 => String::new(),
        line => format!("{}:{line}:{}: ", failure.file.as_deref().unwrap_or(file), failure.span.column),
    };
    for line in format!("{place}{}", failure.message).lines() {
        text.push_str(&format!("{indent}{line}\n"));
    }
    if !result.output.is_empty() {
        text.push_str(&format!("{indent}{}\n", paint.dim("It printed:")));
        for line in result.output.lines() {
            text.push_str(&format!("{indent}  {line}\n"));
        }
    }
    text
}

/// The last line: how many passed and failed, and how long it took.
pub fn render_summary(results: &[TestResult], seconds: f64, paint: Paint) -> String {
    let failed = results.iter().filter(|r| !r.passed()).count();
    let passed = results.len() - failed;
    let verb = if failed > 0 { paint.error(&format!("{:>12}", "Finished")) } else { paint.good(&format!("{:>12}", "Finished")) };
    format!("{verb} {passed} passed, {failed} failed in {seconds:.2} s\n")
}
