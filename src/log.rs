use std::fmt;

use crate::config::Config;
use crate::{enum_with_helpers, util};

enum_with_helpers! {
    pub enum Level {
        Error,
        Warn,
        Info,
        Verb,
        Debug,
        Trace,
    } default: Warn
}

impl TryFrom<usize> for Level {
    type Error = &'static str;

    fn try_from(value: usize) -> Result<Self, <Level as TryFrom<usize>>::Error> {
        if let Some(val) = Self::from_usize(value) {
            Ok(val)
        } else {
            Err("Unknown log level provided")
        }
    }
}

impl TryFrom<&str> for Level {
    type Error = &'static str;

    fn try_from(value: &str) -> Result<Self, <Level as TryFrom<&str>>::Error> {
        if let Ok(val) = Self::from_str(value) {
            Ok(val)
        } else {
            Err("Unknown log level provided")
        }
    }
}

impl fmt::Display for Level {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let value = format!("{self:?}");
        util::format_with_options(&value, f)
    }
}

pub struct Log<'a> {
    config: &'a Config,
    message: String,
    level: Level,
    time: String,
}

impl Log<'_> {
    #[must_use]
    pub fn new<'a>(config: &'a Config, text: &'a str, level: Level) -> Log<'a> {
        Log {
            level,
            message: text.to_string(),
            config,
            time: util::new_time_string(),
        }
    }
}

impl fmt::Display for Log<'_> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let level_text: String;
        let colored: bool;
        #[cfg(feature = "color")]
        {
            colored = util::is_colored_output_avail(self.config);
            level_text = if colored {
                util::log_level_to_string_colorized(self.level).text
            } else {
                self.level.to_string()
            };
        }
        #[cfg(not(feature = "color"))]
        {
            // config is only used when color feature is enabled
            let _ = self.config;
            colored = false;
            level_text = self.level.to_string();
        }

        let log_message_prefix =
            util::format_log_message_prefix(&self.time.clone(), &level_text, colored);
        let log_message = format!("{log_message_prefix}{}", self.message);

        util::format_with_options(&log_message, f)
    }
}


// ############################################ ERROR

#[cfg(feature = "log-error")]
pub fn error(config: &Config, text: &str) {
    let formatted_message = format!("{}", Log::new(config, text, Level::Error));
    eprintln!("{formatted_message}");
}

#[cfg(not(feature = "log-error"))]
pub fn error(config: &Config, text: &str) {
    let _ = Log::new(config, text, Level::Error);
}

#[macro_export]
macro_rules! error {
    ($config:expr, $($arg:tt)*) => {{
        if $config.log_level >= Level::Error {
            error($config, &std::fmt::format(format_args!($($arg)*)));
        }
    }};
}

// ############################################ WARNING

#[cfg(feature = "log-warn")]
pub fn warn(config: &Config, text: &str) {
    let formatted_message = format!("{}", Log::new(config, text, Level::Warn));
    eprintln!("{formatted_message}");
}

#[cfg(not(feature = "log-warn"))]
pub fn warn(config: &Config, text: &str) {
    let _ = Log::new(config, text, Level::Warn);
}

#[macro_export]
macro_rules! warn {
    ($config:expr, $($arg:tt)*) => {{
        if $config.log_level >= Level::Warn {
            warn($config, &std::fmt::format(format_args!($($arg)*)));
        }
    }};
}

// ############################################ INFO

#[cfg(feature = "log-info")]
pub fn info(config: &Config, text: &str) {
    let formatted_message = format!("{}", Log::new(config, text, Level::Info));
    println!("{formatted_message}");
}

#[cfg(not(feature = "log-info"))]
pub fn info(config: &Config, text: &str) {
    let _ = Log::new(config, text, Level::Info);
}

#[macro_export]
macro_rules! info {
    ($config:expr, $($arg:tt)*) => {{
        if $config.log_level >= Level::Info {
            info($config, &std::fmt::format(format_args!($($arg)*)));
        }
    }};
}

// ############################################ VERBOSE

#[cfg(feature = "log-verb")]
pub fn verb(config: &Config, text: &str) {
    let formatted_message = format!("{}", Log::new(config, text, Level::Verb));
    println!("{formatted_message}");
}

#[cfg(not(feature = "log-verb"))]
pub fn verb(config: &Config, text: &str) {
    let _ = Log::new(config, text, Level::Verb);
}

#[macro_export]
macro_rules! verb {
    ($config:expr, $($arg:tt)*) => {{
        if $config.log_level >= Level::Verb {
            verb($config, &std::fmt::format(format_args!($($arg)*)));
        }
    }};
}

// ############################################ DEBUG

#[cfg(feature = "log-debug")]
pub fn debug(config: &Config, text: &str) {
    let formatted_message = format!("{}", Log::new(config, text, Level::Debug));
    eprintln!("{formatted_message}");
}

#[cfg(not(feature = "log-debug"))]
pub fn debug(config: &Config, text: &str) {
    let _ = Log::new(config, text, Level::Debug);
}

#[macro_export]
macro_rules! debug {
    ($config:expr, $($arg:tt)*) => {{
        if $config.log_level >= Level::Debug {
            debug($config, &std::fmt::format(format_args!($($arg)*)));
        }
    }};
}

// ############################################ TRACE

#[cfg(feature = "log-trace")]
pub fn trace(config: &Config, text: &str) {
    let formatted_message = format!("{}", Log::new(config, text, Level::Trace));
    eprintln!("{formatted_message}");
}

#[cfg(not(feature = "log-trace"))]
pub fn trace(config: &Config, text: &str) {
    let _ = Log::new(config, text, Level::Trace);
}

#[macro_export]
macro_rules! trace {
    ($config:expr, $($arg:tt)*) => {{
        if $config.log_level >= Level::Trace {
            trace($config, &std::fmt::format(format_args!($($arg)*)));
        }
    }};
}

// ############################################ INIT

#[cfg(feature = "log-trace")]
#[macro_export]
macro_rules! init {
    ($($arg:tt)*) => {{
        let text = &std::fmt::format(format_args!($($arg)*));
        let formatted_message_prefix = $crate::util::format_log_message_prefix(&$crate::util::new_time_string(), "Init", false);
        let formatted_message = format!("{formatted_message_prefix}{text}");
        println!("{formatted_message}");
    }};
}

#[cfg(not(feature = "log-trace"))]
#[macro_export]
macro_rules! init {
    ($($arg:tt)*) => {{
        let _ = format_args!($($arg)*);
    }};
}
