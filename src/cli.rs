use clap::{Parser, ValueEnum};

#[derive(Clone, Copy, PartialEq, Eq, Debug, ValueEnum)]
pub enum Period {
    Today,
    Week,
    Month,
    #[value(name = "7d")]
    Days7,
    #[value(name = "30d")]
    Days30,
}

impl Period {
    pub fn label(self) -> &'static str {
        match self {
            Period::Today => "Today",
            Period::Week => "Week",
            Period::Month => "Month",
            Period::Days7 => "7d",
            Period::Days30 => "30d",
        }
    }

    /// JSON identifier.
    pub fn id(self) -> &'static str {
        match self {
            Period::Today => "today",
            Period::Week => "week",
            Period::Month => "month",
            Period::Days7 => "7d",
            Period::Days30 => "30d",
        }
    }
}

/// Help and version are plain flags so their output stays exactly `HELP` / `toknote <version>`.
#[derive(Parser, Debug)]
#[command(name = "toknote", disable_help_flag = true, disable_version_flag = true, args_override_self = true)]
pub struct Args {
    pub period: Option<Period>,
    #[arg(long)]
    pub json: bool,
    #[arg(long)]
    pub live: bool,
    #[arg(long)]
    pub redact: bool,
    #[arg(long)]
    pub verbose: bool,
    #[arg(short, long)]
    pub help: bool,
    #[arg(short = 'V', long)]
    pub version: bool,
}

pub const HELP: &str = "Usage: toknote [today|week|month|7d|30d] [OPTIONS]\n\n\
Token usage, estimated cost, and limits from local coding-agent logs.\n\
No period opens the TUI. Network is opt-in.\n\n\
\x20 --json     Machine-readable output\n  --live     Fetch account limits (network)\n\
\x20 --redact   Hide file paths\n  --verbose  Report skipped log lines\n\
\x20 -h, --help\n  -V, --version\n";
