//! DreamWeave versions, their two precedence schemes, and constraints, as the protocol's
//! "Versions and releases" page defines them.
//!
//! A version is one to six dot-separated release numbers with an optional SemVer pre-release and
//! build suffix. How release numbers compare is the project's choice, stated in its manifest:
//!
//! - `numeric`: integers, missing numbers are zero. `1.2 = 1.2.0 < 1.2.10`, and `0.9 < 0.82`.
//! - `decimal`: the first number is an integer; every later number compares like the digits after
//!   a decimal point, trailing zeros ignored. `0.5 = 0.50 < 0.54 < 0.6 < 0.82 < 0.9`.
//!
//! Versions of two projects are never compared, so neither are versions of two schemes.

use std::{cmp::Ordering, fmt};

use serde::{Deserialize, Serialize};

pub const MAXIMUM_RELEASE_NUMBERS: usize = 6;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Scheme {
    Numeric,
    Decimal,
}

impl Scheme {
    pub fn name(self) -> &'static str {
        match self {
            Self::Numeric => "numeric",
            Self::Decimal => "decimal",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionError(String);

impl fmt::Display for VersionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for VersionError {}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Identifier {
    Numeric(String),
    Alphanumeric(String),
}

#[derive(Debug, Clone)]
pub struct Version {
    text: String,
    scheme: Scheme,
    release: Vec<String>,
    prerelease: Vec<Identifier>,
}

impl Version {
    pub fn parse(text: &str, scheme: Scheme) -> Result<Self, VersionError> {
        let invalid = |reason: &str| {
            VersionError(format!(
                "{text:?} is not a {} DreamWeave version: {reason}",
                scheme.name()
            ))
        };
        let (without_build, build) = match text.split_once('+') {
            Some((head, build)) => (head, Some(build)),
            None => (text, None),
        };
        if let Some(build) = build
            && !identifiers_are_valid(build)
        {
            return Err(invalid(
                "build metadata must be dot-separated [0-9A-Za-z-] identifiers",
            ));
        }
        let (release_text, prerelease_text) = match without_build.split_once('-') {
            Some((head, prerelease)) => (head, Some(prerelease)),
            None => (without_build, None),
        };

        let release: Vec<String> = release_text.split('.').map(str::to_owned).collect();
        if release.len() > MAXIMUM_RELEASE_NUMBERS {
            return Err(invalid("a release has at most six numbers"));
        }
        for (position, number) in release.iter().enumerate() {
            if number.is_empty() || !number.bytes().all(|byte| byte.is_ascii_digit()) {
                return Err(invalid("release numbers are digits separated by dots"));
            }
            let integer_required = position == 0 || scheme == Scheme::Numeric;
            if integer_required && number.len() > 1 && number.starts_with('0') {
                return Err(invalid(if position == 0 {
                    "the first number has a leading zero"
                } else {
                    "a number has a leading zero, which only means something with decimal versioning"
                }));
            }
        }

        let mut prerelease = Vec::new();
        if let Some(prerelease_text) = prerelease_text {
            if !identifiers_are_valid(prerelease_text) {
                return Err(invalid(
                    "a pre-release is dot-separated [0-9A-Za-z-] identifiers",
                ));
            }
            for identifier in prerelease_text.split('.') {
                if identifier.bytes().all(|byte| byte.is_ascii_digit()) {
                    if identifier.len() > 1 && identifier.starts_with('0') {
                        return Err(invalid(
                            "a numeric pre-release identifier has a leading zero",
                        ));
                    }
                    prerelease.push(Identifier::Numeric(identifier.to_owned()));
                } else {
                    prerelease.push(Identifier::Alphanumeric(identifier.to_owned()));
                }
            }
        }

        Ok(Self {
            text: text.to_owned(),
            scheme,
            release,
            prerelease,
        })
    }

    pub fn as_str(&self) -> &str {
        &self.text
    }

    pub fn scheme(&self) -> Scheme {
        self.scheme
    }

    pub fn is_prerelease(&self) -> bool {
        !self.prerelease.is_empty()
    }

    /// Precedence order. Panics when the schemes differ: comparing across projects is a bug in
    /// the caller, not something to paper over with an arbitrary answer.
    pub fn precedence(&self, other: &Self) -> Ordering {
        assert_eq!(
            self.scheme,
            other.scheme,
            "compared a {} version with a {} version",
            self.scheme.name(),
            other.scheme.name()
        );
        self.compare_release(other)
            .then_with(|| compare_prerelease(&self.prerelease, &other.prerelease))
    }

    fn compare_release(&self, other: &Self) -> Ordering {
        match self.scheme {
            Scheme::Numeric => {
                for position in 0..MAXIMUM_RELEASE_NUMBERS {
                    let left = self.release.get(position).map_or("0", String::as_str);
                    let right = other.release.get(position).map_or("0", String::as_str);
                    let order = compare_integers(left, right);
                    if order != Ordering::Equal {
                        return order;
                    }
                }
                Ordering::Equal
            }
            Scheme::Decimal => {
                let order = compare_integers(&self.release[0], &other.release[0]);
                if order != Ordering::Equal {
                    return order;
                }
                for position in 1..MAXIMUM_RELEASE_NUMBERS {
                    let left = self
                        .release
                        .get(position)
                        .map_or("", |digits| digits.trim_end_matches('0'));
                    let right = other
                        .release
                        .get(position)
                        .map_or("", |digits| digits.trim_end_matches('0'));
                    let order = left.cmp(right);
                    if order != Ordering::Equal {
                        return order;
                    }
                }
                Ordering::Equal
            }
        }
    }
}

impl fmt::Display for Version {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.text)
    }
}

fn identifiers_are_valid(text: &str) -> bool {
    text.split('.').all(|identifier| {
        !identifier.is_empty()
            && identifier
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
    })
}

/// Compares unsigned decimal integers of any length without overflowing.
fn compare_integers(left: &str, right: &str) -> Ordering {
    let left = left.trim_start_matches('0');
    let right = right.trim_start_matches('0');
    left.len().cmp(&right.len()).then_with(|| left.cmp(right))
}

fn compare_prerelease(left: &[Identifier], right: &[Identifier]) -> Ordering {
    match (left.is_empty(), right.is_empty()) {
        (true, true) => return Ordering::Equal,
        (true, false) => return Ordering::Greater,
        (false, true) => return Ordering::Less,
        (false, false) => {}
    }
    for (left, right) in left.iter().zip(right) {
        let order = match (left, right) {
            (Identifier::Numeric(left), Identifier::Numeric(right)) => {
                compare_integers(left, right)
            }
            (Identifier::Numeric(_), Identifier::Alphanumeric(_)) => Ordering::Less,
            (Identifier::Alphanumeric(_), Identifier::Numeric(_)) => Ordering::Greater,
            (Identifier::Alphanumeric(left), Identifier::Alphanumeric(right)) => left.cmp(right),
        };
        if order != Ordering::Equal {
            return order;
        }
    }
    left.len().cmp(&right.len())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operator {
    Equal,
    NotEqual,
    Greater,
    GreaterOrEqual,
    Less,
    LessOrEqual,
}

impl Operator {
    fn symbol(self) -> &'static str {
        match self {
            Self::Equal => "=",
            Self::NotEqual => "!=",
            Self::Greater => ">",
            Self::GreaterOrEqual => ">=",
            Self::Less => "<",
            Self::LessOrEqual => "<=",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Comparator {
    pub operator: Operator,
    pub version: Version,
}

impl Comparator {
    pub fn allows(&self, version: &Version) -> bool {
        let order = version.precedence(&self.version);
        match self.operator {
            Operator::Equal => order == Ordering::Equal,
            Operator::NotEqual => order != Ordering::Equal,
            Operator::Greater => order == Ordering::Greater,
            Operator::GreaterOrEqual => order != Ordering::Less,
            Operator::Less => order == Ordering::Less,
            Operator::LessOrEqual => order != Ordering::Greater,
        }
    }
}

/// `*`, or comma-separated comparators that must all hold. No `^`, `~` or `||`.
#[derive(Debug, Clone)]
pub struct Constraint {
    pub comparators: Vec<Comparator>,
}

impl Constraint {
    pub fn parse(text: &str, scheme: Scheme) -> Result<Self, VersionError> {
        let trimmed = text.trim();
        if trimmed == "*" {
            return Ok(Self {
                comparators: Vec::new(),
            });
        }
        if trimmed.is_empty() {
            return Err(VersionError(
                "a version constraint is empty; `*` allows any version".to_owned(),
            ));
        }
        let mut comparators = Vec::new();
        for part in trimmed.split(',') {
            let part = part.trim();
            let (operator, rest) = [
                (">=", Operator::GreaterOrEqual),
                ("<=", Operator::LessOrEqual),
                ("!=", Operator::NotEqual),
                ("=", Operator::Equal),
                (">", Operator::Greater),
                ("<", Operator::Less),
            ]
            .into_iter()
            .find_map(|(symbol, operator)| {
                part.strip_prefix(symbol).map(|rest| (operator, rest))
            })
            .ok_or_else(|| {
                VersionError(format!(
                    "{text:?}: {part:?} is not a comparator (use =, !=, >, >=, < or <= before a version)"
                ))
            })?;
            let version = Version::parse(rest.trim(), scheme)
                .map_err(|error| VersionError(format!("{text:?}: {error}")))?;
            comparators.push(Comparator { operator, version });
        }
        Ok(Self { comparators })
    }

    /// Checks the grammar without knowing the target's scheme. Decimal accepts every numeric
    /// version and more, so a constraint that fails here fails under either scheme.
    pub fn check_grammar(text: &str) -> Result<(), VersionError> {
        Self::parse(text, Scheme::Decimal).map(|_| ())
    }

    pub fn allows(&self, version: &Version) -> bool {
        self.comparators
            .iter()
            .all(|comparator| comparator.allows(version))
    }
}

impl fmt::Display for Constraint {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.comparators.is_empty() {
            return formatter.write_str("*");
        }
        for (position, comparator) in self.comparators.iter().enumerate() {
            if position > 0 {
                formatter.write_str(", ")?;
            }
            write!(
                formatter,
                "{}{}",
                comparator.operator.symbol(),
                comparator.version
            )?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn numeric(text: &str) -> Version {
        Version::parse(text, Scheme::Numeric).unwrap()
    }

    fn decimal(text: &str) -> Version {
        Version::parse(text, Scheme::Decimal).unwrap()
    }

    fn assert_ascending(versions: &[Version]) {
        for pair in versions.windows(2) {
            assert_eq!(
                pair[0].precedence(&pair[1]),
                Ordering::Less,
                "{} should sort before {}",
                pair[0],
                pair[1]
            );
        }
    }

    #[test]
    fn numeric_numbers_compare_as_integers() {
        assert_ascending(&[numeric("0.9"), numeric("0.82")]);
        assert_ascending(&[numeric("1.2.9"), numeric("1.2.10")]);
        assert_eq!(
            numeric("1.2").precedence(&numeric("1.2.0")),
            Ordering::Equal
        );
        assert_eq!(
            numeric("1").precedence(&numeric("1.0.0.0.0.0")),
            Ordering::Equal
        );
    }

    #[test]
    fn integers_longer_than_a_machine_word_still_compare() {
        assert_ascending(&[
            numeric("99999999999999999999999999"),
            numeric("100000000000000000000000000"),
        ]);
    }

    #[test]
    fn prerelease_ordering_follows_semver() {
        let ordered = [
            "1.0.0-alpha",
            "1.0.0-alpha.1",
            "1.0.0-alpha.beta",
            "1.0.0-beta",
            "1.0.0-beta.2",
            "1.0.0-beta.11",
            "1.0.0-rc.1",
            "1.0.0",
        ];
        assert_ascending(&ordered.map(numeric));
    }

    #[test]
    fn build_metadata_does_not_change_precedence() {
        assert_eq!(
            numeric("1.0.0+linux").precedence(&numeric("1.0.0+windows")),
            Ordering::Equal
        );
    }

    #[test]
    fn development_builds_sort_between_releases() {
        assert_ascending(&[
            numeric("1.2.0"),
            numeric("1.2.1-dev.4"),
            numeric("1.2.1-dev.5"),
            numeric("1.2.1"),
        ]);
        assert_ascending(&[
            numeric("2.0.0-beta.1"),
            numeric("2.0.0-beta.1.dev.3"),
            numeric("2.0.0-beta.2"),
            numeric("2.0.0"),
        ]);
    }

    #[test]
    fn rejects_malformed_versions() {
        for text in [
            "",
            "v1.0",
            "1.0.",
            "01.2",
            "1.2.3-01",
            "1..2",
            "1.2.3.4.5.6.7",
            "latest",
            "1.0-",
            "1.0+",
            "1.0-beta..1",
            "0.05",
        ] {
            assert!(
                Version::parse(text, Scheme::Numeric).is_err(),
                "{text:?} parsed"
            );
        }
    }

    #[test]
    fn decimal_later_numbers_compare_like_fractions() {
        let history = [
            "0.5", "0.51", "0.52", "0.54", "0.6", "0.61", "0.63", "0.9", "0.91", "0.96", "0.961",
            "0.963", "0.97", "1.0", "1.05", "1.1",
        ];
        assert_ascending(&history.map(decimal));
    }

    #[test]
    fn decimal_trailing_zeros_do_not_count_but_leading_ones_do() {
        assert_eq!(decimal("0.5").precedence(&decimal("0.50")), Ordering::Equal);
        assert_eq!(decimal("1").precedence(&decimal("1.0")), Ordering::Equal);
        assert_ascending(&[decimal("0.05"), decimal("0.5")]);
        assert_ascending(&[decimal("9.9"), decimal("10.1")]);
    }

    #[test]
    fn decimal_development_builds_sort_before_any_successor() {
        let development = decimal("0.9631-dev.2");
        for successor in ["0.9631", "0.964", "0.97", "1.0"] {
            assert_ascending(&[decimal("0.963"), development.clone(), decimal(successor)]);
        }
        assert_ascending(&[decimal("1"), decimal("1.001-dev.1"), decimal("1.01")]);
    }

    #[test]
    #[should_panic(expected = "compared a numeric version with a decimal version")]
    fn schemes_do_not_mix() {
        let _ = numeric("0.9").precedence(&decimal("0.9"));
    }

    #[test]
    fn constraints_require_every_comparator() {
        let constraint = Constraint::parse(">=0.49, <0.51", Scheme::Numeric).unwrap();
        assert!(constraint.allows(&numeric("0.49")));
        assert!(constraint.allows(&numeric("0.50.3")));
        assert!(!constraint.allows(&numeric("0.51")));
        assert!(!constraint.allows(&numeric("0.48.9")));
        assert_eq!(constraint.to_string(), ">=0.49, <0.51");
    }

    #[test]
    fn star_allows_anything() {
        let constraint = Constraint::parse(" * ", Scheme::Numeric).unwrap();
        assert!(constraint.allows(&numeric("0.0.1-dev.1")));
        assert_eq!(constraint.to_string(), "*");
    }

    #[test]
    fn every_operator() {
        let version = numeric("1.5");
        for (text, expected) in [
            ("=1.5", true),
            ("!=1.5", false),
            ("> 1.4", true),
            (">=1.5", true),
            ("<1.5", false),
            ("<=1.5.0", true),
        ] {
            assert_eq!(
                Constraint::parse(text, Scheme::Numeric)
                    .unwrap()
                    .allows(&version),
                expected,
                "{text}"
            );
        }
    }

    #[test]
    fn constraints_use_the_target_scheme() {
        let constraint = Constraint::parse(">=0.9", Scheme::Decimal).unwrap();
        assert!(constraint.allows(&decimal("0.963")));
        assert!(!constraint.allows(&decimal("0.85")));
        let constraint = Constraint::parse(">=0.9", Scheme::Numeric).unwrap();
        assert!(constraint.allows(&numeric("0.85")));
    }

    #[test]
    fn rejects_ambiguous_constraint_grammar() {
        for text in ["^1.2", "~1.2", "1.2", ">=1 || <3", "", "2+", ">=1,"] {
            assert!(
                Constraint::parse(text, Scheme::Numeric).is_err(),
                "{text:?} parsed"
            );
        }
    }

    #[test]
    fn grammar_check_accepts_decimal_only_spellings() {
        assert!(Constraint::check_grammar(">=0.05").is_ok());
        assert!(Constraint::parse(">=0.05", Scheme::Numeric).is_err());
    }
}
