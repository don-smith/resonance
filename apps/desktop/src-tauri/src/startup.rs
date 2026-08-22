use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DebugProfileName(String);

impl DebugProfileName {
    pub(crate) fn parse(value: &str) -> Result<Self, StartupArgumentError> {
        let valid_length = (1..=32).contains(&value.len());
        let valid_characters = value.bytes().enumerate().all(|(index, byte)| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || (index > 0 && byte == b'-')
        });
        if valid_length && valid_characters && !value.ends_with('-') {
            Ok(Self(value.to_owned()))
        } else {
            Err(StartupArgumentError::InvalidProfileName)
        }
    }

    #[cfg(feature = "debug-local-profiles")]
    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StartupArgumentError {
    DebugProfilesDisabled,
    InvalidProfileArgument,
    InvalidProfileName,
}

impl fmt::Display for StartupArgumentError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DebugProfilesDisabled => {
                formatter.write_str("--debug-profile is unavailable in this Resonance build")
            }
            Self::InvalidProfileArgument => formatter.write_str(
                "--debug-profile must be the only application argument and include one profile name",
            ),
            Self::InvalidProfileName => formatter.write_str(
                "debug profile names must use 1-32 lowercase letters, digits, or interior hyphens",
            ),
        }
    }
}

impl std::error::Error for StartupArgumentError {}

pub(crate) fn profile_argument(
    arguments: impl IntoIterator<Item = String>,
    debug_profiles_enabled: bool,
) -> Result<Option<DebugProfileName>, StartupArgumentError> {
    let arguments: Vec<_> = arguments.into_iter().collect();
    let has_profile_option = arguments
        .iter()
        .any(|argument| argument == "--debug-profile");
    if !has_profile_option {
        return Ok(None);
    }
    if !debug_profiles_enabled {
        return Err(StartupArgumentError::DebugProfilesDisabled);
    }
    match arguments.as_slice() {
        [option, name] if option == "--debug-profile" => DebugProfileName::parse(name).map(Some),
        _ => Err(StartupArgumentError::InvalidProfileArgument),
    }
}

pub(crate) fn profile_argument_from_environment(
) -> Result<Option<DebugProfileName>, StartupArgumentError> {
    profile_argument(
        std::env::args().skip(1),
        cfg!(feature = "debug-local-profiles"),
    )
}

#[cfg(test)]
mod tests {
    use super::{profile_argument, DebugProfileName, StartupArgumentError};

    fn arguments(values: &[&str]) -> Vec<String> {
        values.iter().map(ToString::to_string).collect()
    }

    #[test]
    fn disabled_build_rejects_before_any_profile_selection() {
        assert_eq!(
            profile_argument(arguments(&["--debug-profile", "alice"]), false),
            Err(StartupArgumentError::DebugProfilesDisabled)
        );
    }

    #[test]
    fn enabled_build_accepts_only_one_valid_dedicated_argument() {
        assert_eq!(
            profile_argument(arguments(&["--debug-profile", "alice-2"]), true),
            Ok(Some(
                DebugProfileName::parse("alice-2").expect("profile name is valid")
            ))
        );
        assert_eq!(
            profile_argument(arguments(&["--debug-profile"]), true),
            Err(StartupArgumentError::InvalidProfileArgument)
        );
        assert_eq!(
            profile_argument(arguments(&["--debug-profile", "../alice"]), true),
            Err(StartupArgumentError::InvalidProfileName)
        );
    }
}
