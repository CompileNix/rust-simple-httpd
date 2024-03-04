use std::env;
use std::fmt;
use tokio::time::Duration;

use crate::util::new_time_string;
use crate::{init, log, struct_with_colorized_display_impl, util};

#[cfg(feature = "color")]
use crate::color::Colorize;

struct_with_colorized_display_impl!(
    pub struct Config {
        pub log_level: log::Level,
        pub colored_output: bool,
        pub colored_output_forced: bool,
        pub buffer_client_receive_size: usize,
        pub request_header_limit_bytes: usize,
        pub buffer_read_client_timeout: Duration,
    }
);

impl Config {
    pub fn default() -> Config {
        init!("create default config");

        Config {
            log_level: log::Level::Trace,
            colored_output: true,
            colored_output_forced: false,
            buffer_client_receive_size: 32,
            request_header_limit_bytes: 4096,
            buffer_read_client_timeout: Duration::from_secs(3600),
        }
    }

    pub fn default_from_env() -> Config {
        let mut config = Self::default();
        init!("create default config from env vars");

        config.log_level = discover_from_env_or(config.log_level, "RUST_LOG", "log_level");
        // TODO: continue here
        // config.colored_output = discover_from_env_or(config.colored_output, "COLORED_OUTPUT", "colored_output");

        config
    }
}

fn discover_from_env_or<T: PartialEq + fmt::Display + for<'a> TryFrom<&'a str>>(
    default: T,
    env_key: &str,
    config_name: &str,
) -> T {
    init!(r#"discover config "{config_name}" via env "{env_key}""#);

    let discovered_value = match try_discover_env(env_key) {
        None => {
            init!(r#"keep default "{config_name}" value of "{default}""#);
            return default;
        }
        Some(value) => value,
    };

    match try_parse_env(&discovered_value, config_name) {
        None => {
            init!(r#"keep default "{config_name}" value of "{default}""#);
            default
        }
        Some(value) => value,
    }
}

fn try_parse_env<T: for<'a> TryFrom<&'a str> + fmt::Display>(
    value: &str,
    config_name: &str,
) -> Option<T> {
    init!(r#"try to parse "{value}" for "{config_name}""#);

    let parsed_value_option = T::try_from(value);
    if let Ok(parsed_value) = parsed_value_option {
        init!(r#"value of "{value}" was found as "{parsed_value}" for {config_name}"#);
        Some(parsed_value)
    } else {
        init!(r#"value of "{value}" was NOT found for {config_name}"#);
        None
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
