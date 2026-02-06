use anyhow::{Result, bail};
use chrono::Datelike;

/// A parsed version format like "YY.MM.MICRO"
pub struct VersionFormat {
    pub components: Vec<Component>,
    pub micro_index: Option<usize>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Component {
    Yyyy,  // Full year: 2026
    Yy,    // Short year: 26
    Mm,    // Month (no padding): 2
    Dd,    // Day (no padding): 5
    Micro, // Auto-incrementing counter
}

impl VersionFormat {
    /// Parse a format string like "YY.MM.MICRO"
    pub fn parse(format: &str) -> Result<Self> {
        // Only dot separator is allowed
        if format.contains('-') || format.contains('_') {
            bail!(
                "invalid format '{}': only dot (.) separator is allowed",
                format
            );
        }

        let parts: Vec<&str> = format.split('.').collect();
        if parts.is_empty() {
            bail!("invalid format '{}': empty format", format);
        }

        let mut components = Vec::new();
        let mut micro_index = None;

        for (i, part) in parts.iter().enumerate() {
            let component = match *part {
                "YYYY" => Component::Yyyy,
                "YY" => Component::Yy,
                "MM" => Component::Mm,
                "DD" => Component::Dd,
                "MICRO" => {
                    if micro_index.is_some() {
                        bail!("invalid format '{}': MICRO can only appear once", format);
                    }
                    micro_index = Some(i);
                    Component::Micro
                }
                other => bail!("invalid format '{}': unknown token '{}'", format, other),
            };
            components.push(component);
        }

        // MICRO, if present, must be the last component
        if let Some(idx) = micro_index {
            if idx != components.len() - 1 {
                bail!(
                    "invalid format '{}': MICRO must be the last component",
                    format
                );
            }
        }

        // Must have at least one date component
        let date_count = components
            .iter()
            .filter(|c| **c != Component::Micro)
            .count();
        if date_count == 0 {
            bail!(
                "invalid format '{}': must have at least one date token",
                format
            );
        }

        // No duplicate date tokens
        let date_components: Vec<_> = components
            .iter()
            .filter(|c| **c != Component::Micro)
            .collect();
        for (i, a) in date_components.iter().enumerate() {
            for b in date_components.iter().skip(i + 1) {
                if a == b {
                    bail!("invalid format '{}': duplicate date token", format);
                }
            }
        }

        Ok(Self {
            components,
            micro_index,
        })
    }

    /// Whether this format has a MICRO component (allows multiple publishes per period).
    pub fn has_micro(&self) -> bool {
        self.micro_index.is_some()
    }

    /// Compute today's date values for all components.
    fn today_values(&self) -> Vec<u64> {
        let now = chrono::Local::now();
        self.components
            .iter()
            .map(|c| match c {
                Component::Yyyy => now.year() as u64,
                Component::Yy => (now.year() % 100) as u64,
                Component::Mm => now.month() as u64,
                Component::Dd => now.day() as u64,
                Component::Micro => 0, // placeholder
            })
            .collect()
    }

    /// Build today's version string. For formats without MICRO, pads to 3 parts with .0.
    /// For formats with MICRO, uses the given micro value.
    pub fn build_version(&self, micro: u64) -> String {
        let mut values = self.today_values();
        if let Some(idx) = self.micro_index {
            values[idx] = micro;
        }

        let mut parts: Vec<String> = values.iter().map(|v| v.to_string()).collect();

        // Pad to 3 components for semver compatibility
        while parts.len() < 3 {
            parts.push("0".to_string());
        }

        parts.join(".")
    }

    /// Number of components in the format (before padding).
    fn format_len(&self) -> usize {
        self.components.len()
    }

    /// Extract component values from a version string.
    /// Returns None if the version doesn't match the format structure.
    pub fn extract_values(&self, version: &str) -> Option<Vec<u64>> {
        let parts: Vec<&str> = version.split('.').collect();

        // Accept versions with exactly format_len components,
        // or format_len + padding zeros (from our own padding to 3)
        let expected = self.format_len();
        if parts.len() < expected {
            return None;
        }

        // Check that any extra parts beyond format are zeros (padding)
        for extra in parts.iter().skip(expected) {
            if *extra != "0" {
                return None;
            }
        }

        let mut values = Vec::new();
        for (i, part) in parts.iter().take(expected).enumerate() {
            let val: u64 = part.parse().ok()?;
            values.push(val);

            // Validate date components
            if self.micro_index != Some(i) {
                match self.components[i] {
                    Component::Mm => {
                        if val < 1 || val > 12 {
                            return None;
                        }
                    }
                    Component::Dd => {
                        if val < 1 || val > 31 {
                            return None;
                        }
                    }
                    _ => {}
                }
            }
        }

        Some(values)
    }

    /// Check if a version's date parts match today's date.
    pub fn matches_today(&self, version_values: &[u64]) -> bool {
        let today = self.today_values();
        for (i, (v, t)) in version_values.iter().zip(today.iter()).enumerate() {
            if self.micro_index == Some(i) {
                continue;
            }
            if v != t {
                return false;
            }
        }
        true
    }

    /// Check if a version's date parts are ahead of today.
    pub fn ahead_of_today(&self, version_values: &[u64]) -> bool {
        let today = self.today_values();
        for (i, (v, t)) in version_values.iter().zip(today.iter()).enumerate() {
            if self.micro_index == Some(i) {
                continue;
            }
            if v > t {
                return true;
            }
            if v < t {
                return false;
            }
        }
        false
    }

    /// Get the MICRO value from parsed version values.
    pub fn micro_value(&self, version_values: &[u64]) -> Option<u64> {
        self.micro_index.map(|idx| version_values[idx])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- Parsing ---

    #[test]
    fn parse_yy_mm_micro() {
        let fmt = VersionFormat::parse("YY.MM.MICRO").unwrap();
        assert_eq!(
            fmt.components,
            vec![Component::Yy, Component::Mm, Component::Micro]
        );
        assert_eq!(fmt.micro_index, Some(2));
        assert!(fmt.has_micro());
    }

    #[test]
    fn parse_yyyy_mm_micro() {
        let fmt = VersionFormat::parse("YYYY.MM.MICRO").unwrap();
        assert_eq!(
            fmt.components,
            vec![Component::Yyyy, Component::Mm, Component::Micro]
        );
        assert_eq!(fmt.micro_index, Some(2));
    }

    #[test]
    fn parse_yy_mm_dd_micro() {
        let fmt = VersionFormat::parse("YY.MM.DD.MICRO").unwrap();
        assert_eq!(
            fmt.components,
            vec![
                Component::Yy,
                Component::Mm,
                Component::Dd,
                Component::Micro
            ]
        );
        assert_eq!(fmt.micro_index, Some(3));
    }

    #[test]
    fn parse_yy_mm_no_micro() {
        let fmt = VersionFormat::parse("YY.MM").unwrap();
        assert_eq!(fmt.components, vec![Component::Yy, Component::Mm]);
        assert_eq!(fmt.micro_index, None);
        assert!(!fmt.has_micro());
    }

    #[test]
    fn parse_yy_mm_dd_no_micro() {
        let fmt = VersionFormat::parse("YY.MM.DD").unwrap();
        assert_eq!(
            fmt.components,
            vec![Component::Yy, Component::Mm, Component::Dd]
        );
        assert!(!fmt.has_micro());
    }

    #[test]
    fn parse_error_dash_separator() {
        assert!(VersionFormat::parse("YY-MM").is_err());
    }

    #[test]
    fn parse_error_underscore_separator() {
        assert!(VersionFormat::parse("YY_MM").is_err());
    }

    #[test]
    fn parse_error_unknown_token() {
        assert!(VersionFormat::parse("YY.MM.PATCH").is_err());
    }

    #[test]
    fn parse_error_micro_not_last() {
        assert!(VersionFormat::parse("MICRO.YY.MM").is_err());
        assert!(VersionFormat::parse("YY.MICRO.MM").is_err());
    }

    #[test]
    fn parse_error_duplicate_date() {
        assert!(VersionFormat::parse("YY.YY.MICRO").is_err());
        assert!(VersionFormat::parse("MM.MM").is_err());
    }

    #[test]
    fn parse_error_micro_only() {
        assert!(VersionFormat::parse("MICRO").is_err());
    }

    #[test]
    fn parse_error_duplicate_micro() {
        assert!(VersionFormat::parse("YY.MICRO.MICRO").is_err());
    }

    // --- build_version ---

    #[test]
    fn build_version_pads_to_three() {
        let fmt = VersionFormat::parse("YY.MM").unwrap();
        let v = fmt.build_version(0);
        assert_eq!(v.split('.').count(), 3);
        assert!(v.ends_with(".0"));
    }

    #[test]
    fn build_version_with_micro() {
        let fmt = VersionFormat::parse("YY.MM.MICRO").unwrap();
        let v = fmt.build_version(5);
        let parts: Vec<&str> = v.split('.').collect();
        assert_eq!(parts.len(), 3);
        assert_eq!(parts[2], "5");
    }

    #[test]
    fn build_version_four_components_no_padding() {
        let fmt = VersionFormat::parse("YY.MM.DD.MICRO").unwrap();
        let v = fmt.build_version(0);
        assert_eq!(v.split('.').count(), 4);
    }

    // --- extract_values ---

    #[test]
    fn extract_values_matching_format() {
        let fmt = VersionFormat::parse("YY.MM.MICRO").unwrap();
        let vals = fmt.extract_values("26.2.5").unwrap();
        assert_eq!(vals, vec![26, 2, 5]);
    }

    #[test]
    fn extract_values_with_padding() {
        // YY.MM format produces "26.2.0", extract should work on padded version
        let fmt = VersionFormat::parse("YY.MM").unwrap();
        let vals = fmt.extract_values("26.2.0").unwrap();
        assert_eq!(vals, vec![26, 2]);
    }

    #[test]
    fn extract_values_rejects_nonzero_padding() {
        let fmt = VersionFormat::parse("YY.MM").unwrap();
        // "26.2.5" has non-zero padding — should be rejected for YY.MM format
        assert!(fmt.extract_values("26.2.5").is_none());
    }

    #[test]
    fn extract_values_too_few_parts() {
        let fmt = VersionFormat::parse("YY.MM.MICRO").unwrap();
        assert!(fmt.extract_values("26.2").is_none());
    }

    #[test]
    fn extract_values_invalid_month() {
        let fmt = VersionFormat::parse("YY.MM.MICRO").unwrap();
        assert!(fmt.extract_values("26.0.5").is_none()); // month 0
        assert!(fmt.extract_values("26.13.5").is_none()); // month 13
    }

    #[test]
    fn extract_values_invalid_day() {
        let fmt = VersionFormat::parse("YY.MM.DD.MICRO").unwrap();
        assert!(fmt.extract_values("26.2.0.5").is_none()); // day 0
        assert!(fmt.extract_values("26.2.32.5").is_none()); // day 32
    }

    #[test]
    fn extract_values_non_numeric() {
        let fmt = VersionFormat::parse("YY.MM.MICRO").unwrap();
        assert!(fmt.extract_values("26.abc.5").is_none());
    }

    // --- matches_today / ahead_of_today ---

    #[test]
    fn matches_today_with_micro() {
        let fmt = VersionFormat::parse("YY.MM.MICRO").unwrap();
        let now = chrono::Local::now();
        let yy = (now.year() % 100) as u64;
        let mm = now.month() as u64;

        assert!(fmt.matches_today(&[yy, mm, 999])); // micro doesn't matter
        assert!(!fmt.matches_today(&[yy, mm + 1, 0])); // wrong month
        assert!(!fmt.matches_today(&[yy + 1, mm, 0])); // wrong year
    }

    #[test]
    fn matches_today_without_micro() {
        let fmt = VersionFormat::parse("YY.MM").unwrap();
        let now = chrono::Local::now();
        let yy = (now.year() % 100) as u64;
        let mm = now.month() as u64;

        assert!(fmt.matches_today(&[yy, mm]));
        assert!(!fmt.matches_today(&[yy, mm + 1]));
    }

    #[test]
    fn ahead_of_today_future_year() {
        let fmt = VersionFormat::parse("YY.MM.MICRO").unwrap();
        let now = chrono::Local::now();
        let yy = (now.year() % 100) as u64;
        let mm = now.month() as u64;

        assert!(fmt.ahead_of_today(&[yy + 1, 1, 0]));
        assert!(!fmt.ahead_of_today(&[yy, mm, 0])); // same = not ahead
        assert!(!fmt.ahead_of_today(&[yy - 1, mm, 0])); // past
    }

    #[test]
    fn ahead_of_today_future_month() {
        let fmt = VersionFormat::parse("YY.MM.MICRO").unwrap();
        let now = chrono::Local::now();
        let yy = (now.year() % 100) as u64;
        let mm = now.month() as u64;

        if mm < 12 {
            assert!(fmt.ahead_of_today(&[yy, mm + 1, 0]));
        }
        if mm > 1 {
            assert!(!fmt.ahead_of_today(&[yy, mm - 1, 0]));
        }
    }

    // --- micro_value ---

    #[test]
    fn micro_value_present() {
        let fmt = VersionFormat::parse("YY.MM.MICRO").unwrap();
        assert_eq!(fmt.micro_value(&[26, 2, 7]), Some(7));
    }

    #[test]
    fn micro_value_absent() {
        let fmt = VersionFormat::parse("YY.MM").unwrap();
        assert_eq!(fmt.micro_value(&[26, 2]), None);
    }
}
