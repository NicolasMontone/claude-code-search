pub fn short_path(p: &str) -> String {
    if let Some(home) = dirs::home_dir() {
        let home = home.to_string_lossy();
        if p == home {
            return "~".into();
        }
        if let Some(rest) = p.strip_prefix(&format!("{home}/")) {
            return format!("~/{rest}");
        }
    }
    p.to_string()
}

pub fn rel_time(now: u64, then: u64) -> String {
    let d = now.saturating_sub(then);
    match d {
        0..60 => "now".into(),
        60..3_600 => format!("{}m", d / 60),
        3_600..86_400 => format!("{}h", d / 3_600),
        86_400..604_800 => format!("{}d", d / 86_400),
        604_800..2_592_000 => format!("{}w", d / 604_800),
        2_592_000..31_536_000 => format!("{}mo", d / 2_592_000),
        _ => format!("{}y", d / 31_536_000),
    }
}

pub fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}
