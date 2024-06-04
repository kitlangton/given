use std::{cmp::Ordering, fmt::Display};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Version {
    SemVer {
        major: u32,
        minor: u32,
        patch: u32,
        pre_release: Option<PreRelease>,
    },
    Other(String),
}

impl Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Version::SemVer {
                major,
                minor,
                patch,
                pre_release,
            } => {
                if let Some(pre_release) = pre_release {
                    write!(f, "{}.{}.{}-{}", major, minor, patch, pre_release)
                } else {
                    write!(f, "{}.{}.{}", major, minor, patch)
                }
            }
            Version::Other(s) => write!(f, "{}", s),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum PreRelease {
    RC(u32),
    M(u32),
    Other(String),
}

impl Display for PreRelease {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PreRelease::RC(v) => write!(f, "RC{}", v),
            PreRelease::M(v) => write!(f, "M{}", v),
            PreRelease::Other(s) => write!(f, "{}", s),
        }
    }
}

impl Ord for PreRelease {
    fn cmp(&self, other: &Self) -> Ordering {
        use PreRelease::*;
        match (self, other) {
            (RC(v1), RC(v2)) => v1.cmp(v2),
            (M(v1), M(v2)) => v1.cmp(v2),
            (RC(_), _) => Ordering::Greater,
            (_, RC(_)) => Ordering::Less,
            (M(_), _) => Ordering::Greater,
            (_, M(_)) => Ordering::Less,
            (Other(s1), Other(s2)) => s1.cmp(s2),
        }
    }
}

impl PartialOrd for PreRelease {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Version {
    pub fn new(value: &str) -> Self {
        if let Some((major, minor, patch, pre_release)) = Self::parse_semver(value) {
            Version::SemVer {
                major,
                minor,
                patch,
                pre_release,
            }
        } else {
            Version::Other(value.to_string())
        }
    }

    pub fn major(&self) -> Option<u32> {
        match self {
            Version::SemVer { major, .. } => Some(*major),
            Version::Other(_) => None,
        }
    }

    pub fn minor(&self) -> Option<u32> {
        match self {
            Version::SemVer { minor, .. } => Some(*minor),
            Version::Other(_) => None,
        }
    }

    pub fn patch(&self) -> Option<u32> {
        match self {
            Version::SemVer { patch, .. } => Some(*patch),
            Version::Other(_) => None,
        }
    }

    pub fn pre_release(&self) -> Option<PreRelease> {
        match self {
            Version::SemVer { pre_release, .. } => pre_release.clone(),
            Version::Other(_) => None,
        }
    }

    pub fn is_pre_release(&self) -> bool {
        matches!(
            self,
            Version::SemVer {
                pre_release: Some(_),
                ..
            } | Version::Other(_)
        )
    }

    fn parse_semver(value: &str) -> Option<(u32, u32, u32, Option<PreRelease>)> {
        let parts: Vec<&str> = value.splitn(3, '.').collect();
        match parts.len() {
            2 => {
                let major = parts[0].parse().ok()?;
                let minor_parts: Vec<&str> = parts[1].splitn(2, '-').collect();
                let minor = minor_parts[0].parse().ok()?;
                let pre_release = if minor_parts.len() > 1 {
                    Some(Self::parse_pre_release(minor_parts[1]))
                } else {
                    None
                };
                Some((major, minor, 0, pre_release))
            }
            3 => {
                let major = parts[0].parse().ok()?;
                let minor = parts[1].parse().ok()?;
                let patch_parts: Vec<&str> = parts[2].splitn(2, '-').collect();
                let patch = patch_parts[0].parse().ok()?;
                let pre_release = if patch_parts.len() > 1 {
                    Some(Self::parse_pre_release(patch_parts[1]))
                } else {
                    None
                };
                Some((major, minor, patch, pre_release))
            }
            _ => None,
        }
    }

    fn parse_pre_release(value: &str) -> PreRelease {
        if let Some(num) = value.strip_prefix("RC") {
            PreRelease::RC(num.parse().unwrap_or(0))
        } else if let Some(num) = value.strip_prefix('M') {
            PreRelease::M(num.parse().unwrap_or(0))
        } else {
            PreRelease::Other(value.to_string())
        }
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> Ordering {
        match (self, other) {
            (
                Version::SemVer {
                    major: m1,
                    minor: n1,
                    patch: p1,
                    pre_release: pr1,
                },
                Version::SemVer {
                    major: m2,
                    minor: n2,
                    patch: p2,
                    pre_release: pr2,
                },
            ) => {
                let base_cmp = (m1, n1, p1).cmp(&(m2, n2, p2));
                if base_cmp == Ordering::Equal {
                    match (pr1, pr2) {
                        (Some(pr1), Some(pr2)) => pr1.cmp(pr2),
                        (Some(_), None) => Ordering::Less,
                        (None, Some(_)) => Ordering::Greater,
                        (None, None) => Ordering::Equal,
                    }
                } else {
                    base_cmp
                }
            }
            (Version::SemVer { .. }, Version::Other(_)) => Ordering::Less,
            (Version::Other(_), Version::SemVer { .. }) => Ordering::Greater,
            (Version::Other(v1), Version::Other(v2)) => v1.cmp(v2),
        }
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use itertools::Itertools;
    use pretty_assertions::assert_eq;

    #[test]
    fn test_version_parsing() {
        let parsing_expectations = vec![
            (
                "1.0.0",
                Version::SemVer {
                    major: 1,
                    minor: 0,
                    patch: 0,
                    pre_release: None,
                },
            ),
            (
                "1.0.0-RC4",
                Version::SemVer {
                    major: 1,
                    minor: 0,
                    patch: 0,
                    pre_release: Some(PreRelease::RC(4)),
                },
            ),
            (
                "2.0-RC4",
                Version::SemVer {
                    major: 2,
                    minor: 0,
                    patch: 0,
                    pre_release: Some(PreRelease::RC(4)),
                },
            ),
            (
                "1.5.5-M1",
                Version::SemVer {
                    major: 1,
                    minor: 5,
                    patch: 5,
                    pre_release: Some(PreRelease::M(1)),
                },
            ),
            ("4.5.5.5", Version::Other("4.5.5.5".to_string())),
            ("i-hate-semver", Version::Other("i-hate-semver".to_string())),
            (
                "1.1.1-alpha",
                Version::SemVer {
                    major: 1,
                    minor: 1,
                    patch: 1,
                    pre_release: Some(PreRelease::Other("alpha".to_string())),
                },
            ),
            (
                "1.1.1-alpha.5",
                Version::SemVer {
                    major: 1,
                    minor: 1,
                    patch: 1,
                    pre_release: Some(PreRelease::Other("alpha.5".to_string())),
                },
            ),
            (
                "1.1.1-beta",
                Version::SemVer {
                    major: 1,
                    minor: 1,
                    patch: 1,
                    pre_release: Some(PreRelease::Other("beta".to_string())),
                },
            ),
            (
                "1.1.1-beta.5",
                Version::SemVer {
                    major: 1,
                    minor: 1,
                    patch: 1,
                    pre_release: Some(PreRelease::Other("beta.5".to_string())),
                },
            ),
        ];

        for (input, expected) in parsing_expectations {
            assert_eq!(Version::new(input), expected);
        }
    }

    #[test]
    fn test_version_ordering() {
        let ordering_expectations = vec![
            (
                vec!["1.1.1", "2.0.0", "0.0.5"],
                vec!["0.0.5", "1.1.1", "2.0.0"],
            ),
            (
                vec![
                    "0.5.0",
                    "0.1.0-RC12",
                    "0.1.0-RC13",
                    "1.0.0",
                    "1.0.0-RC2",
                    "1.0.0-RC1",
                    "1.0.0-M1",
                    "1.0.0-alpha",
                    "1.0.0-alpha.1",
                    "2.0.0",
                    "2.1.0",
                    "2.0.1",
                ],
                vec![
                    "0.1.0-RC12",
                    "0.1.0-RC13",
                    "0.5.0",
                    "1.0.0-alpha",
                    "1.0.0-alpha.1",
                    "1.0.0-M1",
                    "1.0.0-RC1",
                    "1.0.0-RC2",
                    "1.0.0",
                    "2.0.0",
                    "2.0.1",
                    "2.1.0",
                ],
            ),
        ];

        for (input, expected) in ordering_expectations {
            let input_versions: Vec<Version> = input.iter().map(|v| Version::new(v)).collect();
            let expected_versions: Vec<Version> =
                expected.iter().map(|v| Version::new(v)).collect();
            assert_eq!(
                input_versions.into_iter().sorted().collect::<Vec<_>>(),
                expected_versions
            );
        }
    }

    #[test]
    fn test_pre_release_ordering() {
        let pre_releases = vec![
            PreRelease::Other("alpha".to_string()),
            PreRelease::Other("alpha.1".to_string()),
            PreRelease::M(1),
            PreRelease::RC(1),
            PreRelease::RC(2),
        ];

        let expected_order = vec![
            PreRelease::Other("alpha".to_string()),
            PreRelease::Other("alpha.1".to_string()),
            PreRelease::M(1),
            PreRelease::RC(1),
            PreRelease::RC(2),
        ];

        let mut sorted_pre_releases = pre_releases.clone();
        sorted_pre_releases.sort();

        assert_eq!(sorted_pre_releases, expected_order);
    }
}
