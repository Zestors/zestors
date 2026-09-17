use std::time::Duration;

pub fn format_duration(d: &Duration) -> String {
    let total_secs = d.as_secs();
    let nanos = d.subsec_nanos();

    if total_secs == 0 && nanos == 0 {
        return "0s".to_string();
    }

    if total_secs >= 86400 {
        format!("{}d {}h", total_secs / 86400, (total_secs % 86400) / 3600)
    } else if total_secs >= 3600 {
        format!("{}h {}m", total_secs / 3600, (total_secs % 3600) / 60)
    } else if total_secs >= 60 {
        format!("{}m {}s", total_secs / 60, total_secs % 60)
    } else if total_secs > 0 {
        format!("{}.{:03}s", total_secs, nanos / 1_000_000)
    } else if nanos >= 1_000_000 {
        format!("{:.2}ms", nanos as f64 / 1_000_000.0)
    } else if nanos >= 1_000 {
        format!("{:.2}µs", nanos as f64 / 1_000.0)
    } else {
        format!("{}ns", nanos)
    }
}
