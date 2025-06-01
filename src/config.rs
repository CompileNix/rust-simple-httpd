use std::env;
use std::error::Error;
use std::fmt;
use std::str::FromStr;
use std::time::Duration;

// #[cfg(feature = "humantime")]
// use crate::util::new_time_string;

#[cfg(feature = "color")]
use crate::color::Colorize;

use crate::{init, log, struct_with_colorized_display_impl, util};

struct_with_colorized_display_impl!(
    pub struct Config {
        pub bind_addr: String,
        pub buffer_client_receive_size: usize,
        pub buffer_read_client_timeout: Duration,
        pub buffer_write_client_timeout: Duration,
        pub colored_output: bool,
        pub colored_output_forced: bool,
        pub log_level: log::Level,
        pub request_header_limit_bytes: usize,
        pub workers: usize,
    }
);

#[derive(Debug, PartialEq, Clone)]
pub enum ConfigError {
    Int(std::num::ParseIntError),
    Enum(log::ParseEnumError),
    Bool(std::str::ParseBoolError),
}

impl Error for ConfigError {}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ConfigError::Int(e) => write!(f, "Failed to parse integer: {e}"),
            ConfigError::Enum(e) => write!(f, "Failed to parse enum value: {e}"),
            ConfigError::Bool(e) => write!(f, "Failed to parse boolean: {e}"),
        }
    }
}

pub fn parse_duration(s: &str) -> Result<Duration, ConfigError> {
    fn parse_part<T>(part: &str, multiplier: T) -> Result<u64, ConfigError>
    where
        T: Into<Option<u64>>,
    {
        let num_str = part.trim_end_matches(['s', 'm', 'h']);
        let value = u64::from_str(num_str).map_err(ConfigError::Int)?;
        Ok(value * multiplier.into().unwrap_or(1))
    }

    let parts: Vec<&str> = s.split_whitespace().collect();
    let mut total_seconds: u64 = 0;

    for part in parts {
        let sec = match part.chars().last() {
            Some('h') => parse_part(part, 3600),
            Some('m') => parse_part(part, 60),
            Some('s') | _ => parse_part(part, 1),
        };

        match sec {
            Ok(s) => total_seconds = total_seconds.checked_add(s).expect("Overflow when adding duration parts"),
            Err(e) => {
                init!("Warn: failed to parse duration part '{}': {} - ignoring this part", part, e);
                return Err(e);
            }
        }
    }

    Ok(Duration::from_secs(total_seconds))
}

impl Config {
    pub fn default() -> Config {
        init!("create default config");

        Config {
            bind_addr: "127.0.0.1:8000".to_string(),
            buffer_client_receive_size: 32,
            buffer_read_client_timeout: Duration::from_secs(3600),
            buffer_write_client_timeout: Duration::from_secs(3600),
            colored_output: true,
            colored_output_forced: false,
            log_level: log::Level::Trace,
            request_header_limit_bytes: 4096,
            workers: 0,
        }
    }

    pub fn default_from_env() -> Result<Config, ConfigError> {
        let mut config = Self::default();
        init!("create default config from env vars");

        config.log_level = discover_from_env_or(
            config.log_level,
            "RUST_LOG",
            "log_level",
            |s| log::Level::from_str(&s).map_err(ConfigError::Enum),
            "trace, debug, info, warn, error",
        )?;

        config.bind_addr = discover_from_env_or(config.bind_addr,
            "BIND_ADDR",
            "bind_addr",
            Ok,
            "a valid socket address (e.g., 127.0.0.1:8000)"
        )?;

        config.buffer_client_receive_size = discover_from_env_or(
            config.buffer_client_receive_size,
            "BUFFER_CLIENT_RECEIVE_SIZE",
            "buffer_client_receive_size",
            |s| s.parse().map_err(ConfigError::Int),
            "a positive integer"
        )?;

        config.buffer_read_client_timeout = discover_from_env_or(
            config.buffer_read_client_timeout,
            "BUFFER_READ_CLIENT_TIMEOUT",
            "buffer_read_client_timeout",
            |s| parse_duration(&s),
            "duration in seconds (e.g., 60s, 5m, 1h), or a combination separated by whitespaces (e.g., 1h 30m)"
        )?;

        config.buffer_write_client_timeout = discover_from_env_or(
            config.buffer_write_client_timeout,
            "BUFFER_WRITE_CLIENT_TIMEOUT",
            "buffer_write_client_timeout",
            |s| parse_duration(&s),
            "duration in seconds (e.g., 60s, 5m, 1h), or a combination separated by whitespaces (e.g., 1h 30m)"
        )?;

        config.colored_output = discover_from_env_or(config.colored_output,
            "COLORED_OUTPUT",
            "colored_output",
            |s| s.parse().map_err(ConfigError::Bool),
            "true or false"
        )?;

        config.colored_output_forced = discover_from_env_or(config.colored_output_forced,
            "COLORED_OUTPUT_FORCED",
            "colored_output_forced",
            |s| s.parse().map_err(ConfigError::Bool),
            "true or false"
        )?;

        config.request_header_limit_bytes = discover_from_env_or(config.request_header_limit_bytes,
            "REQUEST_HEADER_LIMIT_BYTES",
            "request_header_limit_bytes",
            |s| s.parse().map_err(ConfigError::Int),
            "a positive integer"
        )?;

        config.workers = discover_from_env_or(config.workers,
            "WORKERS",
            "workers",
            |s| s.parse().map_err(ConfigError::Int),
            "a non-negative integer"
        )?;

        Ok(config)
    }
}

fn discover_from_env_or<T, F>(
    default: T,
    env_key: &str,
    config_name: &str,
    parser: F,
    valid_values_message: &str,
) -> Result<T, ConfigError>
where
    F: Fn(String) -> Result<T, ConfigError>,
    T: PartialEq + fmt::Debug,
{
    init!(r#"discover config "{config_name}" via env "{env_key}""#);

    let Some(discovered_value) = try_discover_env(env_key) else {
        init!(r#"keep default "{config_name}" value of "{default:?}""#);
        return Ok(default);
    };

    init!(r#"try to parse value "{discovered_value}" for "{config_name}" as {valid_values_message}"#);
    match parser(discovered_value.clone()) {
        Ok(value) => {
            init!(r#"value of "{discovered_value}" was found as "{value:?}" for {config_name}"#);
            Ok(value)
        },
        Err(e) => {
            init!(r"internal parser error: {e}");
            init!(r#"value of "{discovered_value}" is NOT valid for {config_name}. Valid value(s): {valid_values_message}"#);
            Err(e)
        }
    }
}

fn try_discover_env(key: &str) -> Option<String> {
    match env::var(key) {
        Ok(val) => {
            init!(r#"env var "{key}" exists and has value "{val}""#);
            Some(val)
        }
        Err(err) => {
            init!(
                r#"couldn't interpret env var "{key}". It either doesn't exist or it's value isn't valid Unicode. Error was: {err:?}"#
            );
            None
        }
    }
}
