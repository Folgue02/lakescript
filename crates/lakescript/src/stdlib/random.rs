//! `std/random/native`, the Rust part of `std/random` *(see `random.lks` and
//! `ideas/stdlib/random.md`)*: numbers, `true` or `false`, UUIDs, the seed
//! and the state. Each function takes the generator first: `null` for the
//! shared one, which `$list.random()` and `$list.shuffle()` use too, or the
//! one of a `Random`. Good enough for scripts, not for secrets.

use std::cell::Cell;
use std::rc::Rc;

use super::{Args, Exports};
use crate::exceptions::{TYPE_ERROR, VALUE_ERROR};
use crate::interpreter::{Exec, Interpreter, error_as, native_error};
use crate::token::Span;
use crate::value::Value;

/// The state of a `Random`, its own xorshift64*.
struct Generator(Cell<u64>);

pub fn register(m: &mut Exports) {
    m.vm_function("generator", &["seed"], generator);
    m.vm_function("number", &["generator"], number);
    m.vm_function("between", &["generator", "min", "max"], between);
    m.vm_function("bool", &["generator"], bool);
    m.vm_function("uuid", &[], uuid);
    m.vm_function("seed", &["generator", "seed"], seed);
    m.vm_function("state", &["generator"], state);
    m.vm_function("restore", &["generator", "state"], restore);
}

/// The state of the generator `g`: the shared one for `null`.
fn get(vm: &Interpreter, g: &Value, span: Span) -> Exec<u64> {
    match g {
        Value::Null => Ok(vm.host.rng),
        other => Ok(own(other, span)?.0.get()),
    }
}

fn set(vm: &mut Interpreter, g: &Value, state: u64, span: Span) -> Exec<()> {
    match g {
        Value::Null => vm.host.rng = state,
        other => own(other, span)?.0.set(state),
    }
    Ok(())
}

fn own(g: &Value, span: Span) -> Exec<&Generator> {
    match g {
        Value::Handle(h) => match h.downcast_ref::<Generator>() {
            Some(g) => Ok(g),
            None => error_as(TYPE_ERROR, "a generator only comes from `random::Random()`", span),
        },
        _ => error_as(TYPE_ERROR, "a generator only comes from `random::Random()`", span),
    }
}

/// The next 64 random bits of `g`.
fn next(vm: &mut Interpreter, g: &Value, span: Span) -> Exec<u64> {
    let mut state = get(vm, g, span)?;
    let bits = crate::host::xorshift(&mut state);
    set(vm, g, state, span)?;
    Ok(bits)
}

/// A state from a whole number: splitmix64, so close seeds give different
/// numbers, and never 0, which xorshift can't leave.
fn seeded(n: u64) -> u64 {
    let mut z = n.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    (z ^ (z >> 31)).max(1)
}

/// A `Random`'s generator, from a seed, or from the system without one.
fn generator(_: &mut Interpreter, values: Vec<Value>, span: Span) -> Exec<Value> {
    let a = Args::new("random::Random", values, 1, 1).map_err(|e| native_error(e, span))?;
    let state = match a.opt_whole(0).map_err(|e| native_error(e, span))? {
        Some(n) => seeded(n as u64),
        None => seeded(system_random()),
    };
    Ok(Value::Handle(Rc::new(Generator(Cell::new(state)))))
}

/// From 0 to 1, without 1: 53 random bits, as many as a float has.
fn number(vm: &mut Interpreter, values: Vec<Value>, span: Span) -> Exec<Value> {
    let a = Args::new("random::number", values, 1, 1).map_err(|e| native_error(e, span))?;
    let bits = next(vm, &a.value(0), span)?;
    Ok(Value::Float((bits >> 11) as f64 / (1u64 << 53) as f64))
}

/// A whole number from `min` to `max`, both included.
fn between(vm: &mut Interpreter, values: Vec<Value>, span: Span) -> Exec<Value> {
    let a = Args::new("random::between", values, 3, 3).map_err(|e| native_error(e, span))?;
    let min = a.whole(1).map_err(|e| native_error(e, span))?;
    let max = a.whole(2).map_err(|e| native_error(e, span))?;
    if min > max {
        return error_as(VALUE_ERROR, format!("`random::between` needs `$min` to be at most `$max`, but got {min} and {max}"), span);
    }
    let count = (max as i128 - min as i128 + 1) as u128;
    let offset = (next(vm, &a.value(0), span)? as u128 % count) as i128;
    Ok(Value::Int((min as i128 + offset) as i64))
}

fn bool(vm: &mut Interpreter, values: Vec<Value>, span: Span) -> Exec<Value> {
    let a = Args::new("random::bool", values, 1, 1).map_err(|e| native_error(e, span))?;
    Ok(Value::Bool(next(vm, &a.value(0), span)? >> 63 == 1))
}

/// A version 4 UUID *(i.e. `f47ac10b-58cc-4372-a567-0e02b2c3d479`)*. Its bits
/// come from the system, not from the generator, so `seed()` never makes two
/// runs give the same ones.
fn uuid(_: &mut Interpreter, values: Vec<Value>, span: Span) -> Exec<Value> {
    Args::new("random::uuid", values, 0, 0).map_err(|e| native_error(e, span))?;
    let mut bytes = [0u8; 16];
    bytes[..8].copy_from_slice(&system_random().to_le_bytes());
    bytes[8..].copy_from_slice(&system_random().to_le_bytes());
    // The version, 4, and the variant of RFC 9562
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    Ok(Value::string(&format!("{}-{}-{}-{}-{}", &hex[..8], &hex[8..12], &hex[12..16], &hex[16..20], &hex[20..])))
}

/// 64 bits from the keys the standard library gets from the system for hash
/// maps, which are different for each one.
fn system_random() -> u64 {
    use std::hash::{BuildHasher, Hasher};
    let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
    hasher.write_u64(crate::host::seed());
    hasher.finish()
}

/// Starts the generator again from a number, so what follows is the same on every run.
fn seed(vm: &mut Interpreter, values: Vec<Value>, span: Span) -> Exec<Value> {
    let a = Args::new("random::seed", values, 2, 2).map_err(|e| native_error(e, span))?;
    let n = a.whole(1).map_err(|e| native_error(e, span))?;
    set(vm, &a.value(0), seeded(n as u64), span)?;
    Ok(Value::Null)
}

/// The state, as its 64 bits in hexadecimal, so JSON can hold it.
fn state(vm: &mut Interpreter, values: Vec<Value>, span: Span) -> Exec<Value> {
    let a = Args::new("random::state", values, 1, 1).map_err(|e| native_error(e, span))?;
    Ok(Value::string(&format!("{:016x}", get(vm, &a.value(0), span)?)))
}

fn restore(vm: &mut Interpreter, values: Vec<Value>, span: Span) -> Exec<Value> {
    let a = Args::new("random::restore", values, 2, 2).map_err(|e| native_error(e, span))?;
    let text = a.str(1).map_err(|e| native_error(e, span))?;
    let state = match parse_state(&text) {
        Some(state) => state,
        None => return error_as(VALUE_ERROR, format!("'{text}' isn't a state of a generator: it comes from `random::state()` or `$random.state()`"), span),
    };
    set(vm, &a.value(0), state, span)?;
    Ok(Value::Null)
}

/// 16 hexadecimal digits that aren't all 0, which xorshift can't leave.
fn parse_state(text: &str) -> Option<u64> {
    if text.len() != 16 || !text.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    u64::from_str_radix(text, 16).ok().filter(|&n| n != 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn states() {
        assert_eq!(parse_state("00000000000000ff"), Some(255));
        assert_eq!(parse_state("0000000000000000"), None);
        assert_eq!(parse_state("ff"), None);
        assert_eq!(parse_state("zzzzzzzzzzzzzzzz"), None);
        assert_ne!(seeded(1), seeded(2));
    }
}
