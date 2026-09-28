//! A tiny HTTP/1.1 server on 127.0.0.1 that serves whatever the test tells it to, so crawls run
//! against real sockets, real redirects and real status codes without touching the internet.
//! Tests reach it through `AddressPolicy::AllowLoopback`.

#![allow(dead_code)]

use std::{
    collections::HashMap,
    fmt::Write as _,
    io::{BufRead, BufReader, Write as _},
    net::{SocketAddr, TcpListener, TcpStream},
    sync::{Arc, Mutex},
    thread,
};

use url::Url;

#[derive(Clone)]
pub enum Route {
    Respond {
        status: u16,
        headers: Vec<(String, String)>,
        body: Vec<u8>,
    },
    /// Accept the connection and close it without answering: a site that is down.
    Hangup,
}

#[derive(Clone, Debug)]
pub struct Request {
    pub path: String,
    pub headers: Vec<(String, String)>,
}

#[derive(Clone)]
pub struct Server {
    address: SocketAddr,
    routes: Arc<Mutex<HashMap<String, Route>>>,
    requests: Arc<Mutex<Vec<Request>>>,
}

impl Server {
    pub fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind a loopback port");
        let address = listener.local_addr().unwrap();
        let server = Self {
            address,
            routes: Arc::default(),
            requests: Arc::default(),
        };
        let handler = server.clone();
        thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let handler = handler.clone();
                thread::spawn(move || handler.answer(stream));
            }
        });
        server
    }

    pub fn url(&self, path: &str) -> Url {
        Url::parse(&format!("http://{}{path}", self.address)).unwrap()
    }

    pub fn origin(&self) -> String {
        format!("http://{}", self.address)
    }

    pub fn route(&self, path: &str, route: Route) {
        self.routes.lock().unwrap().insert(path.to_owned(), route);
    }

    pub fn serve(&self, path: &str, content_type: &str, body: impl Into<Vec<u8>>) {
        self.route(
            path,
            Route::Respond {
                status: 200,
                headers: vec![("Content-Type".to_owned(), content_type.to_owned())],
                body: body.into(),
            },
        );
    }

    pub fn serve_json(&self, path: &str, body: impl Into<Vec<u8>>) {
        self.serve(path, "application/json", body);
    }

    pub fn redirect(&self, path: &str, location: &str) {
        self.route(
            path,
            Route::Respond {
                status: 302,
                headers: vec![("Location".to_owned(), location.to_owned())],
                body: Vec::new(),
            },
        );
    }

    pub fn remove(&self, path: &str) {
        self.routes.lock().unwrap().remove(path);
    }

    /// Every path from now on hangs up: the whole site is down.
    pub fn take_down(&self) -> HashMap<String, Route> {
        let mut routes = self.routes.lock().unwrap();
        let saved = routes.clone();
        for route in routes.values_mut() {
            *route = Route::Hangup;
        }
        saved
    }

    pub fn restore(&self, routes: HashMap<String, Route>) {
        *self.routes.lock().unwrap() = routes;
    }

    pub fn requests(&self) -> Vec<Request> {
        self.requests.lock().unwrap().clone()
    }

    pub fn requested_paths(&self) -> Vec<String> {
        self.requests()
            .into_iter()
            .map(|request| request.path)
            .collect()
    }

    pub fn forget_requests(&self) {
        self.requests.lock().unwrap().clear();
    }

    fn answer(&self, mut stream: TcpStream) {
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        let mut request_line = String::new();
        if reader.read_line(&mut request_line).is_err() {
            return;
        }
        let path = request_line
            .split_whitespace()
            .nth(1)
            .unwrap_or("/")
            .to_owned();
        let mut headers = Vec::new();
        loop {
            let mut line = String::new();
            if reader.read_line(&mut line).is_err() || line == "\r\n" || line.is_empty() {
                break;
            }
            if let Some((name, value)) = line.trim_end().split_once(':') {
                headers.push((name.trim().to_ascii_lowercase(), value.trim().to_owned()));
            }
        }
        self.requests.lock().unwrap().push(Request {
            path: path.clone(),
            headers,
        });
        let route = self.routes.lock().unwrap().get(&path).cloned();
        let (status, response_headers, body) = match route {
            Some(Route::Respond {
                status,
                headers,
                body,
            }) => (status, headers, body),
            Some(Route::Hangup) => return,
            None => (404, Vec::new(), b"not found".to_vec()),
        };
        let mut head = format!(
            "HTTP/1.1 {status} Fixture\r\nContent-Length: {}\r\nConnection: close\r\n",
            body.len()
        );
        for (name, value) in response_headers {
            let _ = write!(head, "{name}: {value}\r\n");
        }
        head.push_str("\r\n");
        let _ = stream.write_all(head.as_bytes());
        let _ = stream.write_all(&body);
    }
}
