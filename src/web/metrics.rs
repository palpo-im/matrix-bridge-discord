use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use salvo::prelude::*;

static MATRIX_MESSAGES_RECEIVED: AtomicU64 = AtomicU64::new(0);
static MATRIX_MESSAGES_SUCCESS: AtomicU64 = AtomicU64::new(0);
static MATRIX_MESSAGES_FAILED: AtomicU64 = AtomicU64::new(0);
static DISCORD_MESSAGES_RECEIVED: AtomicU64 = AtomicU64::new(0);
static DISCORD_MESSAGES_SUCCESS: AtomicU64 = AtomicU64::new(0);
static DISCORD_MESSAGES_FAILED: AtomicU64 = AtomicU64::new(0);
static CACHE_HITS: AtomicU64 = AtomicU64::new(0);
static CACHE_MISSES: AtomicU64 = AtomicU64::new(0);
static PRESENCE_QUEUE_SIZE: AtomicU64 = AtomicU64::new(0);

pub struct Metrics {
    started_at: Instant,
}

impl Default for Metrics {
    fn default() -> Self {
        Self::new()
    }
}

impl Metrics {
    pub fn new() -> Self {
        Self {
            started_at: Instant::now(),
        }
    }

    pub fn matrix_message_received() {
        MATRIX_MESSAGES_RECEIVED.fetch_add(1, Ordering::Relaxed);
    }

    pub fn matrix_message_success() {
        MATRIX_MESSAGES_SUCCESS.fetch_add(1, Ordering::Relaxed);
    }

    pub fn matrix_message_failed() {
        MATRIX_MESSAGES_FAILED.fetch_add(1, Ordering::Relaxed);
    }

    pub fn discord_message_received() {
        DISCORD_MESSAGES_RECEIVED.fetch_add(1, Ordering::Relaxed);
    }

    pub fn discord_message_success() {
        DISCORD_MESSAGES_SUCCESS.fetch_add(1, Ordering::Relaxed);
    }

    pub fn discord_message_failed() {
        DISCORD_MESSAGES_FAILED.fetch_add(1, Ordering::Relaxed);
    }

    pub fn cache_hit() {
        CACHE_HITS.fetch_add(1, Ordering::Relaxed);
    }

    pub fn cache_miss() {
        CACHE_MISSES.fetch_add(1, Ordering::Relaxed);
    }

    pub fn set_presence_queue_size(size: u64) {
        PRESENCE_QUEUE_SIZE.store(size, Ordering::Relaxed);
    }
}

pub fn format_prometheus() -> String {
    let uptime = Instant::now().elapsed().as_secs();
    let matrix_received = MATRIX_MESSAGES_RECEIVED.load(Ordering::Relaxed);
    let matrix_success = MATRIX_MESSAGES_SUCCESS.load(Ordering::Relaxed);
    let matrix_failed = MATRIX_MESSAGES_FAILED.load(Ordering::Relaxed);
    let discord_received = DISCORD_MESSAGES_RECEIVED.load(Ordering::Relaxed);
    let discord_success = DISCORD_MESSAGES_SUCCESS.load(Ordering::Relaxed);
    let discord_failed = DISCORD_MESSAGES_FAILED.load(Ordering::Relaxed);
    let cache_hits = CACHE_HITS.load(Ordering::Relaxed);
    let cache_misses = CACHE_MISSES.load(Ordering::Relaxed);
    let presence_queue = PRESENCE_QUEUE_SIZE.load(Ordering::Relaxed);

    let total_cache = cache_hits + cache_misses;
    let cache_hit_rate = if total_cache > 0 {
        (cache_hits as f64 / total_cache as f64) * 100.0
    } else {
        0.0
    };

    format!(
        r#"# HELP bridge_uptime_seconds Number of seconds the bridge has been running
# TYPE bridge_uptime_seconds gauge
bridge_uptime_seconds {}

# HELP matrix_messages_received Total number of Matrix messages received
# TYPE matrix_messages_received counter
matrix_messages_received {}

# HELP matrix_messages_success Number of Matrix messages successfully processed
# TYPE matrix_messages_success counter
matrix_messages_success {}

# HELP matrix_messages_failed Number of Matrix messages that failed to process
# TYPE matrix_messages_failed counter
matrix_messages_failed {}

# HELP discord_messages_received Total number of Discord messages received
# TYPE discord_messages_received counter
discord_messages_received {}

# HELP discord_messages_success Number of Discord messages successfully processed
# TYPE discord_messages_success counter
discord_messages_success {}

# HELP discord_messages_failed Number of Discord messages that failed to process
# TYPE discord_messages_failed counter
discord_messages_failed {}

# HELP cache_hits_total Number of cache hits
# TYPE cache_hits_total counter
cache_hits_total {}

# HELP cache_misses_total Number of cache misses
# TYPE cache_misses_total counter
cache_misses_total {}

# HELP cache_hit_rate_percent Cache hit rate as percentage
# TYPE cache_hit_rate_percent gauge
cache_hit_rate_percent {}

# HELP presence_queue_size Current size of presence queue
# TYPE presence_queue_size gauge
presence_queue_size {}
"#,
        uptime,
        matrix_received,
        matrix_success,
        matrix_failed,
        discord_received,
        discord_success,
        discord_failed,
        cache_hits,
        cache_misses,
        cache_hit_rate,
        presence_queue,
    )
}

#[handler]
pub async fn metrics_endpoint(res: &mut Response) {
    res.headers_mut()
        .insert("Content-Type", "text/plain; charset=utf-8".parse().unwrap());
    res.body(format_prometheus());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metrics_increments_counters() {
        Metrics::matrix_message_received();
        Metrics::matrix_message_success();
        Metrics::discord_message_received();
        Metrics::discord_message_failed();
        Metrics::cache_hit();
        Metrics::cache_miss();

        assert_eq!(MATRIX_MESSAGES_RECEIVED.load(Ordering::Relaxed), 1);
        assert_eq!(MATRIX_MESSAGES_SUCCESS.load(Ordering::Relaxed), 1);
        assert_eq!(DISCORD_MESSAGES_RECEIVED.load(Ordering::Relaxed), 1);
        assert_eq!(DISCORD_MESSAGES_FAILED.load(Ordering::Relaxed), 1);
        assert_eq!(CACHE_HITS.load(Ordering::Relaxed), 1);
        assert_eq!(CACHE_MISSES.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn format_prometheus_includes_all_metrics() {
        let output = format_prometheus();
        assert!(output.contains("bridge_uptime_seconds"));
        assert!(output.contains("matrix_messages_received"));
        assert!(output.contains("discord_messages_received"));
        assert!(output.contains("cache_hits_total"));
        assert!(output.contains("presence_queue_size"));
    }
}
