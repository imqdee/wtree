use crate::git::GitError;

const BASH_FUNCTION: &str = r#"wt() {
  local output
  output=$(command wt "$@")
  local exit_code=$?
  if [[ $exit_code -eq 0 && "$1" == "switch" ]]; then
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
  if [[ $exit_code -eq 0 && "$1" == "switch" ]]; then
    cd "$output"
  else
    echo "$output"
    return $exit_code
  fi
}
"#;

pub fn run(shell: &str) -> Result<(), Box<dyn std::error::Error>> {
    let function = match shell.to_lowercase().as_str() {
        "bash" => BASH_FUNCTION,
        "zsh" => ZSH_FUNCTION,
        _ => {
            return Err(Box::new(GitError::new(format!(
                "Unsupported shell: {}. Supported shells: bash, zsh",
                shell
            ))));
        }
    };

    print!("{}", function);
    Ok(())
}
