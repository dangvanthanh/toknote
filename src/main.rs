mod aggregate;
mod cli;
mod ctx;
mod features;
mod json;
#[cfg(feature = "live")]
mod live;
mod model;
mod output;
mod pricing;
mod sources;
mod time;
mod tui;

use clap::Parser;

use crate::cli::Args;
use crate::ctx::Ctx;

fn main() {
    let Ok(args) = Args::try_parse() else {
        output::write_err("toknote: invalid argument; possible periods: today, week, month, 7d, 30d\n");
        std::process::exit(2);
    };
    if args.help {
        output::write_out(cli::HELP);
        return;
    }
    if args.version {
        output::write_out(&format!("toknote {}\n", features::VERSION));
        return;
    }
    let ctx = Ctx::from_env();
    let Some(period) = args.period else {
        if let Err(e) = tui::app::run(ctx, args.live) {
            output::write_err(&format!("toknote: {e}\n"));
            std::process::exit(1);
        }
        return;
    };
    let now = time::now();
    let mut scan = output::load(&ctx, now);
    if args.verbose {
        output::skipped(&ctx, &scan, args.redact);
    }
    if args.live {
        let live = features::fetch(&ctx, now);
        for e in &live.errors {
            output::write_err(&format!("live: {e}\n"));
        }
        model::apply_live(&mut scan, live);
    }
    let summary = aggregate::summarize(&scan.events, period, now);
    let out = if args.json {
        output::json(&summary, &scan)
    } else if !scan.any_found() {
        format!("{}\n", output::EMPTY)
    } else {
        output::text(&summary, &scan, now)
    };
    output::write_out(&out);
}
