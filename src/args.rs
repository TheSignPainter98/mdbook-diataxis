use clap::{
    builder::{
        styling::{AnsiColor, Effects},
        Styles,
    },
    ArgAction, Parser, Subcommand,
};

#[derive(Debug, Parser)]
#[command(
    author,
    version,
    about,
    disable_help_flag = true,
    disable_version_flag = true,
    styles = Self::styles(),
)]
pub(crate) struct Args {
    #[command(subcommand)]
    pub(crate) command: Option<Command>,
    /// Print help information, use `--help` for more detail
    #[arg(short, long, action=ArgAction::Help, global=true)]
    help: Option<bool>,

    /// Print version
    #[arg(long, action=ArgAction::Version)]
    version: Option<bool>,
}

impl Args {
    fn styles() -> Styles {
        // Match cargo output style
        Styles::styled()
            .header(AnsiColor::Green.on_default().effects(Effects::BOLD))
            .usage(AnsiColor::Green.on_default().effects(Effects::BOLD))
            .literal(AnsiColor::Cyan.on_default().effects(Effects::BOLD))
            .placeholder(AnsiColor::Cyan.on_default())
            .error(AnsiColor::Red.on_default().effects(Effects::BOLD))
            .valid(AnsiColor::Cyan.on_default().effects(Effects::BOLD))
            .invalid(AnsiColor::Yellow.on_default().effects(Effects::BOLD))
    }
}

#[derive(Debug, Subcommand)]
pub(crate) enum Command {
    /// Check whether this preprocessor supports the given renderer
    Supports(SupportsCmd),
}

#[derive(Debug, Parser)]
pub(crate) struct SupportsCmd {
    /// The renderer to check
    pub(crate) renderer: String,
}
