//! `std/random` *(see `ideas/stdlib/random.md`)*: numbers, `true` or `false`,
//! UUIDs and the seed. They share the generator of `$list.random()` and
//! `$list.shuffle()`, so `seed()` makes those repeatable too. Good enough for
//! scripts, not for secrets.

use super::{Args, Exports};
use crate::exceptions::VALUE_ERROR;
use crate::interpreter::{Exec, Interpreter, error_as, native_error};
use crate::token::Span;
use crate::value::Value;

pub fn register(m: &mut Exports) {
    m.vm_function("number", &[], number);
    m.vm_function("between", &["min", "max"], between);
    m.vm_function("bool", &[], bool);
    m.vm_function("uuid", &[], uuid);
    m.vm_function("seed", &["seed"], seed);
}

/// From 0 to 1, without 1: 53 random bits, as many as a float has.
fn number(vm: &mut Interpreter, values: Vec<Value>, span: Span) -> Exec<Value> {
    Args::new("random::number", values, 0, 0).map_err(|e| native_error(e, span))?;
    Ok(Value::Float((vm.host.random() >> 11) as f64 / (1u64 << 53) as f64))
}

/// A whole number from `min` to `max`, both included.
fn between(vm: &mut Interpreter, values: Vec<Value>, span: Span) -> Exec<Value> {
    let a = Args::new("random::between", values, 2, 2).map_err(|e| native_error(e, span))?;
    let min = a.whole(0).map_err(|e| native_error(e, span))?;
    let max = a.whole(1).map_err(|e| native_error(e, span))?;
    if min > max {
        return error_as(VALUE_ERROR, format!("`random::between` needs `$min` to be at most `$max`, but got {min} and {max}"), span);
    }
    let count = (max as i128 - min as i128 + 1) as u128;
    let offset = (vm.host.random() as u128 % count) as i128;
    Ok(Value::Int((min as i128 + offset) as i64))
}

fn bool(vm: &mut Interpreter, values: Vec<Value>, span: Span) -> Exec<Value> {
    Args::new("random::bool", values, 0, 0).map_err(|e| native_error(e, span))?;
    Ok(Value::Bool(vm.host.random() >> 63 == 1))
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
    let a = Args::new("random::seed", values, 1, 1).map_err(|e| native_error(e, span))?;
    let n = a.whole(0).map_err(|e| native_error(e, span))?;
    // splitmix64, so close seeds give different numbers, and never 0, which xorshift can't leave
    let mut z = (n as u64).wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    vm.host.rng = (z ^ (z >> 31)).max(1);
    Ok(Value::Null)
}
