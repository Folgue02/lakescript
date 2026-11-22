//! `std/time/native`: the Rust part of `std/time` *(see `time.lks` and
//! `ideas/stdlib/time.md`)*. The current time is always here, and dates and
//! zones come with the `dates` feature *(see `dates.rs`)*.

use super::{Args, Exports};
use crate::value::Value;

pub fn register(m: &mut Exports) {
    m.function("now", |_, values| {
        Args::new("time::now", values, 0, 0)?;
        let millis = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0);
        Ok(Value::Int(millis))
    });
    #[cfg(feature = "dates")]
    super::dates::register(m);
}
