//! This is deliberately not a shell-safety proof engine.
//!
//! Required mode has no candidate-byte adapter for shell programs. It pauses those
//! operations rather than blessing a keyword blacklist. Only three exact shell
//! builtins that present no code-changing operation are exempt. Host aliases,
//! startup scripts, alternate tools and replaced executables still require a real
//! runtime filesystem boundary; a hook cannot certify their absence.
use serde_json::Value;

pub(crate) fn check(tool: &str, input: &Value) -> Result<(), String> {
    let field = match tool {
        "Bash" | "shell_command" => "command",
        "exec_command" => "cmd",
        _ => return Err("this shell tool has no verified host adapter".into()),
    };
    let command = input
        .get(field)
        .and_then(Value::as_str)
        .ok_or("the shell payload did not contain its canonical command field")?;
    if command.len() <= 32 && matches!(command.trim(), "pwd" | "true" | "false") {
        return Ok(());
    }
    Err("this shell command has no verified before/after candidate. Required policy pauses it as unverified, not as a proven defect. Use Write, Edit or apply_patch for code changes and Read/Grep/Glob for inspection; explicitly change the policy for operations outside this adapter. This is not an OS filesystem enforcement boundary".into())
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn exact_nonwriting_builtins_are_positive_controls() {
        for command in ["pwd", " true ", "false\n"] {
            assert!(check("Bash", &json!({"command":command})).is_ok());
        }
    }
    #[test]
    fn equivalent_bad_edits_and_unknown_shells_pause() {
        for command in [
            "python -c 'open(\"a.py\",\"w\").write(\"invented()\")'",
            "cat > a.py <<'EOF'\ninvented()\nEOF",
            "sed -i s/real/invented/ a.py",
            "git checkout -- a.py",
            "pwd; echo invented > a.py",
            "pwd > a.py",
            "$(pwd)",
            "git status",
            "bash fix.sh",
            "ls",
        ] {
            assert!(
                check("Bash", &json!({"command":command})).is_err(),
                "{command}"
            );
        }
    }
    #[test]
    fn canonical_host_shapes_and_unsupported_fields_are_distinct() {
        assert!(check("shell_command", &json!({"command":"pwd"})).is_ok());
        assert!(check("exec_command", &json!({"cmd":"pwd"})).is_ok());
        assert!(check("exec_command", &json!({"command":"pwd"})).is_err());
        assert!(check("unknown", &json!({"command":"pwd"})).is_err());
    }
}
