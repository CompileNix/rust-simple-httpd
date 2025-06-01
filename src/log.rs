use std::fmt;

use crate::{enum_with_helpers, util};

#[cfg(feature = "log-error")]
use crate::config::Config;

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

impl Display for Level {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        let value = format!("{self:?}");
        util::format_with_options(&value, f)
    }
}

#[cfg(feature = "log-error")]
pub struct Log<'a> {
    config: &'a Config,
    message: String,
    level: Level,
    time: String,
}

#[cfg(feature = "log-error")]
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

#[cfg(feature = "log-error")]
impl Display for Log<'_> {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
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

#[macro_export]
macro_rules! error {
    ($config:expr, $($arg:tt)*) => {{
        if $config.log_level >= Level::Error {
            #[cfg(feature = "log-error")]
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

#[macro_export]
macro_rules! warn {
    ($config:expr, $($arg:tt)*) => {{
        if $config.log_level >= Level::Warn {
            #[cfg(feature = "log-warn")]
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

#[macro_export]
macro_rules! info {
    ($config:expr, $($arg:tt)*) => {{
        if $config.log_level >= Level::Info {
            #[cfg(feature = "log-info")]
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

#[macro_export]
macro_rules! verb {
    ($config:expr, $($arg:tt)*) => {{
        if $config.log_level >= Level::Verb {
            #[cfg(feature = "log-verb")]
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

#[macro_export]
macro_rules! debug {
    ($config:expr, $($arg:tt)*) => {{
        if $config.log_level >= Level::Debug {
            #[cfg(feature = "log-debug")]
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

// #[cfg(not(feature = "log-trace"))]
// pub fn trace(config: &Config, text: &str) { }

#[macro_export]
macro_rules! trace {
    ($config:expr, $($arg:tt)*) => {{
        if $config.log_level >= Level::Trace {
            #[cfg(feature = "log-trace")]
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
