use super::*;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::mpsc;

/// Serves the given responses, one per connection, on a local port. Sends
/// each request's text (lower-cased) down the channel.
fn serve(responses: Vec<String>) -> (String, mpsc::Receiver<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/cal.ics", listener.local_addr().unwrap());
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        for response in responses {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = Vec::new();
            let mut buf = [0u8; 1024];
            while !request.windows(4).any(|w| w == b"\r\n\r\n") {
                let n = stream.read(&mut buf).unwrap();
                if n == 0 {
                    break;
                }
                request.extend_from_slice(&buf[..n]);
            }
            tx.send(String::from_utf8_lossy(&request).to_lowercase())
                .unwrap();
            stream.write_all(response.as_bytes()).unwrap();
        }
    });
    (url, rx)
}

fn ok(body: &str) -> String {
    format!(
        "HTTP/1.1 200 OK\r\nETag: \"v1\"\r\nLast-Modified: Tue, 13 Oct 2026 08:00:00 GMT\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

const NOT_MODIFIED: &str = "HTTP/1.1 304 Not Modified\r\nConnection: close\r\n\r\n";

#[test]
fn a_fetch_returns_the_body_and_its_validators() {
    let (url, _requests) = serve(vec![ok("BEGIN:VCALENDAR")]);
    let got = UreqFetcher::new().fetch(&url, None, None).unwrap();
    assert_eq!(
        got,
        Fetched::Body {
            text: "BEGIN:VCALENDAR".into(),
            etag: Some("\"v1\"".into()),
            last_modified: Some("Tue, 13 Oct 2026 08:00:00 GMT".into()),
        }
    );
}

#[test]
fn a_second_fetch_sends_the_validators_and_a_304_is_not_modified() {
    let (url, requests) = serve(vec![ok("x"), NOT_MODIFIED.to_string()]);
    let fetcher = UreqFetcher::new();
    fetcher.fetch(&url, None, None).unwrap();
    let first = requests.recv().unwrap();
    assert!(!first.contains("if-none-match"));
    let again = fetcher
        .fetch(&url, Some("\"v1\""), Some("Tue, 13 Oct 2026 08:00:00 GMT"))
        .unwrap();
    assert_eq!(again, Fetched::NotModified);
    let second = requests.recv().unwrap();
    assert!(second.contains("if-none-match: \"v1\""));
    assert!(second.contains("if-modified-since: tue, 13 oct 2026 08:00:00 gmt"));
}

#[test]
fn other_statuses_are_errors_that_do_not_name_the_url() {
    let (url, _requests) = serve(vec![
        "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_string(),
    ]);
    let err = UreqFetcher::new().fetch(&url, None, None).unwrap_err();
    assert_eq!(err, "the server answered 404");
    assert!(!err.contains("127.0.0.1"));
}

#[test]
fn a_feed_over_the_cap_is_refused() {
    let (url, _requests) = serve(vec![ok("0123456789ABCDEF")]);
    let err = UreqFetcher::with_limit(10)
        .fetch(&url, None, None)
        .unwrap_err();
    assert_eq!(err, "the feed is too large");
}

#[test]
fn a_connection_that_is_refused_is_an_error() {
    // Bind then drop, so the port is closed.
    let port = TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let url = format!("http://127.0.0.1:{port}/cal.ics");
    assert!(UreqFetcher::new().fetch(&url, None, None).is_err());
}
