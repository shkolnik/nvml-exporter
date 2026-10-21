mod common;
use common::FakeCollector;
use nvml_exporter::{
    catalog::Sample,
    config::AllowList,
    http::{Options, Outcome, Server, parse_request_line},
};
use std::{
    io::{Read, Write},
    net::{IpAddr, TcpListener, TcpStream},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    thread,
    time::Duration,
};

/// A connected pair: the server-side stream (as if accepted by the Server) and the client.
fn pair() -> (TcpStream, TcpStream) {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let client = TcpStream::connect(l.local_addr().unwrap()).unwrap();
    (l.accept().unwrap().0, client)
}

fn request(addr: std::net::SocketAddr, raw: &str) -> String {
    let mut s = TcpStream::connect(addr).unwrap();
    s.write_all(raw.as_bytes()).unwrap();
    let mut out = String::new();
    s.read_to_string(&mut out).unwrap();
    out
}

fn spawn(
    allow: AllowList,
    options: Options,
) -> (
    std::net::SocketAddr,
    Arc<AtomicBool>,
    thread::JoinHandle<()>,
) {
    let stop = Arc::new(AtomicBool::new(false));
    let stop2 = stop.clone();
    let (tx, rx) = std::sync::mpsc::channel();
    let h = thread::spawn(move || {
        let mut c = FakeCollector {
            up: true,
            samples: vec![Sample::gpu("nvml_gpu_temperature_celsius", 0, 36.0)],
            calls: Arc::new(AtomicUsize::new(0)),
        };
        let mut srv = Server::bind(
            "127.0.0.1:0".parse().unwrap(),
            &allow,
            &mut c,
            "0.1.0",
            options,
        )
        .unwrap();
        tx.send(srv.local_addr()).unwrap();
        srv.run(&stop2).unwrap();
    });
    (rx.recv().unwrap(), stop, h)
}

#[test]
fn http_routes_and_status_codes() {
    let (addr, stop, h) = spawn(AllowList::parse(&[]).unwrap(), Options::default());
    let m = request(addr, "GET /metrics HTTP/1.1\r\nHost: x\r\n\r\n");
    assert!(
        m.starts_with("HTTP/1.0 200 OK\r\n")
            && m.contains("Content-Type: text/plain; version=0.0.4")
            && m.contains("\r\n\r\n# HELP nvml_up")
            && m.contains("nvml_gpu_temperature_celsius{gpu=\"0\"} 36\n")
    );
    for path in ["/", "/health"] {
        let r = request(addr, &format!("GET {path} HTTP/1.0\r\n\r\n"));
        assert!(
            r.starts_with("HTTP/1.0 200 OK\r\n")
                && r.ends_with("\r\n\r\nnvml-exporter 0.1.0, nvml_up 1\n"),
            "{path}: {r}"
        );
    }
    assert!(request(addr, "GET /nope HTTP/1.0\r\n\r\n").starts_with("HTTP/1.0 404 Not Found\r\n"));
    assert!(
        request(addr, "POST /metrics HTTP/1.0\r\n\r\n")
            .starts_with("HTTP/1.0 405 Method Not Allowed\r\n")
    );
    assert!(request(addr, "garbage\r\n\r\n").starts_with("HTTP/1.0 400 Bad Request\r\n"));
    stop.store(true, Ordering::SeqCst);
    h.join().unwrap();
}

#[test]
fn http_matches_path_ignoring_query_and_authority() {
    assert_eq!(
        parse_request_line("GET /metrics?x=1 HTTP/1.1"),
        Some(("GET", "/metrics"))
    );
    assert_eq!(
        parse_request_line("GET http://198.51.100.1:9835/metrics HTTP/1.1"),
        Some(("GET", "/metrics"))
    );
    assert_eq!(parse_request_line("GET /metrics"), None);
    assert_eq!(parse_request_line(""), None);
}

#[test]
fn http_idle_client_is_dropped_after_timeout() {
    let (addr, stop, h) = spawn(
        AllowList::parse(&[]).unwrap(),
        Options {
            read_timeout: Duration::from_millis(200),
            max_request: 8192,
        },
    );
    let mut idle = TcpStream::connect(addr).unwrap(); // sends nothing
    let started = std::time::Instant::now();
    let r = request(addr, "GET /health HTTP/1.0\r\n\r\n"); // must be served after the idle one times out
    assert!(r.starts_with("HTTP/1.0 200 OK"));
    assert!(started.elapsed() < Duration::from_secs(2));
    let mut buf = [0u8; 1];
    assert_eq!(idle.read(&mut buf).unwrap(), 0); // server closed it
    stop.store(true, Ordering::SeqCst);
    h.join().unwrap();
}

#[test]
fn http_oversized_request_is_bad_request() {
    let (addr, stop, h) = spawn(
        AllowList::parse(&[]).unwrap(),
        Options {
            read_timeout: Duration::from_secs(2),
            max_request: 64,
        },
    );
    let r = request(
        addr,
        &format!("GET /metrics HTTP/1.0\r\nX: {}\r\n\r\n", "a".repeat(100)),
    );
    assert!(r.starts_with("HTTP/1.0 400 Bad Request\r\n"));
    stop.store(true, Ordering::SeqCst);
    h.join().unwrap();
}

#[test]
fn http_rejects_unlisted_peer_before_reading_and_counts_it() {
    let allow = AllowList::parse(&["192.0.2.1".into()]).unwrap();
    let mut c = FakeCollector {
        up: true,
        samples: vec![],
        calls: Arc::new(AtomicUsize::new(0)),
    };
    let mut srv = Server::bind(
        "127.0.0.1:0".parse().unwrap(),
        &allow,
        &mut c,
        "0.1.0",
        Options::default(),
    )
    .unwrap();

    let (accepted, mut client) = pair();
    assert!(matches!(
        srv.handle(accepted, "192.0.2.53".parse::<IpAddr>().unwrap()),
        Outcome::Rejected
    ));
    let mut buf = [0u8; 1];
    assert_eq!(client.read(&mut buf).unwrap_or(0), 0); // closed, nothing written

    let (accepted, mut client) = pair();
    client.write_all(b"GET /metrics HTTP/1.0\r\n\r\n").unwrap();
    assert!(matches!(
        srv.handle(accepted, "127.0.0.1".parse::<IpAddr>().unwrap()),
        Outcome::Served(200)
    ));
    let mut out = String::new();
    client.read_to_string(&mut out).unwrap();
    assert!(out.contains("nvml_exporter_rejected_connections_total 1\n"));

    let (accepted, mut client) = pair();
    client.write_all(b"GET /health HTTP/1.0\r\n\r\n").unwrap();
    assert!(matches!(
        srv.handle(accepted, "192.0.2.1".parse::<IpAddr>().unwrap()),
        Outcome::Served(200)
    ));
}

#[test]
fn http_health_and_metrics_each_scrape_once() {
    let calls = Arc::new(AtomicUsize::new(0));
    let calls2 = calls.clone();
    let stop = Arc::new(AtomicBool::new(false));
    let stop2 = stop.clone();
    let (tx, rx) = std::sync::mpsc::channel();
    let allow = AllowList::parse(&[]).unwrap();
    let h = thread::spawn(move || {
        let mut c = FakeCollector {
            up: false,
            samples: vec![],
            calls: calls2,
        };
        let mut srv = Server::bind(
            "127.0.0.1:0".parse().unwrap(),
            &allow,
            &mut c,
            "0.1.0",
            Options::default(),
        )
        .unwrap();
        tx.send(srv.local_addr()).unwrap();
        srv.run(&stop2).unwrap();
    });
    let addr = rx.recv().unwrap();
    let r = request(addr, "GET /health HTTP/1.0\r\n\r\n");
    assert!(r.ends_with("nvml-exporter 0.1.0, nvml_up 0\n"));
    let _ = request(addr, "GET /metrics HTTP/1.0\r\n\r\n");
    let _ = request(addr, "GET /nope HTTP/1.0\r\n\r\n"); // no scrape for a 404
    stop.store(true, Ordering::SeqCst);
    h.join().unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}

/// Connects, then writes one byte every `every` from a background thread until the peer closes.
fn trickle(addr: std::net::SocketAddr, prefix: &[u8], every: Duration) -> thread::JoinHandle<()> {
    let mut s = TcpStream::connect(addr).unwrap();
    s.write_all(prefix).unwrap();
    thread::spawn(move || {
        for _ in 0..30 {
            thread::sleep(every);
            if s.write_all(b"a").is_err() {
                return;
            }
        }
    })
}

#[test]
fn http_trickling_request_is_dropped_at_overall_deadline() {
    let (addr, stop, h) = spawn(
        AllowList::parse(&[]).unwrap(),
        Options {
            read_timeout: Duration::from_millis(300),
            max_request: 8192,
        },
    );
    let started = std::time::Instant::now();
    let t = trickle(addr, b"GET /", Duration::from_millis(100));
    let r = request(addr, "GET /health HTTP/1.0\r\n\r\n");
    assert!(r.starts_with("HTTP/1.0 200 OK"));
    assert!(
        started.elapsed() < Duration::from_millis(800),
        "{:?}",
        started.elapsed()
    );
    stop.store(true, Ordering::SeqCst);
    h.join().unwrap();
    t.join().unwrap();
}

#[test]
fn http_trickling_after_request_is_drained_briefly() {
    let (addr, stop, h) = spawn(
        AllowList::parse(&[]).unwrap(),
        Options {
            read_timeout: Duration::from_millis(300),
            max_request: 8192,
        },
    );
    let started = std::time::Instant::now();
    let t = trickle(
        addr,
        b"GET /health HTTP/1.0\r\n\r\n",
        Duration::from_millis(30),
    );
    let r = request(addr, "GET /health HTTP/1.0\r\n\r\n");
    assert!(r.starts_with("HTTP/1.0 200 OK"));
    assert!(
        started.elapsed() < Duration::from_millis(600),
        "{:?}",
        started.elapsed()
    );
    stop.store(true, Ordering::SeqCst);
    h.join().unwrap();
    t.join().unwrap();
}
