//! `std/bytes` *(see `ideas/stdlib/bytes.md`)*: bytes from hex and base64.

use std::rc::Rc;

use super::{Args, Exports};
use crate::exceptions::VALUE_ERROR;
use crate::value::{NativeError, Value};

pub fn register(m: &mut Exports) {
    m.function("fromHex", |_, values| {
        let a = Args::new("bytes::fromHex", values, 1, 1)?;
        let bytes = crate::bytes::from_hex(&a.str(0)?).map_err(|e| NativeError::new(VALUE_ERROR, e))?;
        Ok(Value::Bytes(Rc::from(bytes)))
    });
    m.function("fromBase64", |_, values| {
        let a = Args::new("bytes::fromBase64", values, 1, 1)?;
        let bytes = crate::bytes::from_base64(&a.str(0)?).map_err(|e| NativeError::new(VALUE_ERROR, e))?;
        Ok(Value::Bytes(Rc::from(bytes)))
    });
}
