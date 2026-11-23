//! Writes the head of an answer to the player.

use std::io::Write;

/// The standard reason phrase of a status, or a class name for one the proxy only relays.
pub(crate) fn reason_phrase(status: u16) -> &'static str {
    match status {
        200 => "OK",
        206 => "Partial Content",
        400 => "Bad Request",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        408 => "Request Timeout",
        410 => "Gone",
        416 => "Range Not Satisfiable",
        431 => "Request Header Fields Too Large",
        502 => "Bad Gateway",
        503 => "Service Unavailable",
        other => class_name(other),
    }
}

/// The name of the class of a status (2xx, 3xx, 4xx, 5xx).
fn class_name(status: u16) -> &'static str {
    match status / 100 {
        2 => "Success",
        3 => "Redirection",
        4 => "Client Error",
        _ => "Server Error",
    }
}

/// Writes the status line and the headers; the connection is closed after every answer, so the
/// length of the body may be unknown.
pub(crate) fn write_head(
    out: &mut impl Write,
    status: u16,
    headers: &[(&str, String)],
) -> std::io::Result<()> {
    let mut head = format!("HTTP/1.1 {status} {}\r\n", reason_phrase(status));
    for (name, value) in headers {
        head.push_str(&format!("{name}: {value}\r\n"));
    }
    head.push_str("Connection: close\r\n\r\n");
    out.write_all(head.as_bytes())
}

/// Writes a complete answer with a short plain-text body.
pub(crate) fn write_message(
    out: &mut impl Write,
    status: u16,
    message: &str,
) -> std::io::Result<()> {
    let headers = [
        ("Content-Type", "text/plain; charset=utf-8".to_owned()),
        ("Content-Length", message.len().to_string()),
    ];
    write_head(out, status, &headers)?;
    out.write_all(message.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_head_writes_the_status_line_headers_and_closes_the_connection() {
        let mut out = Vec::new();
        write_head(&mut out, 206, &[("Content-Length", "5".to_owned())]).expect("writes");
        assert_eq!(
            String::from_utf8(out).expect("text"),
            "HTTP/1.1 206 Partial Content\r\nContent-Length: 5\r\nConnection: close\r\n\r\n"
        );
    }

    #[test]
    fn write_message_writes_a_complete_plain_text_answer() {
        let mut out = Vec::new();
        write_message(&mut out, 410, "gone").expect("writes");
        let text = String::from_utf8(out).expect("text");
        assert!(text.starts_with("HTTP/1.1 410 Gone\r\n"));
        assert!(text.contains("Content-Length: 4\r\n"));
        assert!(text.ends_with("\r\n\r\ngone"));
    }

    #[test]
    fn reason_phrase_names_relayed_statuses_by_class() {
        assert_eq!(reason_phrase(429), "Client Error");
        assert_eq!(reason_phrase(500), "Server Error");
        assert_eq!(reason_phrase(204), "Success");
        assert_eq!(reason_phrase(304), "Redirection");
        assert_eq!(reason_phrase(404), "Not Found");
    }
}
