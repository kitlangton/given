use std::fmt::Display;

use itertools::Itertools;

use super::Version;

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum VersionType {
    Major,
    Minor,
    Patch,
    PreRelease,
}

impl Display for VersionType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VersionType::Major => write!(f, "Major"),
            VersionType::Minor => write!(f, "Minor"),
            VersionType::Patch => write!(f, "Patch"),
            VersionType::PreRelease => write!(f, "PreRelease"),
        }
    }
}

impl VersionType {
    pub fn next(self) -> VersionType {
        use VersionType::*;
        match self {
            Major => Minor,
            Minor => Patch,
            Patch => PreRelease,
            PreRelease => Major,
        }
    }

    pub fn prev(self) -> VersionType {
        use VersionType::*;
        match self {
            Major => PreRelease,
            Minor => Major,
            Patch => Minor,
            PreRelease => Patch,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct UpdateOptions {
    pub major: Option<Version>,
    pub minor: Option<Version>,
    pub patch: Option<Version>,
    pub pre_release: Option<Version>,
}

impl UpdateOptions {
    pub fn new(current: &Version, available: &[Version]) -> Option<UpdateOptions> {
        match current {
            Version::SemVer { .. } => UpdateOptions::get_options_semver(current, available),
            _ => UpdateOptions::get_options_semver(&Version::new("0.0.0"), available),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.major.is_none()
            && self.minor.is_none()
            && self.patch.is_none()
            && self.pre_release.is_none()
    }

    /// Given the current version, and a list of versions, this method returns the
    /// next major, minor, patch, or pre-release version, if any, that is greater
    /// than the current version.
    pub fn get_options_semver(current: &Version, available: &[Version]) -> Option<UpdateOptions> {
        let available: Vec<&Version> = available
            .iter()
            .filter(|v| matches!(v, Version::SemVer { .. }) && *v > current)
            .sorted()
            .collect();

        let (major, minor, patch) = (current.major(), current.minor(), current.patch());

        let mut update_options = UpdateOptions::default();

        for &v in &available {
            if v.major() > major && v.pre_release().is_none() {
                update_options.major = Some(v.clone());
            } else if v.minor() > minor && v.pre_release().is_none() {
                update_options.minor = Some(v.clone());
            } else if v.patch() > patch && v.pre_release().is_none() {
                update_options.patch = Some(v.clone());
            } else if v.pre_release().is_some() {
                update_options.pre_release = Some(v.clone());
            }
        }

        // Keep only pre-release versions that are greater than any other version
        if let Some(pre_release) = &update_options.pre_release {
            if update_options
                .major
                .iter()
                .chain(update_options.minor.iter())
                .chain(update_options.patch.iter())
                .any(|v| pre_release <= v)
            {
                update_options.pre_release = None;
            }
        }

        if update_options.is_empty() {
            None
        } else {
            Some(update_options)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Version;
    use pretty_assertions::assert_eq;

    #[test]
    fn test_get_options() {
        let current = Version::new("1.2.3");
        let available = vec![
            Version::new("1.2.1"),
            Version::new("1.2.3"),
            Version::new("1.2.4"),
            Version::new("2.0.0"),
        ];

        let options = UpdateOptions::new(&current, &available).unwrap();

        assert_eq!(options.major, Some(Version::new("2.0.0")));
        assert_eq!(options.minor, None);
        assert_eq!(options.patch, Some(Version::new("1.2.4")));
        assert_eq!(options.pre_release, None);
    }

    #[test]
    fn test_get_options_with_pre_release() {
        let current = Version::new("1.2.3");
        let available = vec![
            Version::new("1.2.1"),
            Version::new("1.2.3"),
            Version::new("3.1.0-RC1"),
            Version::new("3.0.0"),
            Version::new("2.0.0"),
        ];

        let options = UpdateOptions::new(&current, &available).unwrap();

        assert_eq!(options.major, Some(Version::new("3.0.0")));
        assert_eq!(options.minor, None);
        assert_eq!(options.patch, None);
        assert_eq!(options.pre_release, Some(Version::new("3.1.0-RC1")));
    }

    #[test]
    fn test_get_options_no_updates() {
        let current = Version::new("2.2.3");
        let available = vec![
            Version::new("1.0.0"),
            Version::new("1.1.0"),
            Version::new("1.2.0"),
            Version::new("1.2.3"),
            Version::new("1.2.3-RC1"),
            Version::new("1.2.3-M1"),
            Version::new("2.0.0"),
            Version::new("2.1.0"),
            Version::new("2.2.0"),
            Version::new("2.2.3"),
            Version::new("2.2.3-RC1"),
            Version::new("2.2.3-M1"),
        ];

        let options = UpdateOptions::new(&current, &available);

        assert_eq!(options.is_none(), true);
    }

    #[test]
    fn test_get_options_mixed_versions() {
        let current = Version::new("2.1.0");
        let available = vec![
            Version::new("1.0.0"),
            Version::new("1.1.0"),
            Version::new("1.2.0"),
            Version::new("1.2.3"),
            Version::new("1.2.3-RC1"),
            Version::new("1.2.3-M1"),
            Version::new("2.0.0"),
            Version::new("2.1.0"),
            Version::new("2.1.1"),
            Version::new("2.1.1-RC1"),
            Version::new("2.1.1-M1"),
            Version::new("2.2.0"),
            Version::new("2.2.3"),
            Version::new("2.2.3-RC1"),
            Version::new("2.2.3-M1"),
            Version::new("3.0.0"),
            Version::new("3.0.0-RC1"),
            Version::new("3.1.0-M1"),
        ];

        let options = UpdateOptions::new(&current, &available).unwrap();

        assert_eq!(options.major, Some(Version::new("3.0.0")));
        assert_eq!(options.minor, Some(Version::new("2.2.3")));
        assert_eq!(options.patch, Some(Version::new("2.1.1")));
        assert_eq!(options.pre_release, Some(Version::new("3.1.0-M1")));
    }
}
