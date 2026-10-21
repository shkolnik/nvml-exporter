//! Minimal HTTP/1.0 server: one connection at a time on the calling thread, a reader
//! allow-list checked before reading, and bounded request reads.

use std::collections::HashSet;
use std::io::{self, Read, Write};
use std::net::{IpAddr, Shutdown, SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use crate::catalog::Sample;
use crate::collector::Collector;
use crate::config::AllowList;
use crate::expo::render;

const POLL: Duration = Duration::from_millis(50);
const DRAIN_TIMEOUT: Duration = Duration::from_millis(100);
const DRAIN_LIMIT: usize = 4 * 1024;

pub struct Options {
    pub read_timeout: Duration,
    pub max_request: usize,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            read_timeout: Duration::from_secs(2),
            max_request: 8192,
        }
    }
}

#[derive(Debug)]
pub enum Outcome {
    Rejected,
    Served(u16),
    BadRequest,
}

pub struct Server<'a> {
    listener: TcpListener,
    allow: &'a AllowList,
    collector: &'a mut dyn Collector,
    version: &'a str,
    options: Options,
    rejected: u64,
    seen_rejected: HashSet<IpAddr>,
}

enum Request {
    Nothing,
    Bad,
    Line(String),
}

impl<'a> Server<'a> {
    pub fn bind(
        listen: SocketAddr,
        allow: &'a AllowList,
        collector: &'a mut dyn Collector,
        version: &'a str,
        options: Options,
    ) -> io::Result<Self> {
        let listener = TcpListener::bind(listen)?;
        listener.set_nonblocking(true)?;
        Ok(Self {
            listener,
            allow,
            collector,
            version,
            options,
            rejected: 0,
            seen_rejected: HashSet::new(),
        })
    }

    /// Runs one scrape outside any request, so the collector logs NVML's state at startup.
    pub fn collect_once(&mut self) {
        self.collector.collect();
    }

    /// # Panics
    /// If the OS cannot report the bound socket's address.
    #[must_use]
    pub fn local_addr(&self) -> SocketAddr {
        self.listener
            .local_addr()
            .expect("bound listener has an address")
    }

    /// Serves connections until `shutdown` is set.
    ///
    /// # Errors
    /// An accept error other than `WouldBlock` or `Interrupted`.
    pub fn run(&mut self, shutdown: &AtomicBool) -> io::Result<()> {
        while !shutdown.load(Ordering::SeqCst) {
            match self.listener.accept() {
                Ok((stream, peer)) => {
                    if stream.set_nonblocking(false).is_err()
                        || stream
                            .set_read_timeout(Some(self.options.read_timeout))
                            .is_err()
                        || stream
                            .set_write_timeout(Some(self.options.read_timeout))
                            .is_err()
                    {
                        continue;
                    }
                    self.handle(stream, peer.ip());
                }
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => thread::sleep(POLL),
                Err(e)
                    if matches!(
                        e.kind(),
                        io::ErrorKind::Interrupted | io::ErrorKind::ConnectionAborted
                    ) => {}
                Err(e) => return Err(e),
            }
        }
        Ok(())
    }

    pub fn handle(&mut self, mut stream: TcpStream, peer: IpAddr) -> Outcome {
        if !self.allow.allows(peer) {
            drop(stream);
            self.rejected += 1;
            if self.seen_rejected.insert(peer) {
                log::info!("rejected connection from {peer} (not in readers)");
            } else {
                log::debug!("rejected connection from {peer}");
            }
            return Outcome::Rejected;
        }
        let deadline = Instant::now() + self.options.read_timeout;
        let mut out = Response {
            stream: &mut stream,
            budget: self.options.read_timeout,
        };
        let line = match read_request(out.stream, deadline, self.options.max_request) {
            Request::Nothing => return Outcome::BadRequest,
            Request::Bad => {
                out.error(400, "Bad Request");
                return Outcome::BadRequest;
            }
            Request::Line(line) => line,
        };
        let Some((method, path)) = parse_request_line(&line) else {
            out.error(400, "Bad Request");
            return Outcome::BadRequest;
        };
        let code = if method != "GET" {
            out.error(405, "Method Not Allowed");
            405
        } else if path == "/metrics" {
            let mut samples = self.collector.collect().samples;
            #[allow(clippy::cast_precision_loss)] // exact below 2^53 connections
            samples.push(Sample::global(
                "nvml_exporter_rejected_connections_total",
                self.rejected as f64,
            ));
            out.send(
                200,
                "OK",
                "text/plain; version=0.0.4; charset=utf-8",
                &render(&samples),
            );
            200
        } else if path == "/" || path == "/health" {
            let up = u8::from(self.collector.collect().up);
            let body = format!("nvml-exporter {}, nvml_up {up}\n", self.version);
            out.send(200, "OK", "text/plain; charset=utf-8", &body);
            200
        } else {
            out.error(404, "Not Found");
            404
        };
        Outcome::Served(code)
    }
}

/// Reads until the end of the headers, EOF, `deadline` or `max` bytes.
fn read_request(stream: &mut TcpStream, deadline: Instant, max: usize) -> Request {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 1024];
    loop {
        let end = header_end(&buf);
        if end.map_or(buf.len(), |e| e + 4) > max {
            return Request::Bad;
        }
        if let Some(end) = end {
            return first_line(&buf[..end]);
        }
        match read_until(stream, deadline, &mut chunk) {
            Ok(0) | Err(_) => break,
            Ok(n) => buf.extend_from_slice(&chunk[..n]),
        }
    }
    // Incomplete request: answer only if a full request line arrived.
    match buf.iter().position(|&b| b == b'\n') {
        Some(end) => first_line(&buf[..end]),
        None => Request::Nothing,
    }
}

fn header_end(buf: &[u8]) -> Option<usize> {
    let crlf = buf.windows(4).position(|w| w == b"\r\n\r\n");
    let lf = buf.windows(2).position(|w| w == b"\n\n");
    match (crlf, lf) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (a, b) => a.or(b),
    }
}

fn first_line(head: &[u8]) -> Request {
    let end = head.iter().position(|&b| b == b'\n').unwrap_or(head.len());
    match std::str::from_utf8(&head[..end]) {
        Ok(line) => Request::Line(line.trim_end_matches('\r').to_owned()),
        Err(_) => Request::Bad,
    }
}

/// Splits `METHOD TARGET HTTP/x` into method and path, dropping the query string and an
/// absolute-URI scheme and authority.
#[must_use]
pub fn parse_request_line(line: &str) -> Option<(&str, &str)> {
    let mut parts = line.split(' ');
    let (method, target, version) = (parts.next()?, parts.next()?, parts.next()?);
    if parts.next().is_some() || method.is_empty() || !version.starts_with("HTTP/") {
        return None;
    }
    let target = if let Some(rest) = target
        .strip_prefix("http://")
        .or_else(|| target.strip_prefix("https://"))
    {
        rest.find('/').map_or("/", |i| &rest[i..])
    } else {
        target
    };
    let path = target.split('?').next().unwrap_or(target);
    if path.starts_with('/') {
        Some((method, path))
    } else {
        None
    }
}

/// One `read` bounded by the time left until `deadline`; a per-call socket timeout alone
/// would let a trickling peer extend the connection indefinitely.
fn read_until(stream: &mut TcpStream, deadline: Instant, buf: &mut [u8]) -> io::Result<usize> {
    let left = deadline.saturating_duration_since(Instant::now());
    if left.is_zero() {
        return Err(io::ErrorKind::TimedOut.into());
    }
    stream.set_read_timeout(Some(left))?;
    stream.read(buf)
}

/// Writes a response within an overall budget, then closes the connection.
struct Response<'s> {
    stream: &'s mut TcpStream,
    budget: Duration,
}

impl Response<'_> {
    fn error(&mut self, code: u16, reason: &str) {
        self.send(
            code,
            reason,
            "text/plain; charset=utf-8",
            &format!("{reason}\n"),
        );
    }

    fn send(&mut self, code: u16, reason: &str, content_type: &str, body: &str) {
        let head = format!(
            "HTTP/1.0 {code} {reason}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        );
        let deadline = Instant::now() + self.budget;
        let _ = self.write_all(deadline, head.as_bytes());
        let _ = self.write_all(deadline, body.as_bytes());
        let _ = self.stream.shutdown(Shutdown::Write);
        // Unread input at close makes the kernel send RST, which can destroy the response
        // before the client reads it; drain briefly so the close is orderly.
        let drain_deadline = Instant::now() + DRAIN_TIMEOUT;
        let mut chunk = [0u8; 1024];
        let mut drained = 0;
        while drained < DRAIN_LIMIT {
            match read_until(self.stream, drain_deadline, &mut chunk) {
                Ok(0) | Err(_) => break,
                Ok(n) => drained += n,
            }
        }
    }

    fn write_all(&mut self, deadline: Instant, mut buf: &[u8]) -> io::Result<()> {
        while !buf.is_empty() {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Err(io::ErrorKind::TimedOut.into());
            }
            self.stream.set_write_timeout(Some(left))?;
            match self.stream.write(buf) {
                Ok(0) => return Err(io::ErrorKind::WriteZero.into()),
                Ok(n) => buf = &buf[n..],
                Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                Err(e) => return Err(e),
            }
        }
        Ok(())
    }
}
