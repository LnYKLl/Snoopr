/// Seconds in an average month (365.2425 days / 12).
const SECS_PER_MONTH: i64 = 2_629_746;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum AgeFilter {
    #[default]
    All,
    Months6,
    Year1,
    Years2,
}

impl AgeFilter {
    pub const ALL: [AgeFilter; 4] = [
        AgeFilter::All,
        AgeFilter::Months6,
        AgeFilter::Year1,
        AgeFilter::Years2,
    ];

    pub fn months(self) -> u32 {
        match self {
            AgeFilter::All => 0,
            AgeFilter::Months6 => 6,
            AgeFilter::Year1 => 12,
            AgeFilter::Years2 => 24,
        }
    }
}

/// Age in whole months between `newest` and `now` (Unix seconds); 0 if `newest` is in the future.
pub fn age_months(now: i64, newest: i64) -> u32 {
    let secs = now.saturating_sub(newest);
    if secs <= 0 {
        0
    } else {
        u32::try_from(secs / SECS_PER_MONTH).unwrap_or(u32::MAX)
    }
}

/// True when the filter is active and the item is more recent than the threshold.
pub fn is_dimmed(filter: AgeFilter, months: u32) -> bool {
    filter != AgeFilter::All && months < filter.months()
}

pub fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn age_months_counts_whole_months() {
        assert_eq!(age_months(10_000_000, 10_000_000 - 3 * 2_629_746), 3);
    }

    #[test]
    fn age_months_is_zero_when_negative() {
        assert_eq!(age_months(0, 100), 0);
    }

    #[test]
    fn dimming_follows_threshold() {
        assert!(is_dimmed(AgeFilter::Year1, 11));
        assert!(!is_dimmed(AgeFilter::Year1, 12));
        assert!(!is_dimmed(AgeFilter::All, 0));
    }

    #[test]
    fn filter_months() {
        assert_eq!(AgeFilter::Years2.months(), 24);
    }
}
