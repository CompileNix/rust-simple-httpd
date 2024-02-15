// vim: sw=4 et filetype=rust

// TODO: remove
#![allow(dead_code)]
#![allow(unused)]

use std::fmt;

#[cfg(feature = "humantime")]
use time::util::local_offset::Soundness;

#[cfg(feature = "color")]
use crate::color::Color;
use crate::color::Colorize;
use crate::log;
use crate::util;

/// An enum representing the available verbosity levels of the logger.
///
/// Default value: `Warn`
#[repr(usize)]
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Hash, Default)]
pub enum Level {
    /// The "error" level.
    ///
    /// Designates very serious errors.
    Error = 1,
    /// The "warn" level.
    ///
    /// Designates hazardous situations.
    #[default]
    Warn,
    /// The "info" level.
    ///
    /// Designates useful information.
    Info,
    /// The "verbose" level.
    ///
    /// Designates some additional information.
    Verb,
    /// The "debug" level.
    ///
    /// Designates lower priority information.
    Debug,
    /// The "trace" level.
    ///
    /// Designates very low priority, often extremely verbose, information.
    Trace,
}

impl Level {
    fn from_usize(u: usize) -> Option<Level> {
        match u {
            1 => Some(Level::Error),
            2 => Some(Level::Warn),
            3 => Some(Level::Info),
            4 => Some(Level::Verb),
            5 => Some(Level::Debug),
            6 => Some(Level::Trace),
            _ => None,
        }
    }

    fn from_str(s: &str) -> Option<Level> {
        match s {
            "error" => Some(Level::Error),
            "warn" => Some(Level::Warn),
            "info" => Some(Level::Info),
            "verb" => Some(Level::Verb),
            "debug" => Some(Level::Debug),
            "trace" => Some(Level::Trace),
            _ => None,
        }
    }

    pub fn iter() -> impl Iterator<Item = Self> {
        (1..7).map(|i| Self::from_usize(i).unwrap())
    }

    fn compare_levels(a: Level, b: Level) -> std::cmp::Ordering {
        a.cmp(&b)
    }
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
        if let Some(val) = Self::from_str(value) {
            Ok(val)
        } else {
            Err("Unknown log level provided")
        }
    }
}

#[cfg(feature = "color")]
impl fmt::Display for Level {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let value = match self {
            Level::Error => "Error".red(),
            Level::Warn => "Warn".yellow(),
            Level::Info => "Info".green(),
            Level::Verb => "Verb".magenta(),
            Level::Debug => "Debug".blue(),
            Level::Trace => "Trace".cyan(),
        };

        util::format_with_options(&value, f)
    }
}

#[cfg(not(feature = "color"))]
impl fmt::Display for Level {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let value = format!("{self:?}");
        util::format_with_options(&value, f)
    }
}

/// Safety notice: <https://docs.rs/time/0.3.34/time/util/local_offset/fn.set_soundness.html#safety>
#[cfg(feature = "humantime")]
fn new_log_message_prefix_text(level: Level) -> String {
    unsafe {
        time::util::local_offset::set_soundness(Soundness::Unsound);
    }
    let now = time::OffsetDateTime::now_local().unwrap_or(time::OffsetDateTime::now_utc());
    let time = now
        .format(
            &time::format_description::parse_borrowed::<2>(
                "[weekday repr:short], [day] [month repr:short] [year] [hour]:[minute]:[second]",
            )
            .expect("Could not format current datetime to string for log message"),
        )
        .unwrap_or_default();

    if cfg!(feature = "color") {
        format!("[{time} {level:^14} ]")
    } else {
        format!("[{time} {level:^5} ]")
    }
}

#[cfg(not(feature = "humantime"))]
fn new_log_message_prefix_text(level: Level) -> String {
    let now = time::OffsetDateTime::now_utc();
    let time = now
        .format(&time::format_description::well_known::Iso8601::DEFAULT)
        .unwrap_or_default();

    if cfg!(feature = "color") {
        let level = format!("{level:<14}");
        format!("[{time} {level} ]")
    } else {
        format!("[{time} {level:^5} ]")
    }
}

pub fn error(text: &str) {
    let message_prefix = new_log_message_prefix_text(Level::Error);
    eprintln!("{message_prefix} {text}");
}

pub fn warn(text: &str) {
    let message_prefix = new_log_message_prefix_text(Level::Warn);
    eprintln!("{message_prefix} {text}");
}

pub fn info(text: &str) {
    let message_prefix = new_log_message_prefix_text(Level::Info);
    println!("{message_prefix} {text}");
}

pub fn verb(text: &str) {
    let message_prefix = new_log_message_prefix_text(Level::Verb);
    eprintln!("{message_prefix} {text}");
}

pub fn debug(text: &str) {
    let message_prefix = new_log_message_prefix_text(Level::Debug);
    eprintln!("{message_prefix} {text}");
}

pub fn trace(text: &str) {
    let message_prefix = new_log_message_prefix_text(Level::Trace);
    eprintln!("{message_prefix} {text}");
}

#[derive(Clone, Copy, Debug, Hash, Default)]
pub struct Logger {
    pub level: Level,
}

impl Logger {
    pub fn error(self, text: &str) {
        if (self.level >= Level::Error) {
            error(text);
        }
    }

    pub fn warn(self, text: &str) {
        if (self.level >= Level::Warn) {
            warn(text);
        }
    }

    pub fn info(self, text: &str) {
        if (self.level >= Level::Info) {
            info(text);
        }
    }

    pub fn verb(self, text: &str) {
        if (self.level >= Level::Verb) {
            verb(text);
        }
    }

    pub fn debug(self, text: &str) {
        if (self.level >= Level::Debug) {
            debug(text);
        }
    }

    pub fn trace(self, text: &str) {
        if (self.level >= Level::Trace) {
            trace(text);
        }
    }
}
