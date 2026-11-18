//! Keeps the command table in `AGENTS.md` (section 4) the same as the commands `main.rs` registers.
//!
//! The table is written by hand and was the part of that file that drifted most. Only the command
//! names and their count are checked; paths and symbols elsewhere in `AGENTS.md` are not.

use std::collections::BTreeSet;

const MAIN_RS: &str = include_str!("main.rs");
const AGENTS_MD: &str = include_str!("../../../AGENTS.md");
const HANDLER_START: &str = "generate_handler![";
const TABLE_HEADING: &str = "## 4. Tauri IPC Interface";

/// The command names inside `generate_handler![...]`, without their module paths.
fn registered_commands(main_rs: &str) -> BTreeSet<String> {
    let Some((_, after_start)) = main_rs.split_once(HANDLER_START) else {
        return BTreeSet::new();
    };
    let list = after_start.split(']').next().unwrap_or_default();
    list.split(',')
        .filter_map(|path| path.trim().rsplit("::").next())
        .filter(|name| !name.is_empty())
        .map(str::to_owned)
        .collect()
}

/// The text of section 4, from its heading to the next `## ` heading.
fn command_section(agents_md: &str) -> &str {
    let Some((_, section)) = agents_md.split_once(TABLE_HEADING) else {
        return "";
    };
    section.split("\n## ").next().unwrap_or_default()
}

/// Every `name` in backticks in the "Command Identifier" column (the second) of section 4's table.
fn documented_commands(agents_md: &str) -> BTreeSet<String> {
    command_section(agents_md)
        .lines()
        .filter(|line| line.starts_with('|'))
        .filter_map(|row| row.split('|').nth(2))
        .flat_map(|cell| cell.split('`').skip(1).step_by(2))
        .map(str::to_owned)
        .collect()
}

/// The number in section 4's heading, "(64 Commands Reference)".
fn documented_count(agents_md: &str) -> Option<usize> {
    let heading = agents_md.split_once(TABLE_HEADING)?.1.lines().next()?;
    let after_paren = heading.split_once('(')?.1;
    after_paren.split_whitespace().next()?.parse().ok()
}

#[test]
fn registered_commands_handler_list_returns_names_without_modules() {
    let main_rs = "x.invoke_handler(tauri::generate_handler![\n  a::first,\n  b::c::second,\n])";
    let expected: BTreeSet<String> = ["first", "second"].map(str::to_owned).into();
    assert_eq!(registered_commands(main_rs), expected);
}

#[test]
fn documented_commands_table_returns_every_name_in_the_second_column() {
    let agents_md = "## 4. Tauri IPC Interface (2 Commands)\n\n| Category | Command | Description |\n|---|---|---|\n| **Data** | `get_cap` / `set_cap` | Uses `other`. |\n\n## 5. Next\n| x | `not_this` | y |\n";
    let expected: BTreeSet<String> = ["get_cap", "set_cap"].map(str::to_owned).into();
    assert_eq!(documented_commands(agents_md), expected);
    assert_eq!(documented_count(agents_md), Some(2));
}

#[test]
fn agents_md_command_table_matches_the_registered_commands() {
    let registered = registered_commands(MAIN_RS);
    let documented = documented_commands(AGENTS_MD);
    let undocumented: Vec<_> = registered.difference(&documented).collect();
    let unregistered: Vec<_> = documented.difference(&registered).collect();
    assert!(
        undocumented.is_empty() && unregistered.is_empty(),
        "AGENTS.md section 4 is out of date: registered but not in the table {undocumented:?}, \
         in the table but not registered {unregistered:?}"
    );
}

#[test]
fn agents_md_command_count_matches_the_registered_commands() {
    assert_eq!(
        documented_count(AGENTS_MD),
        Some(registered_commands(MAIN_RS).len()),
        "the count in the heading of AGENTS.md section 4 is out of date"
    );
}
