//! A tiny HTTP server for tests: `std` only, loopback only.
//!
//! Each route maps a path to a closure that receives the hit number (1 for the
//! first request) and returns the response, so tests can simulate playlists
//! that change between refreshes.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering::SeqCst};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

pub(crate) struct Response {
    pub status: u16,
    pub content_type: String,
    pub body: Vec<u8>,
}

impl Response {
    pub(crate) fn ok(content_type: &str, body: impl Into<Vec<u8>>) -> Self {
        Self {
            status: 200,
            content_type: content_type.to_string(),
            body: body.into(),
        }
    }

    pub(crate) fn status(status: u16) -> Self {
        Self {
            status,
            content_type: "text/plain".to_string(),
            body: Vec::new(),
        }
    }
}

pub(crate) type Handler = Box<dyn Fn(u32) -> Response + Send + Sync>;

pub(crate) struct TestServer {
    addr: SocketAddr,
    stop: Arc<AtomicBool>,
    hits: Arc<Mutex<HashMap<String, u32>>>,
    thread: Option<JoinHandle<()>>,
}

impl TestServer {
    pub(crate) fn start(routes: Vec<(&str, Handler)>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
        let addr = listener.local_addr().unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let hits = Arc::new(Mutex::new(HashMap::new()));
        let routes: Arc<HashMap<String, Handler>> = Arc::new(
            routes
                .into_iter()
                .map(|(path, handler)| (path.to_string(), handler))
                .collect(),
        );

        let thread = {
            let (stop, hits) = (Arc::clone(&stop), Arc::clone(&hits));
            thread::spawn(move || {
                for stream in listener.incoming() {
                    if stop.load(SeqCst) {
                        break;
                    }
                    let Ok(stream) = stream else { continue };
                    let (routes, hits) = (Arc::clone(&routes), Arc::clone(&hits));
                    thread::spawn(move || serve(stream, &routes, &hits));
                }
            })
        };

        Self {
            addr,
            stop,
            hits,
            thread: Some(thread),
        }
    }

    pub(crate) fn url(&self, path: &str) -> String {
        format!("http://{}{path}", self.addr)
    }

    pub(crate) fn hits(&self, path: &str) -> u32 {
        *self.hits.lock().unwrap().get(path).unwrap_or(&0)
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        self.stop.store(true, SeqCst);
        // Unblock the accept loop.
        let _ = TcpStream::connect(self.addr);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// Shorthand for a fixed response.
pub(crate) fn fixed(response: impl Fn() -> Response + Send + Sync + 'static) -> Handler {
    Box::new(move |_| response())
}

fn serve(
    mut stream: TcpStream,
    routes: &HashMap<String, Handler>,
    hits: &Mutex<HashMap<String, u32>>,
) {
    let mut reader = BufReader::new(stream.try_clone().expect("clone stream"));
    let mut request_line = String::new();
    if reader.read_line(&mut request_line).is_err() {
        return;
    }
    // Drain the headers.
    loop {
        let mut line = String::new();
        match reader.read_line(&mut line) {
            Ok(0) | Err(_) => break,
            Ok(_) if line == "\r\n" || line == "\n" => break,
            Ok(_) => {}
        }
    }

    let path = request_line
        .split_whitespace()
        .nth(1)
        .unwrap_or("/")
        .split('?')
        .next()
        .unwrap_or("/")
        .to_string();
    let hit = {
        let mut hits = hits.lock().unwrap();
        let count = hits.entry(path.clone()).or_insert(0);
        *count += 1;
        *count
    };

    let response = match routes.get(&path) {
        Some(handler) => handler(hit),
        None => Response::status(404),
    };
    let head = format!(
        "HTTP/1.1 {} X\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        response.status,
        response.content_type,
        response.body.len()
    );
    let _ = stream.write_all(head.as_bytes());
    let _ = stream.write_all(&response.body);
    let _ = stream.flush();
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    fn get(url: &str) -> (u16, String, Vec<u8>) {
        let response = reqwest::blocking::get(url).unwrap();
        let status = response.status().as_u16();
        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();
        (status, content_type, response.bytes().unwrap().to_vec())
    }

    #[test]
    fn serves_routes_counts_hits_and_404s_unknown_paths() {
        let server = TestServer::start(vec![
            ("/a", fixed(|| Response::ok("text/plain", "hello"))),
            (
                "/count",
                Box::new(|hit| Response::ok("text/plain", hit.to_string())),
            ),
        ]);

        assert_eq!(
            get(&server.url("/a")),
            (200, "text/plain".to_string(), b"hello".to_vec())
        );
        assert_eq!(get(&server.url("/count")).2, b"1");
        assert_eq!(get(&server.url("/count?x=1")).2, b"2");
        assert_eq!(get(&server.url("/missing")).0, 404);
        assert_eq!(server.hits("/count"), 2);
        assert_eq!(server.hits("/never"), 0);
    }

    #[test]
    fn dropping_the_server_stops_it() {
        let server = TestServer::start(vec![]);
        let addr = server.addr;
        drop(server);

        let mut stream = TcpStream::connect_timeout(&addr, std::time::Duration::from_millis(500));
        if let Ok(stream) = stream.as_mut() {
            // A late connect may still be accepted by the OS backlog but must get no response.
            stream
                .set_read_timeout(Some(std::time::Duration::from_millis(300)))
                .unwrap();
            let mut buf = [0u8; 1];
            assert!(stream.read(&mut buf).map_or(true, |n| n == 0));
        }
    }
}
