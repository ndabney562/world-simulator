//! Optional external-tool identity lookup shared with build-script regression tests.
use std::process::Command;
pub(crate) fn try_output(program: &str, args: &[&str]) -> Option<String> {
    let out = Command::new(program).args(args).output().ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8(out.stdout).ok()?.trim().to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn absent_or_unsuccessful_identity_tool_is_optional() {
        assert_eq!(try_output("worldsim-tool-that-does-not-exist", &[]), None);
        assert_eq!(try_output("rustc", &["--worldsim-invalid-option"]), None);
        assert!(try_output("rustc", &["--version"]).is_some());
    }
}
