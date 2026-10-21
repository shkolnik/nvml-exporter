use nvml_exporter::catalog::{CATALOG, Kind, render_docs};

#[test]
fn catalog_names_are_unique_prefixed_and_counters_end_in_total() {
    let mut seen = std::collections::HashSet::new();
    for m in CATALOG {
        assert!(m.name.starts_with("nvml_"), "{}", m.name);
        assert!(seen.insert(m.name), "duplicate {}", m.name);
        if matches!(m.kind, Kind::Counter) {
            assert!(m.name.ends_with("_total"), "{}", m.name);
        }
    }
    assert_eq!(CATALOG.len(), 23);
}

#[test]
fn docs_metrics_md_is_current() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/docs/metrics.md");
    let want = render_docs();
    if std::env::var_os("UPDATE_DOCS").is_some() {
        std::fs::write(path, &want).unwrap();
    }
    let have = std::fs::read_to_string(path).unwrap_or_default();
    assert_eq!(
        have, want,
        "docs/metrics.md is stale: run UPDATE_DOCS=1 cargo test --test catalog"
    );
}
