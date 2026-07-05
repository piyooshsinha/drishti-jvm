//! Prometheus exposition format parser.
//!
//! Parses the text output from `/actuator/prometheus` into typed metric samples.
//! This is the primary bulk scrape path — one GET returns all Micrometer metrics.

use std::collections::HashMap;

/// A single Prometheus metric sample.
#[derive(Debug, Clone)]
pub struct Sample {
    pub name: String,
    pub labels: HashMap<String, String>,
    pub value: f64,
}

/// Parse Prometheus exposition text into samples.
///
/// Filters for metric families relevant to JVM monitoring:
/// `jvm_memory_*`, `jvm_gc_*`, `jvm_threads_*`, `process_cpu_*`,
/// `http_server_requests_*`, `hikaricp_*`, `tomcat_*`, etc.
pub fn parse_prometheus_text(text: &str) -> Vec<Sample> {
    let mut samples = Vec::new();

    for line in text.lines() {
        let line = line.trim();

        // Skip comments and TYPE/HELP lines
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        if let Some(sample) = parse_line(line) {
            samples.push(sample);
        }
    }

    samples
}

/// Parse a single Prometheus exposition line.
///
/// Format: `metric_name{label1="val1",label2="val2"} value [timestamp]`
/// or:     `metric_name value [timestamp]`
fn parse_line(line: &str) -> Option<Sample> {
    let (name_and_labels, value_str) = if let Some(brace_start) = line.find('{') {
        let brace_end = line.find('}')?;
        let labels_str = &line[brace_start + 1..brace_end];
        let rest = line[brace_end + 1..].trim();
        let value_str = rest.split_whitespace().next()?;

        let name = line[..brace_start].to_string();
        let labels = parse_labels(labels_str);

        (
            Sample {
                name,
                labels,
                value: 0.0,
            },
            value_str,
        )
    } else {
        let mut parts = line.split_whitespace();
        let name = parts.next()?.to_string();
        let value_str = parts.next()?;

        (
            Sample {
                name,
                labels: HashMap::new(),
                value: 0.0,
            },
            value_str,
        )
    };

    let value = value_str.parse::<f64>().ok()?;

    Some(Sample {
        value,
        ..name_and_labels
    })
}

/// Parse `key1="val1",key2="val2"` into a HashMap.
fn parse_labels(s: &str) -> HashMap<String, String> {
    let mut labels = HashMap::new();

    // Simple parser — handles the common case. Does not handle escaped quotes.
    for pair in s.split(',') {
        let pair = pair.trim();
        if let Some(eq_pos) = pair.find('=') {
            let key = pair[..eq_pos].trim().to_string();
            let val = pair[eq_pos + 1..].trim().trim_matches('"').to_string();
            if !key.is_empty() {
                labels.insert(key, val);
            }
        }
    }

    labels
}

/// Extract a gauge value by metric name and optional label filter.
pub fn find_gauge(samples: &[Sample], name: &str, labels: &[(&str, &str)]) -> Option<f64> {
    samples.iter().find_map(|s| {
        if s.name != name {
            return None;
        }
        for (k, v) in labels {
            if s.labels.get(*k).map(|s| s.as_str()) != Some(*v) {
                return None;
            }
        }
        Some(s.value)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_simple_gauge() {
        let text = "process_cpu_usage 0.42\n";
        let samples = parse_prometheus_text(text);
        assert_eq!(samples.len(), 1);
        assert_eq!(samples[0].name, "process_cpu_usage");
        assert!((samples[0].value - 0.42).abs() < f64::EPSILON);
    }

    #[test]
    fn parse_labeled_metric() {
        let text = r#"jvm_memory_used_bytes{area="heap",id="G1 Eden Space"} 12345678.0"#;
        let samples = parse_prometheus_text(text);
        assert_eq!(samples.len(), 1);
        assert_eq!(samples[0].labels.get("area").unwrap(), "heap");
        assert_eq!(samples[0].labels.get("id").unwrap(), "G1 Eden Space");
    }

    #[test]
    fn skip_comments_and_type_lines() {
        let text = "# HELP jvm_memory_used_bytes Used bytes\n# TYPE jvm_memory_used_bytes gauge\njvm_memory_used_bytes 42.0\n";
        let samples = parse_prometheus_text(text);
        assert_eq!(samples.len(), 1);
    }

    #[test]
    fn find_gauge_with_labels() {
        let text = r#"jvm_memory_used_bytes{area="heap",id="G1 Old Gen"} 99999.0
jvm_memory_used_bytes{area="heap",id="G1 Eden Space"} 55555.0"#;
        let samples = parse_prometheus_text(text);
        let val = find_gauge(&samples, "jvm_memory_used_bytes", &[("id", "G1 Old Gen")]);
        assert!((val.unwrap() - 99999.0).abs() < f64::EPSILON);
    }
}
