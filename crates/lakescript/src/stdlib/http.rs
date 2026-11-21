//! `std/http/native`: the Rust part of `std/http` *(see `http.lks` and
//! `ideas/stdlib/http.md`)*. Only `http.lks` uses it.

use std::cell::RefCell;
use std::io::Read;
use std::rc::Rc;
use std::time::Duration;

use indexmap::IndexMap;
use ureq::ResponseExt;

use super::{Args, Exports, io_error};
use crate::exceptions::{TYPE_ERROR, VALUE_ERROR};
use crate::value::{Key, NativeContext, NativeError, Value};

pub const NETWORK_ERROR: &str = "NetworkError";

pub fn register(m: &mut Exports) {
    // send(method, url, body, headers, timeout, path): a map with the status,
    // headers, body and url of the response. With `path`, a body that came
    // with a 2xx status is written to that file instead.
    m.function("send", send);
}

fn send(ctx: &mut NativeContext, values: Vec<Value>) -> Result<Value, NativeError> {
    let a = Args::new("http::request", values, 6, 6)?;
    let method = a.str(0)?.to_uppercase();
    let url = a.str(1)?;
    let timeout = match a.value(4) {
        Value::Null => None,
        _ => {
            let seconds = a.num(4)?;
            if !(seconds > 0.0 && seconds.is_finite()) {
                return Err(NativeError::new(VALUE_ERROR, format!("a timeout must be a positive number of seconds, not {seconds}")));
            }
            Some(Duration::from_secs_f64(seconds))
        }
    };
    let path = match a.value(5) {
        Value::Null => None,
        _ => Some(a.str(5)?),
    };

    let mut request = ureq::http::Request::builder().method(method.as_str()).uri(url.as_str());
    let mut content_type = None;
    let body: Vec<u8> = match a.value(2) {
        Value::Null => Vec::new(),
        Value::Str(text) => text.as_bytes().to_vec(),
        other => {
            content_type = Some("application/json");
            super::json::stringify(&other, None)?.into_bytes()
        }
    };
    let mut given_type = false;
    if let Value::Map(headers) = a.value(3) {
        for (name, value) in headers.borrow().iter() {
            let (Key::Str(name), Value::Str(value)) = (name, value) else {
                return Err(NativeError::new(TYPE_ERROR, "the headers of a request must be a map of strings"));
            };
            given_type |= name.eq_ignore_ascii_case("content-type");
            request = request.header(&**name, &**value);
        }
    }
    if let (Some(t), false) = (content_type, given_type) {
        request = request.header("content-type", t);
    }
    let request = request.body(body).map_err(|_| NativeError::new(VALUE_ERROR, format!("'{url}' isn't a valid address, or a header isn't valid")))?;

    let agent: ureq::Agent = ureq::Agent::config_builder()
        .http_status_as_error(false)
        .timeout_global(timeout)
        .user_agent(concat!("lakescript/", env!("CARGO_PKG_VERSION")))
        .build()
        .into();
    let mut response = agent.run(request).map_err(|e| failed(&url, e))?;

    let status = response.status().as_u16();
    let final_url = response.get_uri().to_string();
    let mut headers = IndexMap::new();
    for (name, value) in response.headers() {
        let value = String::from_utf8_lossy(value.as_bytes());
        // A header that comes several times: its values joined, as HTTP allows
        let entry = headers.entry(Key::Str(Rc::from(name.as_str()))).or_insert_with(|| Value::string(""));
        let joined = match entry {
            Value::Str(old) if !old.is_empty() => format!("{old}, {value}"),
            _ => value.into_owned(),
        };
        *entry = Value::string(&joined);
    }

    let body = match path {
        Some(p) if (200..300).contains(&status) => {
            let file = ctx.state.resolve(&p);
            let mut out = std::fs::File::create(&file).map_err(|e| io_error("write", &p, e))?;
            let mut reader = response.body_mut().as_reader();
            std::io::copy(&mut reader, &mut out).map_err(|e| read_failed(&url, e))?;
            String::new()
        }
        _ => {
            let mut bytes = Vec::new();
            response.body_mut().as_reader().read_to_end(&mut bytes).map_err(|e| read_failed(&url, e))?;
            String::from_utf8_lossy(&bytes).into_owned()
        }
    };

    let mut map = IndexMap::new();
    map.insert(Key::Str(Rc::from("status")), Value::Int(status as i64));
    map.insert(Key::Str(Rc::from("headers")), Value::Map(Rc::new(RefCell::new(headers))));
    map.insert(Key::Str(Rc::from("body")), Value::string(&body));
    map.insert(Key::Str(Rc::from("url")), Value::string(&final_url));
    Ok(Value::Map(Rc::new(RefCell::new(map))))
}

/// A request that got no response.
fn failed(url: &str, e: ureq::Error) -> NativeError {
    let reason = match e {
        ureq::Error::BadUri(_) | ureq::Error::Http(_) => {
            return NativeError::new(VALUE_ERROR, format!("'{url}' isn't a valid address"));
        }
        ureq::Error::HostNotFound => "the name of the server wasn't found".to_string(),
        ureq::Error::ConnectionFailed => "the connection failed".to_string(),
        ureq::Error::Timeout(_) => "it took too long".to_string(),
        ureq::Error::TooManyRedirects => "it was redirected too many times".to_string(),
        ureq::Error::Io(e) => io_reason(&e),
        other => other.to_string(),
    };
    NativeError::new(NETWORK_ERROR, format!("can't reach '{url}': {reason}"))
}

/// A response whose body stopped coming.
fn read_failed(url: &str, e: std::io::Error) -> NativeError {
    NativeError::new(NETWORK_ERROR, format!("can't read the answer of '{url}': {}", io_reason(&e)))
}

/// What the system said, in words that don't need its codes.
fn io_reason(e: &std::io::Error) -> String {
    use std::io::ErrorKind::*;
    match e.kind() {
        ConnectionRefused => "the server refused the connection".into(),
        ConnectionReset | ConnectionAborted => "the server closed the connection".into(),
        TimedOut => "it took too long".into(),
        // The resolver's errors have no kind of their own
        _ if e.to_string().contains("lookup address") => "the name of the server wasn't found".into(),
        _ => e.to_string(),
    }
}
