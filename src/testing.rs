//! A tiny in-process HTTP server for the unit tests (no extra dependencies).

use std::collections::VecDeque;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;

use serde_json::{json, Value};

pub struct FakeResponse {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: String,
    pub drop: bool,
}

#[derive(Debug, Clone)]
pub struct Call {
    pub path: String,
    pub params: Vec<(String, String)>,
    pub headers: Vec<(String, String)>,
}

impl Call {
    pub fn param(&self, name: &str) -> Option<&str> {
        self.params
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }
}

pub fn res(status: u16, body: Option<Value>, headers: &[(&str, &str)]) -> FakeResponse {
    FakeResponse {
        status,
        headers: headers
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
        body: body
            .map(|b| b.to_string())
            .unwrap_or_else(|| "<html>bad gateway</html>".to_string()),
        drop: false,
    }
}

pub fn ok(body: Value) -> FakeResponse {
    res(200, Some(body), &[])
}

pub fn ok_with(body: Value, headers: &[(&str, &str)]) -> FakeResponse {
    res(200, Some(body), headers)
}

pub fn drop_connection() -> FakeResponse {
    FakeResponse {
        status: 0,
        headers: vec![],
        body: String::new(),
        drop: true,
    }
}

pub fn level(name: &str, items: Vec<Value>) -> FakeResponse {
    ok(
        json!({"success": true, "data": [{"type": name, "data": items}], "total": items.len(), "duration_ms": 1.0}),
    )
}

pub struct FakeServer {
    pub url: String,
    calls: Arc<Mutex<Vec<Call>>>,
}

impl FakeServer {
    pub fn start(responses: Vec<FakeResponse>) -> FakeServer {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let queue = Arc::new(Mutex::new(VecDeque::from(responses)));
        let calls = Arc::new(Mutex::new(Vec::new()));
        let (q, c) = (queue.clone(), calls.clone());
        thread::spawn(move || {
            for stream in listener.incoming() {
                match stream {
                    Ok(mut s) => handle(&mut s, &q, &c),
                    Err(_) => break,
                }
            }
        });
        FakeServer {
            url: format!("http://{}", addr),
            calls,
        }
    }

    pub fn calls(&self) -> Vec<Call> {
        self.calls.lock().unwrap().clone()
    }
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => out.push(b' '),
            b'%' if i + 2 < bytes.len() => {
                if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                    out.push(v);
                    i += 2;
                } else {
                    out.push(b'%');
                }
            }
            b => out.push(b),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn handle(stream: &mut TcpStream, queue: &Mutex<VecDeque<FakeResponse>>, calls: &Mutex<Vec<Call>>) {
    let mut buf = Vec::new();
    let mut tmp = [0u8; 4096];
    loop {
        let n = match stream.read(&mut tmp) {
            Ok(0) | Err(_) => break,
            Ok(n) => n,
        };
        buf.extend_from_slice(&tmp[..n]);
        if buf.windows(4).any(|w| w == b"\r\n\r\n") {
            break;
        }
    }
    let text = String::from_utf8_lossy(&buf).into_owned();
    let mut lines = text.split("\r\n");
    let request_line = lines.next().unwrap_or("");
    let target = request_line.split_whitespace().nth(1).unwrap_or("/");
    let (path, query) = target.split_once('?').unwrap_or((target, ""));
    let params = query
        .split('&')
        .filter(|s| !s.is_empty())
        .map(|kv| {
            let (k, v) = kv.split_once('=').unwrap_or((kv, ""));
            (percent_decode(k), percent_decode(v))
        })
        .collect();
    let headers = lines
        .take_while(|l| !l.is_empty())
        .filter_map(|l| {
            l.split_once(": ")
                .map(|(k, v)| (k.to_string(), v.to_string()))
        })
        .collect();
    calls.lock().unwrap().push(Call {
        path: path.to_string(),
        params,
        headers,
    });

    let response = queue.lock().unwrap().pop_front();
    let response = match response {
        None => res(599, Some(json!({"detail": "unexpected request"})), &[]),
        Some(r) => r,
    };
    if response.drop {
        let _ = stream.shutdown(std::net::Shutdown::Both);
        return;
    }
    let mut out = format!(
        "HTTP/1.1 {} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n",
        response.status,
        response.body.len()
    );
    for (k, v) in &response.headers {
        out.push_str(&format!("{}: {}\r\n", k, v));
    }
    out.push_str("\r\n");
    out.push_str(&response.body);
    let _ = stream.write_all(out.as_bytes());
    let _ = stream.flush();
}
