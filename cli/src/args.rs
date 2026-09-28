#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Doctor,
    Models,
    Recommend {
        profile: Option<String>,
    },
    Start {
        profile: Option<String>,
    },
    Status,
    Chat {
        prompt: Option<String>,
    },
    Stop,
    Run {
        prompt: Option<String>,
    },
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
            "recommend" => Command::Recommend { profile: None },
            "start" => Command::Start { profile: None },
            "status" => Command::Status,
            "chat" => Command::Chat { prompt: None },
            "stop" => Command::Stop,
            "run" => Command::Unknown,
            _ => Command::Unknown,
        },
        [arg, name] if arg == "recommend" || arg == "start" => profile_command(arg, name),
        [arg, tail @ ..] if arg == "chat" && !tail.is_empty() => {
            Command::Chat {
                prompt: Some(tail.join(" ")),
            }
        }
        [arg, tail @ ..] if arg == "run" && !tail.is_empty() => {
            Command::Run {
                prompt: Some(tail.join(" ")),
            }
        }
        _ => Command::Unknown,
    }
}

fn profile_command(command: &str, name: &str) -> Command {
    let lowered = name.to_lowercase();
    let profile = match lowered.as_str() {
        "chat" | "coding" | "reasoning" | "vision" | "agent" | "longcontext" => Some(lowered),
        _ => None,
    };
    let Some(profile) = profile else {
        return Command::Unknown;
    };
    match command {
        "recommend" => Command::Recommend { profile: Some(profile) },
        "start" => Command::Start { profile: Some(profile) },
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
        assert_eq!(
            parse(&args(&["recommend"])),
            Command::Recommend { profile: None }
        );
        assert_eq!(parse(&args(&["start"])), Command::Start { profile: None });
        assert_eq!(parse(&args(&["status"])), Command::Status);
        assert_eq!(parse(&args(&["chat"])), Command::Chat { prompt: None });
        assert_eq!(parse(&args(&["stop"])), Command::Stop);
    }

    #[test]
    fn recommend_without_profile_parses() {
        assert_eq!(
            parse(&args(&["recommend"])),
            Command::Recommend { profile: None }
        );
    }

    #[test]
    fn recommend_with_profile_parses() {
        assert_eq!(
            parse(&args(&["recommend", "chat"])),
            Command::Recommend { profile: Some("chat".to_string()) }
        );
        assert_eq!(
            parse(&args(&["recommend", "coding"])),
            Command::Recommend { profile: Some("coding".to_string()) }
        );
        assert_eq!(
            parse(&args(&["recommend", "reasoning"])),
            Command::Recommend { profile: Some("reasoning".to_string()) }
        );
        assert_eq!(
            parse(&args(&["recommend", "vision"])),
            Command::Recommend { profile: Some("vision".to_string()) }
        );
        assert_eq!(
            parse(&args(&["recommend", "agent"])),
            Command::Recommend { profile: Some("agent".to_string()) }
        );
        assert_eq!(
            parse(&args(&["recommend", "longcontext"])),
            Command::Recommend { profile: Some("longcontext".to_string()) }
        );
    }

    #[test]
    fn recommend_profile_is_case_insensitive() {
        assert_eq!(
            parse(&args(&["recommend", "Coding"])),
            Command::Recommend { profile: Some("coding".to_string()) }
        );
        assert_eq!(
            parse(&args(&["recommend", "LongContext"])),
            Command::Recommend { profile: Some("longcontext".to_string()) }
        );
        assert_eq!(
            parse(&args(&["recommend", "REASONING"])),
            Command::Recommend { profile: Some("reasoning".to_string()) }
        );
    }

    #[test]
    fn recommend_invalid_profile_is_unknown() {
        assert_eq!(parse(&args(&["recommend", "frobnicate"])), Command::Unknown);
        assert_eq!(parse(&args(&["recommend", "--json"])), Command::Unknown);
    }

    #[test]
    fn start_without_profile_parses() {
        assert_eq!(parse(&args(&["start"])), Command::Start { profile: None });
    }

    #[test]
    fn start_with_profile_parses() {
        for name in ["chat", "coding", "reasoning", "vision", "agent", "longcontext"] {
            assert_eq!(
                parse(&args(&["start", name])),
                Command::Start { profile: Some(name.to_string()) },
                "expected Start for `start {name}`"
            );
        }
    }

    #[test]
    fn start_profile_is_case_insensitive() {
        assert_eq!(
            parse(&args(&["start", "CHAT"])),
            Command::Start { profile: Some("chat".to_string()) }
        );
        assert_eq!(
            parse(&args(&["start", "Coding"])),
            Command::Start { profile: Some("coding".to_string()) }
        );
        assert_eq!(
            parse(&args(&["start", "LongContext"])),
            Command::Start { profile: Some("longcontext".to_string()) }
        );
    }

    #[test]
    fn start_invalid_profile_is_unknown() {
        assert_eq!(parse(&args(&["start", "frobnicate"])), Command::Unknown);
        assert_eq!(parse(&args(&["start", "--json"])), Command::Unknown);
    }

    #[test]
    fn chat_without_prompt_parses() {
        assert_eq!(parse(&args(&["chat"])), Command::Chat { prompt: None });
    }

    #[test]
    fn chat_prompt_joins_with_spaces() {
        assert_eq!(
            parse(&args(&["chat", "hello"])),
            Command::Chat {
                prompt: Some("hello".to_string())
            }
        );
        assert_eq!(
            parse(&args(&["chat", "hello", "world"])),
            Command::Chat {
                prompt: Some("hello world".to_string())
            }
        );
    }

    #[test]
    fn run_prompt_joins_with_spaces() {
        assert_eq!(
            parse(&args(&["run", "hello"])),
            Command::Run {
                prompt: Some("hello".to_string())
            }
        );
        assert_eq!(
            parse(&args(&["run", "hello", "world"])),
            Command::Run {
                prompt: Some("hello world".to_string())
            }
        );
    }

    #[test]
    fn run_without_prompt_is_unknown() {
        assert_eq!(parse(&args(&["run"])), Command::Unknown);
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
        for command in ["doctor", "models", "start", "status", "stop"] {
            assert_eq!(
                parse(&args(&[command, "extra"])),
                Command::Unknown,
                "expected Unknown for `{command} extra`"
            );
        }
        assert_eq!(
            parse(&args(&["recommend", "coding", "extra"])),
            Command::Unknown
        );
    }
}
