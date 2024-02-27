// vim: sw=4 et filetype=rust

use std::{env, fmt};
use tokio::time::Duration;

use crate::log::{format_log_message, new_time_string};
use crate::{init, util, Level};

#[derive(Clone, Copy, Debug, Hash, Default)]
pub struct Config {
    pub log_level: Level,
    pub colored_output: bool,
    pub colored_output_forced: bool,
    pub buffer_client_receive_size: usize,
    pub request_header_limit_bytes: usize,
    pub buffer_read_client_timeout: Duration,
}

impl Config {
    pub fn default() -> Config {
        init!("create default config");

        Config {
            log_level: Level::Trace,
            colored_output: true,
            colored_output_forced: false,
            buffer_client_receive_size: 32,
            request_header_limit_bytes: 4096,
            buffer_read_client_timeout: Duration::from_secs(3600),
        }
    }

    pub fn default_from_env() -> Config {
        init!("create default config from env vars");
        let mut config = Self::default();

        // TODO: following code block is unpleasant to read
        let rust_log_key = "RUST_LOG";
        init!(r#"discover config "log_level" from env var "{rust_log_key}""#);
        if let Ok(rust_log) = env::var(rust_log_key) {
            init!(r#"env var "{rust_log_key}" exists"#);
            let parsed_log_level = Self::try_parse_log_level(rust_log.as_str());

            if let Ok(log_level) = parsed_log_level {
                config.log_level = log_level;
            } else {
                init!(r#"keep default log level of "{}""#, config.log_level);
            }
        } else {
            init!(r#"keep default log level of "{}""#, config.log_level);
        }

        config
    }

    fn try_parse_log_level(level: &str) -> Result<Level, &'static str> {
        init!(r#"try to find a log level named "{level}""#);

        let parsed_log_level = Level::try_from(level);

        if let Ok(level) = parsed_log_level {
            init!("log level was found as {level:?}");
        } else {
            init!("log level was NOT found");
        }

        parsed_log_level
    }
}

impl fmt::Display for Config {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let value = format!("{self:?}");
        util::format_with_options(&value, f)
    }
}
