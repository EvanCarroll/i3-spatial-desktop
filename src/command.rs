//! i3 commands as values. `Display` writes the text i3 parses, quoting
//! every name.

use std::fmt;

use crate::error::{Error, Result};
use crate::ipc::Connection;
use crate::world::OutputName;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// Show and focus a workspace.
    Show(String),
    /// The same with plain `workspace`, so `workspace_auto_back_and_forth`
    /// still applies.
    Goto(String),
    /// Focus the workspace an output shows.
    FocusOutput(OutputName),
    /// Move the focused workspace to an output.
    MoveWorkspaceTo(OutputName),
    /// Move the focused container to a workspace.
    MoveContainerTo(String),
    Rename { from: String, to: String },
    /// Move every window of one workspace into another. Fails when there
    /// is none, which is fine.
    Fold { from: String, into: String },
}

impl Command {
    /// Whether the rest of a batch stands when this command fails.
    fn may_fail(&self) -> bool {
        matches!(self, Command::Fold { .. })
    }
}

/// Quotes a workspace or output name for use in an i3/sway command.
fn quote(name: &str) -> String {
    format!("\"{}\"", name.replace('\\', "\\\\").replace('"', "\\\""))
}

/// A regular expression that matches exactly `name`, for use in criteria.
fn exactly(name: &str) -> String {
    let mut pattern = String::from("^");
    for c in name.chars() {
        if r"\^$.|?*+()[]{}".contains(c) {
            pattern.push('\\');
        }
        pattern.push(c);
    }
    pattern.push('$');
    pattern
}

impl fmt::Display for Command {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Command::Show(name) => {
                write!(f, "workspace --no-auto-back-and-forth {}", quote(name))
            }
            Command::Goto(name) => write!(f, "workspace {}", quote(name)),
            Command::FocusOutput(output) => write!(f, "focus output {}", quote(output.as_str())),
            Command::MoveWorkspaceTo(output) => {
                write!(f, "move workspace to output {}", quote(output.as_str()))
            }
            Command::MoveContainerTo(name) => {
                write!(f, "move container to workspace {}", quote(name))
            }
            Command::Rename { from, to } => {
                write!(f, "rename workspace {} to {}", quote(from), quote(to))
            }
            Command::Fold { from, into } => write!(
                f,
                "[workspace={}] move container to workspace {}",
                quote(&exactly(from)),
                quote(into)
            ),
        }
    }
}

/// Runs `commands` as one message, failing with the first that failed.
pub fn run(conn: &mut Connection, commands: &[Command]) -> Result<()> {
    if commands.is_empty() {
        return Ok(());
    }
    let mut texts: Vec<String> = commands.iter().map(Command::to_string).collect();
    let batch = texts.join("; ");
    let replies = conn.replies(&batch)?;
    // i3 answers each command in turn, which tells which one failed.
    let paired = replies.len() == commands.len();
    let excused = |index: usize| paired && commands[index].may_fail();
    let failed = (replies.iter().enumerate())
        .position(|(index, reply)| !reply.success && !excused(index));
    let Some(failed) = failed else {
        return Ok(());
    };
    Err(Error::Command {
        command: if paired {
            texts.swap_remove(failed)
        } else {
            batch
        },
        message: replies[failed].error.clone().unwrap_or_default(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn output(name: &str) -> OutputName {
        OutputName(name.into())
    }

    #[test]
    fn commands_as_i3_reads_them() {
        assert_eq!(
            Command::Show("4:db: ←".into()).to_string(),
            r#"workspace --no-auto-back-and-forth "4:db: ←""#
        );
        assert_eq!(Command::Goto("4:db".into()).to_string(), r#"workspace "4:db""#);
        assert_eq!(
            Command::FocusOutput(output("DP-1")).to_string(),
            r#"focus output "DP-1""#
        );
        assert_eq!(
            Command::MoveWorkspaceTo(output("DP-1")).to_string(),
            r#"move workspace to output "DP-1""#
        );
        assert_eq!(
            Command::MoveContainerTo("6: (-1,0)".into()).to_string(),
            r#"move container to workspace "6: (-1,0)""#
        );
        assert_eq!(
            Command::Rename {
                from: "4:Foo".into(),
                to: "4:Bar".into()
            }
            .to_string(),
            r#"rename workspace "4:Foo" to "4:Bar""#
        );
    }

    #[test]
    fn a_fold_names_its_workspace_exactly() {
        let fold = |from: &str| {
            Command::Fold {
                from: from.into(),
                into: "10:com 📌".into(),
            }
            .to_string()
        };
        assert_eq!(
            fold("10:com"),
            r#"[workspace="^10:com$"] move container to workspace "10:com 📌""#
        );
        // What a regular expression would read otherwise, and then what
        // i3's quoting needs on top.
        assert_eq!(
            fold("6: (-1,0)"),
            r#"[workspace="^6: \\(-1,0\\)$"] move container to workspace "10:com 📌""#
        );
        assert_eq!(exactly("a.b|c[d]{e}*+?^$\\"), r"^a\.b\|c\[d\]\{e\}\*\+\?\^\$\\$");
    }

    #[test]
    fn names_are_quoted() {
        assert_eq!(
            Command::Show(r#"say "hi" \ bye"#.into()).to_string(),
            r#"workspace --no-auto-back-and-forth "say \"hi\" \\ bye""#
        );
        // A comma or semicolon would otherwise end the command.
        assert_eq!(
            Command::Goto("a;b,c".into()).to_string(),
            r#"workspace "a;b,c""#
        );
    }
}
