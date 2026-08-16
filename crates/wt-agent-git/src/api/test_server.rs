use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::thread::{self, JoinHandle};

pub(crate) struct ExpectedRequest {
    pub method: &'static str,
    pub path: &'static str,
    pub required_header: Option<(&'static str, &'static str)>,
    pub body_contains: Option<&'static str>,
    pub response_content_type: &'static str,
    pub response_body: &'static str,
}

pub(crate) fn serve(expected: Vec<ExpectedRequest>) -> (String, JoinHandle<Result<(), String>>) {
    serve_with_statuses(expected.into_iter().map(|request| (request, 200)).collect())
}

pub(crate) fn serve_one_with_status(
    expected: ExpectedRequest,
    status: u16,
) -> (String, JoinHandle<Result<(), String>>) {
    serve_with_statuses(vec![(expected, status)])
}

pub(crate) fn serve_with_statuses(
    expected: Vec<(ExpectedRequest, u16)>,
) -> (String, JoinHandle<Result<(), String>>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind fixture server");
    let address = listener.local_addr().expect("read fixture address");
    let handle = thread::spawn(move || {
        for (expected_request, response_status) in expected {
            let (stream, _) = listener
                .accept()
                .map_err(|error| format!("accept fixture request: {error}"))?;
            handle_request(stream, expected_request, response_status)?;
        }
        Ok(())
    });
    (format!("http://{address}"), handle)
}

fn handle_request(
    mut stream: TcpStream,
    expected: ExpectedRequest,
    response_status: u16,
) -> Result<(), String> {
    let mut reader = BufReader::new(
        stream
            .try_clone()
            .map_err(|error| format!("clone fixture stream: {error}"))?,
    );
    let mut request_line = String::new();
    reader
        .read_line(&mut request_line)
        .map_err(|error| format!("read fixture request line: {error}"))?;
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or_default();
    let path = parts.next().unwrap_or_default();
    if method != expected.method || path != expected.path {
        return Err(format!(
            "expected {} {}, received {method} {path}",
            expected.method, expected.path
        ));
    }

    let mut content_length = 0;
    let mut required_header_found = expected.required_header.is_none();
    loop {
        let mut header = String::new();
        reader
            .read_line(&mut header)
            .map_err(|error| format!("read fixture header: {error}"))?;
        if header == "\r\n" || header.is_empty() {
            break;
        }
        let lowercase_header = header.to_ascii_lowercase();
        if let Some(value) = lowercase_header.strip_prefix("content-length:") {
            content_length = value
                .trim()
                .parse::<usize>()
                .map_err(|error| format!("parse fixture content length: {error}"))?;
        }
        if let Some((name, value)) = expected.required_header {
            if let Some((actual_name, actual_value)) = header.split_once(':') {
                if actual_name.eq_ignore_ascii_case(name) && actual_value.trim() == value {
                    required_header_found = true;
                }
            }
        }
    }
    if !required_header_found {
        return Err("required fixture request header was not present".to_owned());
    }
    let mut body = vec![0; content_length];
    reader
        .read_exact(&mut body)
        .map_err(|error| format!("read fixture body: {error}"))?;
    let body = String::from_utf8(body).map_err(|error| format!("decode fixture body: {error}"))?;
    if let Some(fragment) = expected.body_contains {
        if !body.contains(fragment) {
            return Err(format!(
                "request body does not contain `{fragment}`: {body}"
            ));
        }
    }

    let response = format!(
        "HTTP/1.1 {response_status} Fixture\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        expected.response_content_type,
        expected.response_body.len(),
        expected.response_body
    );
    stream
        .write_all(response.as_bytes())
        .map_err(|error| format!("write fixture response: {error}"))
}
