// vim: sw=4 et filetype=rust

// TODO: remove
#![allow(dead_code)]
#![allow(unused)]

use crate::color::Color;
use anyhow::Error;
use std::convert::Infallible;
use std::fmt::format;
use std::{env, fmt};
use tokio::time::Duration;

use crate::{util, log, Level, trace};

#[derive(Clone, Copy, Debug, Hash, Default)]
pub struct Config {
    pub log_level: Level,
    pub buffer_client_receive_size: usize,
    pub request_header_limit_bytes: usize,
    pub buffer_read_client_timeout: Duration,
}

impl Config {
    pub fn default() -> Config {
        trace!(Level::Trace, "create default config");

        Config {
            log_level: Level::Warn,
            buffer_client_receive_size: 32,
            request_header_limit_bytes: 4096,
            buffer_read_client_timeout: Duration::from_secs(3600),
        }
    }

    pub fn default_from_env() -> Config {
        trace!(Level::Trace, "create default config from env vars");
        let mut config = Self::default();

        let rust_log_key = "RUST_LOG";
        trace!(Level::Trace, r#"discover config "log_level" from env var "{rust_log_key}""#);
        if let Ok(rust_log) = env::var(rust_log_key) {
            trace!(Level::Trace, r#"env var "{rust_log_key}" exists"#);
            let parsed_log_level = Self::try_parse_log_level(rust_log.as_str());

            if let Ok(log_level) = parsed_log_level {
                config.log_level = log_level;
            } else {
                trace!(Level::Trace, r#"keep default log level of "{}""#, config.log_level);
            }
        } else {
            trace!(Level::Trace, r#"keep default log level of "{}""#, config.log_level);
        }

        config
    }

    fn try_parse_log_level(level: &str) -> Result<Level, &'static str> {
        trace!(Level::Trace, r#"try to find a log level named "{level}""#);

        let parsed_log_level = Level::try_from(level);

        if let Ok(level) = parsed_log_level {
            trace!(Level::Trace, "log level was found as {level:?}");
        } else {
            trace!(Level::Trace, "log level was NOT found");
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
