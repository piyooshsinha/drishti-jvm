//! GC log file tailer — watches a GC log file and emits parsed events.
//!
//! Uses tokio::fs polling (not inotify) for maximum portability.
//! Handles log rotation by detecting file truncation/replacement.

use crate::{detect_algorithm, g1, shenandoah, zgc};
use drishti_core::model::{GcAlgorithm, GcEvent};
use std::path::Path;
use tokio::io::{AsyncBufReadExt, AsyncSeekExt, BufReader};
use tokio::sync::mpsc;
use tracing;

/// Tail a GC log file and send parsed events through the channel.
///
/// Auto-detects the GC algorithm from the first few lines.
/// Handles log rotation (file shrinks or inode changes).
pub async fn tail_gc_log(
    path: &Path,
    tx: mpsc::Sender<GcEvent>,
    cancel: tokio_util::sync::CancellationToken,
) -> Result<(), crate::GcLogError> {
    let mut pos: u64 = 0;
    let mut algorithm = GcAlgorithm::Unknown;
    let mut detect_lines: Vec<String> = Vec::new();
    let poll_interval = std::time::Duration::from_millis(500);

    // Start from end of file if it already exists
    if let Ok(meta) = tokio::fs::metadata(path).await {
        pos = meta.len();
    }

    loop {
        tokio::select! {
            _ = cancel.cancelled() => break,
            _ = tokio::time::sleep(poll_interval) => {}
        }

        // Check if file exists
        let meta = match tokio::fs::metadata(path).await {
            Ok(m) => m,
            Err(_) => continue, // File doesn't exist yet, keep polling
        };

        let current_len = meta.len();

        // Detect rotation: file got smaller
        if current_len < pos {
            tracing::info!("GC log rotated, resetting position");
            pos = 0;
            algorithm = GcAlgorithm::Unknown;
            detect_lines.clear();
        }

        // No new data
        if current_len == pos {
            continue;
        }

        // Read new lines
        let file = match tokio::fs::File::open(path).await {
            Ok(f) => f,
            Err(e) => {
                tracing::warn!("Failed to open GC log: {}", e);
                continue;
            }
        };

        let mut reader = BufReader::new(file);
        if pos > 0 {
            if let Err(e) = reader.seek(std::io::SeekFrom::Start(pos)).await {
                tracing::warn!("Failed to seek in GC log: {}", e);
                continue;
            }
        }

        let mut line = String::new();
        loop {
            line.clear();
            match reader.read_line(&mut line).await {
                Ok(0) => break, // EOF
                Ok(n) => {
                    pos += n as u64;
                    let trimmed = line.trim();
                    if trimmed.is_empty() {
                        continue;
                    }

                    // Auto-detect algorithm from first 20 lines
                    if algorithm == GcAlgorithm::Unknown {
                        detect_lines.push(trimmed.to_string());
                        if detect_lines.len() >= 20 {
                            let refs: Vec<&str> = detect_lines.iter().map(|s| s.as_str()).collect();
                            algorithm = detect_algorithm(&refs);
                            tracing::info!("Detected GC algorithm: {:?}", algorithm);
                        }
                    }

                    // Parse event based on detected algorithm
                    let event = match algorithm {
                        GcAlgorithm::G1 => g1::parse_g1_event(trimmed),
                        GcAlgorithm::Zgc | GcAlgorithm::ZgcGenerational => {
                            zgc::parse_zgc_event(trimmed)
                        }
                        GcAlgorithm::Shenandoah => shenandoah::parse_shenandoah_event(trimmed),
                        _ => {
                            // Try all parsers
                            g1::parse_g1_event(trimmed)
                                .or_else(|| zgc::parse_zgc_event(trimmed))
                                .or_else(|| shenandoah::parse_shenandoah_event(trimmed))
                        }
                    };

                    if let Some(ev) = event {
                        if tx.send(ev).await.is_err() {
                            return Ok(()); // Receiver dropped
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!("Error reading GC log: {}", e);
                    break;
                }
            }
        }
    }

    Ok(())
}
