//! `std/io` *(see `ideas/stdlib/io.md`)*.

use super::{Args, Exports, strings};
use crate::value::{NativeError, Value};

fn read_error(e: std::io::Error) -> NativeError {
    NativeError::new("IOError", format!("can't read stdin: {e}"))
}

pub fn register(m: &mut Exports) {
    m.function("readLine", |ctx, values| {
        Args::new("io::readLine", values, 0, 0)?;
        let mut line = String::new();
        let n = ctx.input.read_line(&mut line).map_err(read_error)?;
        if n == 0 {
            return Ok(Value::Null);
        }
        if line.ends_with('\n') {
            line.pop();
            if line.ends_with('\r') {
                line.pop();
            }
        }
        Ok(Value::string(&line))
    });
    m.function("readAll", |ctx, values| {
        Args::new("io::readAll", values, 0, 0)?;
        let mut text = String::new();
        ctx.input.read_to_string(&mut text).map_err(read_error)?;
        Ok(Value::string(&text))
    });
    m.function("lines", |ctx, values| {
        Args::new("io::lines", values, 0, 0)?;
        let mut text = String::new();
        ctx.input.read_to_string(&mut text).map_err(read_error)?;
        Ok(strings(text.lines().map(String::from)))
    });
    m.function("piped?", |ctx, values| {
        Args::new("io::piped?", values, 0, 0)?;
        Ok(Value::Bool(ctx.input_piped))
    });
    m.function("flush", |ctx, values| {
        Args::new("io::flush", values, 0, 0)?;
        let _ = ctx.output.flush();
        let _ = ctx.error.flush();
        Ok(Value::Null)
    });
}
