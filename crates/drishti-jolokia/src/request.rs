//! Jolokia request types and bulk request builder.

use serde::Serialize;

/// A single Jolokia request operation.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum JolokiaRequest {
    Read {
        mbean: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        attribute: Option<Vec<String>>,
    },
    Exec {
        mbean: String,
        operation: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        arguments: Option<Vec<serde_json::Value>>,
    },
    Search {
        mbean: String,
    },
    List {
        #[serde(skip_serializing_if = "Option::is_none")]
        path: Option<String>,
    },
}

/// Fluent builder for composing a bulk Jolokia request.
///
/// ```rust,no_run
/// use drishti_jolokia::BulkRequestBuilder;
///
/// let requests = BulkRequestBuilder::new()
///     .read("java.lang:type=Memory", &["HeapMemoryUsage", "NonHeapMemoryUsage"])
///     .read("java.lang:type=Threading", &["ThreadCount", "DaemonThreadCount"])
///     .exec_no_args("java.lang:type=Threading", "findDeadlockedThreads")
///     .build();
/// ```
#[derive(Debug, Default)]
pub struct BulkRequestBuilder {
    requests: Vec<JolokiaRequest>,
}

impl BulkRequestBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a read request for specific attributes.
    pub fn read(mut self, mbean: &str, attributes: &[&str]) -> Self {
        self.requests.push(JolokiaRequest::Read {
            mbean: mbean.to_string(),
            attribute: Some(attributes.iter().map(|s| s.to_string()).collect()),
        });
        self
    }

    /// Add a read request for all attributes of an MBean.
    pub fn read_all(mut self, mbean: &str) -> Self {
        self.requests.push(JolokiaRequest::Read {
            mbean: mbean.to_string(),
            attribute: None,
        });
        self
    }

    /// Add an exec request with no arguments.
    pub fn exec_no_args(mut self, mbean: &str, operation: &str) -> Self {
        self.requests.push(JolokiaRequest::Exec {
            mbean: mbean.to_string(),
            operation: operation.to_string(),
            arguments: None,
        });
        self
    }

    /// Add an exec request with arguments.
    pub fn exec(mut self, mbean: &str, operation: &str, args: Vec<serde_json::Value>) -> Self {
        self.requests.push(JolokiaRequest::Exec {
            mbean: mbean.to_string(),
            operation: operation.to_string(),
            arguments: Some(args),
        });
        self
    }

    /// Add a search request (MBean pattern matching).
    pub fn search(mut self, mbean_pattern: &str) -> Self {
        self.requests.push(JolokiaRequest::Search {
            mbean: mbean_pattern.to_string(),
        });
        self
    }

    /// Build the standard drishti bulk request that captures core JVM state.
    pub fn standard() -> Vec<JolokiaRequest> {
        Self::new()
            .read(
                "java.lang:type=Memory",
                &["HeapMemoryUsage", "NonHeapMemoryUsage"],
            )
            .read(
                "java.lang:type=Threading",
                &["ThreadCount", "DaemonThreadCount", "PeakThreadCount"],
            )
            .read_all("java.lang:type=GarbageCollector,name=*")
            .read(
                "java.lang:type=MemoryPool,name=*",
                &["Usage", "CollectionUsage", "Type"],
            )
            .read(
                "java.lang:type=OperatingSystem",
                &[
                    "ProcessCpuLoad",
                    "SystemCpuLoad",
                    "AvailableProcessors",
                    "TotalPhysicalMemorySize",
                    "FreePhysicalMemorySize",
                    "SystemLoadAverage",
                ],
            )
            .read(
                "java.lang:type=Runtime",
                &[
                    "Uptime",
                    "VmName",
                    "VmVendor",
                    "VmVersion",
                    "SpecVersion",
                    "InputArguments",
                ],
            )
            .read_all("java.lang:type=ClassLoading")
            .exec_no_args("java.lang:type=Threading", "findDeadlockedThreads")
            .build()
    }

    pub fn build(self) -> Vec<JolokiaRequest> {
        self.requests
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standard_request_has_8_elements() {
        let reqs = BulkRequestBuilder::standard();
        assert_eq!(reqs.len(), 8);
    }

    #[test]
    fn serializes_to_json_array() {
        let reqs = BulkRequestBuilder::new()
            .read("java.lang:type=Memory", &["HeapMemoryUsage"])
            .build();

        let json = serde_json::to_string(&reqs).unwrap();
        assert!(json.starts_with('['));
        assert!(json.contains("HeapMemoryUsage"));
    }
}
