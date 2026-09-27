#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    Doctor,
    Models,
    Recommend,
    Start,
    Status,
    Chat,
    Stop,
    Run,
    Help,
    Version,
    Unknown,
}

pub fn parse(args: &[String]) -> Command {
    match args {
        [] => Command::Help,
        [arg] => match arg.as_str() {
            "--help" | "help" => Command::Help,
            "--version" | "version" => Command::Version,
            "doctor" => Command::Doctor,
            "models" => Command::Models,
            "recommend" => Command::Recommend,
            "start" => Command::Start,
            "status" => Command::Status,
            "chat" => Command::Chat,
            "stop" => Command::Stop,
            "run" => Command::Run,
            _ => Command::Unknown,
        },
        _ => Command::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::{Command, parse};

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| value.to_string()).collect()
    }

    #[test]
    fn no_args_is_help() {
        assert_eq!(parse(&[]), Command::Help);
    }

    #[test]
    fn help_flag_is_help() {
        assert_eq!(parse(&args(&["--help"])), Command::Help);
        assert_eq!(parse(&args(&["help"])), Command::Help);
    }

    #[test]
    fn version_flag_is_version() {
        assert_eq!(parse(&args(&["--version"])), Command::Version);
        assert_eq!(parse(&args(&["version"])), Command::Version);
    }

    #[test]
    fn parses_recognized_commands() {
        assert_eq!(parse(&args(&["doctor"])), Command::Doctor);
        assert_eq!(parse(&args(&["models"])), Command::Models);
        assert_eq!(parse(&args(&["recommend"])), Command::Recommend);
        assert_eq!(parse(&args(&["start"])), Command::Start);
        assert_eq!(parse(&args(&["status"])), Command::Status);
        assert_eq!(parse(&args(&["chat"])), Command::Chat);
        assert_eq!(parse(&args(&["stop"])), Command::Stop);
        assert_eq!(parse(&args(&["run"])), Command::Run);
    }

    #[test]
    fn unknown_command_is_unknown() {
        assert_eq!(parse(&args(&["frobnicate"])), Command::Unknown);
    }

    #[test]
    fn unknown_flag_is_unknown() {
        assert_eq!(parse(&args(&["--json"])), Command::Unknown);
    }

    #[test]
    fn extra_args_are_unknown() {
        assert_eq!(parse(&args(&["doctor", "--json"])), Command::Unknown);
    }
}
