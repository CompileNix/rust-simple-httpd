use std::fmt;

pub fn format_with_options(value: &impl fmt::Display, f: &mut fmt::Formatter) -> fmt::Result {
    if let Some(width) = f.width() {
        let alignment = f.align().unwrap_or(fmt::Alignment::Right);

        // Format output according to the width and alignment
        let formatted_value = match alignment {
            fmt::Alignment::Left => format!("{value:<width$}"),
            fmt::Alignment::Right => format!("{value:>width$}"),
            fmt::Alignment::Center => format!("{value:^width$}"),
        };

        // Write the formatted string to the formatter
        write!(f, "{formatted_value}")
    } else {
        // If no width is set, just write the value
        write!(f, "{value}")
    }
}
