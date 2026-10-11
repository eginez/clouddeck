use std::time::{Duration, SystemTime};

const THROTTLE_CAP_SECS: u64 = 60;

/// The auto-refresh interval for a number of minutes.
pub fn refresh_interval(minutes: u32) -> Duration {
    Duration::from_secs(u64::from(minutes) * 60)
}

/// Whether the last successful load is older than twice the interval.
pub fn is_stale(last_success: Option<SystemTime>, now: SystemTime, interval_minutes: u32) -> bool {
    let Some(last) = last_success else {
        return false;
    };
    match now.duration_since(last) {
        Ok(elapsed) => elapsed > refresh_interval(interval_minutes) * 2,
        Err(_) => false,
    }
}

/// Whether a timed auto-refresh should start now.
pub fn should_auto_refresh(
    last_success: Option<SystemTime>,
    now: SystemTime,
    auto_refresh: bool,
    interval_minutes: u32,
    window_active: bool,
    is_loading: bool,
) -> bool {
    if !auto_refresh || !window_active || is_loading {
        return false;
    }
    let Some(last) = last_success else {
        return true;
    };
    match now.duration_since(last) {
        Ok(elapsed) => elapsed >= refresh_interval(interval_minutes),
        Err(_) => false,
    }
}

/// Whether regaining window focus should start a refresh.
pub fn should_refresh_on_focus(
    last_success: Option<SystemTime>,
    now: SystemTime,
    refresh_on_focus: bool,
    interval_minutes: u32,
    is_loading: bool,
) -> bool {
    if !refresh_on_focus || is_loading {
        return false;
    }
    let Some(last) = last_success else {
        return false;
    };
    match now.duration_since(last) {
        Ok(elapsed) => elapsed > refresh_interval(interval_minutes),
        Err(_) => false,
    }
}

/// The delay before retrying a throttled load, doubling per attempt up to 60 seconds.
pub fn throttle_retry_delay(attempt: u32) -> Duration {
    let secs = if attempt >= 5 {
        THROTTLE_CAP_SECS
    } else {
        (2u64 << attempt).min(THROTTLE_CAP_SECS)
    };
    Duration::from_secs(secs)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t0() -> SystemTime {
        SystemTime::UNIX_EPOCH + Duration::from_secs(1_791_460_800)
    }

    fn after(base: SystemTime, secs: u64) -> SystemTime {
        base + Duration::from_secs(secs)
    }

    #[test]
    fn refresh_interval_converts_minutes_to_seconds() {
        assert_eq!(refresh_interval(5), Duration::from_secs(300));
    }

    #[test]
    fn is_stale_rules() {
        assert!(!is_stale(None, t0(), 5));
        assert!(!is_stale(Some(t0()), after(t0(), 600), 5));
        assert!(is_stale(Some(t0()), after(t0(), 601), 5));
        assert!(!is_stale(Some(after(t0(), 60)), t0(), 5));
    }

    #[test]
    fn should_auto_refresh_rules() {
        let now = t0();
        assert!(should_auto_refresh(None, now, true, 5, true, false));
        assert!(!should_auto_refresh(
            Some(t0()),
            after(t0(), 299),
            true,
            5,
            true,
            false
        ));
        assert!(should_auto_refresh(
            Some(t0()),
            after(t0(), 300),
            true,
            5,
            true,
            false
        ));
        assert!(!should_auto_refresh(
            Some(after(t0(), 60)),
            now,
            true,
            5,
            true,
            false
        ));
        assert!(!should_auto_refresh(None, now, false, 5, true, false));
        assert!(!should_auto_refresh(None, now, true, 5, false, false));
        assert!(!should_auto_refresh(None, now, true, 5, true, true));
    }

    #[test]
    fn should_refresh_on_focus_rules() {
        assert!(!should_refresh_on_focus(
            Some(t0()),
            after(t0(), 300),
            true,
            5,
            false
        ));
        assert!(should_refresh_on_focus(
            Some(t0()),
            after(t0(), 301),
            true,
            5,
            false
        ));
        assert!(!should_refresh_on_focus(
            None,
            after(t0(), 301),
            true,
            5,
            false
        ));
        assert!(!should_refresh_on_focus(
            Some(t0()),
            after(t0(), 301),
            false,
            5,
            false
        ));
        assert!(!should_refresh_on_focus(
            Some(t0()),
            after(t0(), 301),
            true,
            5,
            true
        ));
        assert!(!should_refresh_on_focus(
            Some(after(t0(), 60)),
            t0(),
            true,
            5,
            false
        ));
    }

    #[test]
    fn throttle_retry_delay_doubles_then_caps() {
        assert_eq!(throttle_retry_delay(0), Duration::from_secs(2));
        assert_eq!(throttle_retry_delay(1), Duration::from_secs(4));
        assert_eq!(throttle_retry_delay(2), Duration::from_secs(8));
        assert_eq!(throttle_retry_delay(3), Duration::from_secs(16));
        assert_eq!(throttle_retry_delay(4), Duration::from_secs(32));
        assert_eq!(throttle_retry_delay(5), Duration::from_secs(60));
        assert_eq!(throttle_retry_delay(6), Duration::from_secs(60));
        assert_eq!(throttle_retry_delay(100), Duration::from_secs(60));
        assert_eq!(throttle_retry_delay(u32::MAX), Duration::from_secs(60));
    }
}
