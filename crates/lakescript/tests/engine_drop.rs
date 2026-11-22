//! An engine frees what its scripts declared when it's dropped.

// Only with the features of the crate these modules need
#![cfg(all(feature = "html", feature = "dates"))]

use std::rc::Rc;

use lakescript::{CapturedOutput, Engine, Value};

#[test]
fn dropping_an_engine_frees_its_globals() {
    // A value only the script holds: when it's freed, this is the last one
    let token: Rc<dyn std::any::Any> = Rc::new(());
    let mut engine = Engine::with_output(CapturedOutput::new());
    let given = token.clone();
    engine.register("token", move |_, _| Ok(Value::Handle(given.clone())));
    // A function keeps its top level, which keeps the function and `$kept`
    engine
        .run("use std/html;\nuse std/time;\nconst $kept = token();\nclass Box {\n    pub any @inside;\n    pub static Box of(any $v) = Box($v);\n}\nconst $boxed = Box::of(token());\nvoid f() {}\nconst $g = () -> f();\nconst $page = html::parse('<p>x</p>');\nconst $d = time::date();")
        .unwrap();
    assert!(Rc::strong_count(&token) > 2, "the script holds it while the engine lives");
    drop(engine);
    assert_eq!(Rc::strong_count(&token), 1, "the engine's globals were freed");
}
