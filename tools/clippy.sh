set -x
cargo clippy "$@" -- --no-deps -Wclippy::pedantic -Wclippy::cognitive_complexity -Wclippy::indexing_slicing -Wclippy::large_include_file -Wclippy::linkedlist -Wclippy::map_unwrap_or -Wclippy::option_option -Wclippy::verbose_bit_mask -Wclippy::unused_self -Wclippy::unreadable_literal -Wclippy::unnested_or_patterns -Wclippy::unnecessary_wraps -Wclippy::uninlined_format_args -Wclippy::unchecked_duration_subtraction -Wclippy::too_many_lines -Wclippy::unwrap_used

