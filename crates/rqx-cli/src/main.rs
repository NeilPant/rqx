use cli::run::run;

mod agent_setup;
mod badge;
mod baseline;
mod brand;
mod cache;
mod check;
mod cli;
mod complexity;
mod config;
mod credit;
mod edit_config;
mod explain;
mod external;
mod fix;
mod forge;
mod gate;
mod graph;
mod health;
mod history;
mod hotspots;
mod large_stack;
mod listed;
mod lsp;
mod mcp;
mod migrate;
mod ownership;
mod pager;
mod path_suffixes;
mod perf;
mod plan;
mod preview;
mod progress;
mod project;
mod query;
mod recommend;
mod regressions;
mod reimplemented;
mod release;
mod render;
mod report;
mod scan;
mod session;
mod setting;
mod setup;
mod sink;
mod slop;
mod staged;
mod stats;
mod term;
mod trend;
mod unified_diff;
mod update;
mod uri;
mod verify;
mod waive;
mod watch;

pub(crate) use cli::args::{Cli, Command, Complexity, DisplayCap, Evidence, Standard, View};
pub(crate) use cli::emit::{emit_timed, sinks, write_sink};
pub(crate) use cli::parser::parser;
pub(crate) use cli::run::{filter_of, listed_files};
#[cfg(test)]
use sink::Sink;

#[cfg(feature = "mimalloc")]
#[cfg(test)]
#[path = "../tests/common/scratch.rs"]
mod scratch;

#[global_allocator]
static ALLOCATOR: mimalloc::MiMalloc = mimalloc::MiMalloc;

/// The whole contract a pipeline branches on. Named rather than returned as bare
/// integers so a new command cannot invent a fourth meaning by accident.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
enum Exit {
    Clean = 0,
    GateFailed = 1,
    Failed = 2,
}

impl Exit {
    const fn of(failed: bool) -> Self {
        if failed {
            Self::GateFailed
        } else {
            Self::Clean
        }
    }
}

/// `rqx scan . | head` closes the reader mid-report. That is the reader's decision,
/// not a failure of the scan, and reporting it as one fails a pipeline that did
/// exactly what it was asked to.
fn reader_went_away(error: &anyhow::Error) -> bool {
    error.chain().any(|cause| {
        cause
            .downcast_ref::<std::io::Error>()
            .is_some_and(|io| io.kind() == std::io::ErrorKind::BrokenPipe)
    })
}

fn main() {
    std::process::exit(large_stack::run(exit_code));
}

fn exit_code() -> i32 {
    match run() {
        Ok(code) => code as i32,
        Err(error) if reader_went_away(&error) => Exit::Clean as i32,
        Err(error) => {
            eprintln!("Error: {error:#}");
            // Distinct from a failed gate: CI has to be able to tell "your code has
            // problems" from "rqx could not run".
            Exit::Failed as i32
        }
    }
}
