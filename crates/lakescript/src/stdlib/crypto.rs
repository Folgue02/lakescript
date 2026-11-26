//! `std/crypto/hash`, `std/crypto/random`, `std/crypto/seal` and
//! `std/crypto/password` *(see `ideas/stdlib/crypto.md`)*: hashes and HMAC,
//! secure random values, sealing data with a key, and password hashes. On
//! the RustCrypto crates, in pure Rust, so they build for every system.

use std::io::Read;
use std::rc::Rc;

use super::{Args, Exports, io_error};
use crate::exceptions::{TYPE_ERROR, VALUE_ERROR};
use crate::value::{NativeError, Value};

/// The bytes of a string, as UTF-8, or of `bytes`.
fn data(a: &Args, i: usize, what: &str) -> Result<Vec<u8>, NativeError> {
    match a.value(i) {
        Value::Str(s) => Ok(s.as_bytes().to_vec()),
        Value::Bytes(b) => Ok(b.to_vec()),
        other => Err(NativeError::new(TYPE_ERROR, format!("`{what}` takes a string or bytes, not a {}", other.type_name()))),
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn system(e: impl std::fmt::Display) -> NativeError {
    NativeError::new(VALUE_ERROR, format!("the system gave no random bytes: {e}"))
}

/// `n` bytes from the system's secure source.
fn random_bytes(n: usize) -> Result<Vec<u8>, NativeError> {
    let mut out = vec![0u8; n];
    getrandom::fill(&mut out).map_err(system)?;
    Ok(out)
}

// ---- std/crypto/hash ----

/// A hash being computed, for the algorithms there are.
enum Hasher {
    Sha256(sha2::Sha256),
    Sha512(sha2::Sha512),
    Sha1(sha1::Sha1),
    Md5(md5::Md5),
    Crc32(crc32fast::Hasher),
}

impl Hasher {
    fn named(name: &str) -> Result<Hasher, NativeError> {
        use sha2::Digest;
        Ok(match name {
            "sha256" => Hasher::Sha256(sha2::Sha256::new()),
            "sha512" => Hasher::Sha512(sha2::Sha512::new()),
            "sha1" => Hasher::Sha1(sha1::Sha1::new()),
            "md5" => Hasher::Md5(md5::Md5::new()),
            "crc32" => Hasher::Crc32(crc32fast::Hasher::new()),
            other => return Err(NativeError::new(VALUE_ERROR, format!("'{other}' isn't a hash there is: they're 'sha256', 'sha512', 'sha1', 'md5' and 'crc32'"))),
        })
    }

    fn update(&mut self, bytes: &[u8]) {
        use sha2::Digest;
        match self {
            Hasher::Sha256(h) => h.update(bytes),
            Hasher::Sha512(h) => h.update(bytes),
            Hasher::Sha1(h) => h.update(bytes),
            Hasher::Md5(h) => h.update(bytes),
            Hasher::Crc32(h) => h.update(bytes),
        }
    }

    fn hex(self) -> String {
        use sha2::Digest;
        match self {
            Hasher::Sha256(h) => hex(&h.finalize()),
            Hasher::Sha512(h) => hex(&h.finalize()),
            Hasher::Sha1(h) => hex(&h.finalize()),
            Hasher::Md5(h) => hex(&h.finalize()),
            Hasher::Crc32(h) => format!("{:08x}", h.finalize()),
        }
    }
}

fn hash_of(name: &'static str, function: &'static str) -> impl Fn(&mut super::NativeContext, Vec<Value>) -> Result<Value, NativeError> {
    move |_, values| {
        let a = Args::new(function, values, 1, 1)?;
        let mut h = Hasher::named(name)?;
        h.update(&data(&a, 0, function)?);
        Ok(Value::string(&h.hex()))
    }
}

/// The HMAC of `data` with `key`, with SHA-256 or SHA-512.
fn hmac(algorithm: &str, key: &[u8], data: &[u8]) -> Result<Vec<u8>, NativeError> {
    use hmac::{KeyInit, Mac};
    let wrong = || NativeError::new(VALUE_ERROR, format!("'{algorithm}' isn't a hash HMAC can use here: they're 'sha256' and 'sha512'"));
    Ok(match algorithm {
        "sha256" => {
            let mut mac = <hmac::Hmac<sha2::Sha256> as KeyInit>::new_from_slice(key).map_err(|_| wrong())?;
            mac.update(data);
            mac.finalize().into_bytes().to_vec()
        }
        "sha512" => {
            let mut mac = <hmac::Hmac<sha2::Sha512> as KeyInit>::new_from_slice(key).map_err(|_| wrong())?;
            mac.update(data);
            mac.finalize().into_bytes().to_vec()
        }
        _ => return Err(wrong()),
    })
}

pub fn register_hash(m: &mut Exports) {
    m.function("sha256", hash_of("sha256", "hash::sha256"));
    m.function("sha512", hash_of("sha512", "hash::sha512"));
    m.function("sha1", hash_of("sha1", "hash::sha1"));
    m.function("md5", hash_of("md5", "hash::md5"));
    m.function("crc32", hash_of("crc32", "hash::crc32"));
    m.function_named("file", &["path", "algorithm"], |ctx, values| {
        let a = Args::new("hash::file", values, 1, 2)?;
        let p = a.str(0)?;
        let mut h = Hasher::named(&a.str_or(1, "sha256")?)?;
        // In pieces, so a big file never has to fit in memory
        let mut file = std::fs::File::open(ctx.state.resolve(&p)).map_err(|e| io_error("read", &p, e))?;
        let mut buffer = vec![0u8; 64 * 1024];
        loop {
            let n = file.read(&mut buffer).map_err(|e| io_error("read", &p, e))?;
            if n == 0 {
                break;
            }
            h.update(&buffer[..n]);
        }
        Ok(Value::string(&h.hex()))
    });
    m.function_named("hmac", &["key", "data", "algorithm"], |_, values| {
        let a = Args::new("hash::hmac", values, 2, 3)?;
        Ok(Value::string(&hex(&hmac(&a.str_or(2, "sha256")?, &data(&a, 0, "hash::hmac")?, &data(&a, 1, "hash::hmac")?)?)))
    });
    m.function_named("verifyHmac", &["key", "data", "signature", "algorithm"], |_, values| {
        use subtle::ConstantTimeEq;
        let a = Args::new("hash::verifyHmac", values, 3, 4)?;
        let expected = hmac(&a.str_or(3, "sha256")?, &data(&a, 0, "hash::verifyHmac")?, &data(&a, 1, "hash::verifyHmac")?)?;
        // In hex, in any case, like most APIs send it
        let given = a.str(2)?.to_ascii_lowercase();
        let given = given.strip_prefix("sha256=").or_else(|| given.strip_prefix("sha512=")).unwrap_or(&given);
        // Compared in the same time whatever matches, so the time says nothing about it
        Ok(Value::Bool(hex(&expected).as_bytes().ct_eq(given.as_bytes()).into()))
    });
}

// ---- std/crypto/random ----

pub fn register_random(m: &mut Exports) {
    m.function_named("bytes", &["length"], |_, values| {
        let a = Args::new("random::bytes", values, 1, 1)?;
        let n = count(&a, 0, "random::bytes", 32)?;
        Ok(Value::Bytes(Rc::from(random_bytes(n)?)))
    });
    m.function_named("hex", &["bytes"], |_, values| {
        let a = Args::new("random::hex", values, 0, 1)?;
        Ok(Value::string(&hex(&random_bytes(count(&a, 0, "random::hex", 16)?)?)))
    });
    m.function_named("token", &["bytes"], |_, values| {
        let a = Args::new("random::token", values, 0, 1)?;
        let bytes = random_bytes(count(&a, 0, "random::token", 32)?)?;
        Ok(Value::string(&base64_url(&bytes)))
    });
    m.function_named("between", &["min", "max"], |_, values| {
        let a = Args::new("random::between", values, 2, 2)?;
        let (min, max) = (a.whole(0)?, a.whole(1)?);
        if min > max {
            return Err(NativeError::new(VALUE_ERROR, format!("`random::between` needs `$min` to be at most `$max`, but got {min} and {max}")));
        }
        let count = (max as i128 - min as i128 + 1) as u128;
        // Without the bias of `%`: numbers past the last whole round are drawn again
        let zone = u64::MAX as u128 - (u64::MAX as u128 + 1) % count;
        loop {
            let n = getrandom::u64().map_err(system)? as u128;
            if n <= zone {
                return Ok(Value::Int((min as i128 + (n % count) as i128) as i64));
            }
        }
    });
}

/// How many bytes, given or `default`, from 1 to 1024.
fn count(a: &Args, i: usize, what: &str, default: i64) -> Result<usize, NativeError> {
    let n = if a.values.len() > i { a.whole(i)? } else { default };
    if !(1..=1024).contains(&n) {
        return Err(NativeError::new(VALUE_ERROR, format!("`{what}` takes from 1 to 1024 bytes, not {n}")));
    }
    Ok(n as usize)
}

/// Base64 for URLs, without `=`: a token can go in a URL or a header as it is.
fn base64_url(bytes: &[u8]) -> String {
    const CHARS: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let n = (chunk[0] as u32) << 16 | (*chunk.get(1).unwrap_or(&0) as u32) << 8 | *chunk.get(2).unwrap_or(&0) as u32;
        for i in 0..chunk.len() + 1 {
            out.push(CHARS[(n >> (18 - 6 * i) & 63) as usize] as char);
        }
    }
    out
}

// ---- std/crypto/seal ----

/// The first byte of what `seal()` gives, so the format can change later.
const SEAL_VERSION: u8 = 1;
const NONCE: usize = 24;

fn key_of(a: &Args, i: usize, what: &str) -> Result<chacha20poly1305::XChaCha20Poly1305, NativeError> {
    use chacha20poly1305::KeyInit;
    let Value::Bytes(key) = a.value(i) else {
        return Err(NativeError::new(TYPE_ERROR, format!("`{what}` takes the key as bytes, from `seal::key()`, not a {}", a.value(i).type_name())));
    };
    chacha20poly1305::XChaCha20Poly1305::new_from_slice(&key)
        .map_err(|_| NativeError::new(VALUE_ERROR, format!("a key is 32 bytes, from `seal::key()`, and this one is {}", key.len())))
}

pub fn register_seal(m: &mut Exports) {
    m.function("key", |_, values| {
        Args::new("seal::key", values, 0, 0)?;
        Ok(Value::Bytes(Rc::from(random_bytes(32)?)))
    });
    m.function_named("seal", &["data", "key"], |_, values| {
        use chacha20poly1305::aead::Aead;
        let a = Args::new("seal::seal", values, 2, 2)?;
        let plain = data(&a, 0, "seal::seal")?;
        let cipher = key_of(&a, 1, "seal::seal")?;
        let nonce = random_bytes(NONCE)?;
        let nonce_ref = <&chacha20poly1305::XNonce>::try_from(&nonce[..]).expect("24 bytes");
        let sealed = cipher.encrypt(nonce_ref, plain.as_slice()).map_err(|_| NativeError::new(VALUE_ERROR, "the data couldn't be sealed"))?;
        let mut out = Vec::with_capacity(1 + NONCE + sealed.len());
        out.push(SEAL_VERSION);
        out.extend_from_slice(&nonce);
        out.extend_from_slice(&sealed);
        Ok(Value::Bytes(Rc::from(out)))
    });
    m.function_named("open", &["sealed", "key"], |_, values| {
        use chacha20poly1305::aead::Aead;
        let a = Args::new("seal::open", values, 2, 2)?;
        let Value::Bytes(sealed) = a.value(0) else {
            return Err(NativeError::new(TYPE_ERROR, format!("`seal::open` takes the bytes `seal::seal()` gave, not a {}", a.value(0).type_name())));
        };
        let cipher = key_of(&a, 1, "seal::open")?;
        let wrong = || NativeError::new(VALUE_ERROR, "these bytes can't be opened: the key is another one, or they were changed after they were sealed");
        if sealed.len() < 1 + NONCE + 16 || sealed[0] != SEAL_VERSION {
            return Err(NativeError::new(VALUE_ERROR, "these bytes weren't made by `seal::seal()`"));
        }
        let nonce = <&chacha20poly1305::XNonce>::try_from(&sealed[1..1 + NONCE]).expect("24 bytes");
        let plain = cipher.decrypt(nonce, &sealed[1 + NONCE..]).map_err(|_| wrong())?;
        Ok(Value::Bytes(Rc::from(plain)))
    });
}

// ---- std/crypto/password ----

pub fn register_password(m: &mut Exports) {
    m.function_named("hash", &["password"], |_, values| {
        use argon2::PasswordHasher;
        let a = Args::new("password::hash", values, 1, 1)?;
        let hashed = argon2::Argon2::default().hash_password(a.str(0)?.as_bytes()).map_err(|e| NativeError::new(VALUE_ERROR, format!("the password couldn't be hashed: {e}")))?;
        Ok(Value::string(&hashed.to_string()))
    });
    m.function_named("verify", &["password", "hash"], |_, values| {
        use argon2::PasswordVerifier;
        let a = Args::new("password::verify", values, 2, 2)?;
        let (password, hashed) = (a.str(0)?, a.str(1)?);
        let parsed = argon2::PasswordHash::new(&hashed).map_err(|_| NativeError::new(VALUE_ERROR, "that isn't a hash from `password::hash()`: it starts with `$argon2`"))?;
        Ok(Value::Bool(argon2::Argon2::default().verify_password(password.as_bytes(), &parsed).is_ok()))
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_values() {
        let of = |name, text: &str| {
            let mut h = Hasher::named(name).unwrap();
            h.update(text.as_bytes());
            h.hex()
        };
        assert_eq!(of("sha256", ""), "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
        assert_eq!(of("md5", "abc"), "900150983cd24fb0d6963f7d28e17f72");
        assert_eq!(of("sha1", "abc"), "a9993e364706816aba3e25717850c26c9cd0d89d");
        assert_eq!(of("crc32", "123456789"), "cbf43926");
        // RFC 4231, test case 2
        assert_eq!(hex(&hmac("sha256", b"Jefe", b"what do ya want for nothing?").unwrap()), "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843");
        assert_eq!(base64_url(&[0xfb, 0xff]), "-_8");
    }
}
