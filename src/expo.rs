//! Prometheus text exposition format 0.0.4.

use std::fmt::Write as _;

use crate::catalog::{CATALOG, Sample, find};

pub fn render(samples: &[Sample]) -> String {
    for s in samples {
        assert!(find(s.name).is_some(), "metric {} not in CATALOG", s.name);
    }
    let mut out = String::new();
    for def in CATALOG {
        let mut group = samples.iter().filter(|s| s.name == def.name).peekable();
        if group.peek().is_none() {
            continue;
        }
        let _ = writeln!(out, "# HELP {} {}", def.name, escape_help(def.help));
        let _ = writeln!(out, "# TYPE {} {}", def.name, def.kind.as_str());
        for s in group {
            out.push_str(s.name);
            if !s.labels.is_empty() {
                out.push('{');
                for (i, (k, v)) in s.labels.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    let _ = write!(out, "{k}=\"{}\"", escape_label(v));
                }
                out.push('}');
            }
            let _ = writeln!(out, " {}", s.value);
        }
    }
    out
}

fn escape_help(s: &str) -> String {
    s.replace('\\', "\\\\").replace('\n', "\\n")
}

fn escape_label(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}
