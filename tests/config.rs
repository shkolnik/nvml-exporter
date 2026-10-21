use nvml_exporter::config::{AllowList, ConfigError, parse};
use std::net::IpAddr;

fn ip(s: &str) -> IpAddr {
    s.parse().unwrap()
}

#[test]
fn config_empty_text_gives_defaults() {
    let c = parse("").unwrap();
    assert_eq!(c.listen.to_string(), "127.0.0.1:9835");
    assert!(c.readers.entries().is_empty());
    assert!(!c.collect_pcie);
}

#[test]
fn config_parses_all_keys() {
    let c = parse("listen = \"192.0.2.10:9835\"\nreaders = [\"192.0.2.1\", \"203.0.113.0/24\"]\ncollect_pcie = true\n").unwrap();
    assert_eq!(c.listen.to_string(), "192.0.2.10:9835");
    assert_eq!(c.readers.entries().len(), 2);
    assert_eq!(c.readers.entries()[0].to_string(), "192.0.2.1/32");
    assert!(c.collect_pcie);
}

#[test]
fn config_rejects_unknown_key_and_bad_reader() {
    assert!(matches!(parse("port = 9835\n"), Err(ConfigError::Toml(_))));
    match parse("readers = [\"reader-host\"]\n") {
        Err(ConfigError::Reader { entry, .. }) => assert_eq!(entry, "reader-host"),
        other => panic!("{other:?}"),
    }
    assert!(matches!(
        parse("listen = \"nope\"\n"),
        Err(ConfigError::Listen { .. })
    ));
}

#[test]
fn allows_loopback_always_and_listed_peers() {
    let a = AllowList::parse(&["192.0.2.1".into(), "198.51.100.0/24".into()]).unwrap();
    assert!(a.allows(ip("127.0.0.1")));
    assert!(a.allows(ip("::1")));
    assert!(a.allows(ip("192.0.2.1")));
    assert!(a.allows(ip("198.51.100.7")));
    assert!(!a.allows(ip("192.0.2.53")));
    assert!(!a.allows(ip("203.0.113.1")));
    assert!(!AllowList::parse(&[]).unwrap().allows(ip("192.0.2.1")));
}

#[test]
fn allows_ipv4_mapped_peer() {
    let a = AllowList::parse(&["192.0.2.1".into()]).unwrap();
    assert!(a.allows(ip("::ffff:192.0.2.1")));
    assert!(a.allows(ip("::ffff:127.0.0.1")));
}
