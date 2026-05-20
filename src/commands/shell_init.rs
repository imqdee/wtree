use crate::git::GitError;

const BASH_FUNCTION: &str = r#"wt() {
  local output
  output=$(command wt "$@")
  local exit_code=$?
  if [[ $exit_code -eq 0 && ("$1" == "switch" || "$1" == "sw" || "$*" == *"--switch"* || " $* " == *" -s "* || "$*" == "-s" || "$*" == *" -s") ]]; then
    cd "$output"
  else
    echo "$output"
    return $exit_code
  fi
}
"#;

const ZSH_FUNCTION: &str = r#"wt() {
  local output
  output=$(command wt "$@")
  local exit_code=$?
  if [[ $exit_code -eq 0 && ("$1" == "switch" || "$1" == "sw" || "$*" == *"--switch"* || " $* " == *" -s "* || "$*" == "-s" || "$*" == *" -s") ]]; then
    cd "$output"
  else
    echo "$output"
    return $exit_code
  fi
}
"#;

/// Get the shell function for a given shell type
pub fn get_shell_function(shell: &str) -> Result<&'static str, GitError> {
    match shell.to_lowercase().as_str() {
        "bash" => Ok(BASH_FUNCTION),
        "zsh" => Ok(ZSH_FUNCTION),
        _ => Err(GitError::new(format!(
            "Unsupported shell: {}. Supported shells: bash, zsh",
            shell
        ))),
    }
}

pub fn run(shell: &str) -> Result<(), Box<dyn std::error::Error>> {
    let function = get_shell_function(shell)?;
    print!("{}", function);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_shell_function_bash() {
        let result = get_shell_function("bash");
        assert!(result.is_ok());
        assert!(result.unwrap().contains("wt()"));
    }

    #[test]
    fn test_get_shell_function_zsh() {
        let result = get_shell_function("zsh");
        assert!(result.is_ok());
        assert!(result.unwrap().contains("wt()"));
    }

    #[test]
    fn test_get_shell_function_case_insensitive() {
        assert!(get_shell_function("BASH").is_ok());
        assert!(get_shell_function("Bash").is_ok());
        assert!(get_shell_function("ZSH").is_ok());
        assert!(get_shell_function("Zsh").is_ok());
    }

    #[test]
    fn test_get_shell_function_unsupported() {
        let result = get_shell_function("fish");
        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("Unsupported shell"));
    }

    #[test]
    fn test_get_shell_function_empty() {
        let result = get_shell_function("");
        assert!(result.is_err());
    }

    #[test]
    fn test_shell_function_contains_switch_detection() {
        let bash = get_shell_function("bash").unwrap();
        let zsh = get_shell_function("zsh").unwrap();

        // Both should detect switch command and its alias
        for func in [bash, zsh] {
            assert!(func.contains("switch"));
            assert!(func.contains("sw"));
            assert!(func.contains("--switch"));
            assert!(func.contains("-s"));
            assert!(func.contains("cd \"$output\""));
        }
    }
}
