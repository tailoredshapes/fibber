//! The base protocol of LSP: a message is `Content-Length: N`, a blank
//! line and N bytes of JSON, both ways.

use std::io::{self, BufRead, Write};

/// The largest message accepted: a bigger one is an error, not an
/// allocation.
const MAX_BODY: usize = 64 * 1024 * 1024;

fn bad(what: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, what.to_string())
}

/// The next message body, `None` at the end of input. Headers other
/// than `Content-Length` are skipped; a malformed header is an error.
pub fn read_message<R: BufRead>(r: &mut R) -> io::Result<Option<String>> {
    let mut length = None;
    let mut first = true;
    loop {
        let mut line = String::new();
        if r.read_line(&mut line)? == 0 {
            return if first {
                Ok(None)
            } else {
                Err(bad("input ended in a header"))
            };
        }
        first = false;
        let line = line.trim_end_matches(['\r', '\n']);
        if line.is_empty() {
            break;
        }
        let Some((name, value)) = line.split_once(':') else {
            return Err(bad("malformed header"));
        };
        if name.eq_ignore_ascii_case("content-length") {
            length = Some(
                value
                    .trim()
                    .parse::<usize>()
                    .map_err(|_| bad("bad Content-Length"))?,
            );
        }
    }
    let n = length.ok_or_else(|| bad("no Content-Length"))?;
    if n > MAX_BODY {
        return Err(bad("message too large"));
    }
    let mut body = vec![0; n];
    r.read_exact(&mut body)?;
    String::from_utf8(body)
        .map(Some)
        .map_err(|_| bad("message is not UTF-8"))
}

/// Writes one framed message and flushes.
pub fn write_message<W: Write>(w: &mut W, body: &str) -> io::Result<()> {
    write!(w, "Content-Length: {}\r\n\r\n{body}", body.len())?;
    w.flush()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn framed_messages_round_trip_and_the_end_is_none() {
        let mut buf = Vec::new();
        write_message(&mut buf, "{\"a\":\"\u{e9}\"}").expect("writes");
        write_message(&mut buf, "{}").expect("writes");
        let mut r = io::Cursor::new(buf);
        assert_eq!(
            read_message(&mut r).expect("reads").as_deref(),
            Some("{\"a\":\"\u{e9}\"}")
        );
        assert_eq!(read_message(&mut r).expect("reads").as_deref(), Some("{}"));
        assert_eq!(read_message(&mut r).expect("end"), None);
    }

    #[test]
    fn broken_framing_is_an_error() {
        for bad in [
            "Content-Length: x\r\n\r\n",
            "Foo: 1\r\n\r\n{}",
            "Content-Length: 5\r\n\r\n{}",
            "Content-Length: 99999999999\r\n\r\n",
            "Content-Length: 2",
        ] {
            assert!(
                read_message(&mut io::Cursor::new(bad.as_bytes())).is_err(),
                "{bad:?}"
            );
        }
    }
}
