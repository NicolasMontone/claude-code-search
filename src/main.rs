mod history;
mod index;
mod rank;
mod tui;
mod util;

use clap::Parser;
use std::io::IsTerminal;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Command, ExitCode};

/// Fuzzy-find Claude Code sessions across every project and resume one in its directory.
#[derive(Parser)]
#[command(name = "ccs", version, after_help = KEYS)]
struct Args {
    /// Initial search query
    query: Vec<String>,

    /// Only sessions started in (or under) the current directory
    #[arg(long)]
    here: bool,

    /// Print matches instead of opening the picker
    #[arg(short, long)]
    list: bool,

    /// With --list: print JSON
    #[arg(long)]
    json: bool,

    /// With --list: how many results
    #[arg(short = 'n', long, default_value_t = 20)]
    limit: usize,

    /// Resume the top match right away without the picker
    #[arg(short = '1', long = "first")]
    first: bool,

    /// Resume as a new forked session (claude --fork-session)
    #[arg(long)]
    fork: bool,

    /// Ignore the cache and re-parse every transcript
    #[arg(long)]
    reindex: bool,

    /// Extra args passed to `claude` after `--`
    #[arg(last = true)]
    claude_args: Vec<String>,
}

const KEYS: &str = "\
Picker keys:
  type to search · ↑/↓ ctrl-n/p move · enter resume · ctrl-f fork
  tab toggle here/all · ctrl-u clear · ctrl-w delete word · esc quit

Shell: to stay in the project dir after claude exits, see README (CCS_CD_FILE).";

fn main() -> ExitCode {
    let args = Args::parse();
    let sessions = index::load(args.reindex);
    let mut history = history::History::load();
    let now = util::now();
    let here = std::env::current_dir()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default();
    let query = args.query.join(" ");

    let interactive = std::io::stdout().is_terminal() && std::io::stdin().is_terminal();

    let pick = {
        let ctx = rank::Ctx {
            now,
            here: here.clone(),
            history: &history,
        };
        if args.list || !interactive {
            print_list(&sessions, &ctx, &query, args.here, args.limit, args.json);
            return ExitCode::SUCCESS;
        }
        if args.first {
            match rank::rank(&sessions, &query, &ctx, args.here).first() {
                Some(h) => (h.idx, args.fork),
                None => {
                    eprintln!("ccs: no session matches {query:?}");
                    return ExitCode::FAILURE;
                }
            }
        } else {
            match tui::run(&sessions, &ctx, query, args.here) {
                Ok(tui::Outcome::Resume { idx, fork }) => (idx, fork || args.fork),
                Ok(tui::Outcome::Quit) => return ExitCode::SUCCESS,
                Err(e) => {
                    eprintln!("ccs: terminal error: {e}");
                    return ExitCode::FAILURE;
                }
            }
        }
    };

    let (idx, fork) = pick;
    let s = &sessions[idx];
    resume(s, fork, &args.claude_args, &mut history, now)
}

fn resume(
    s: &index::Session,
    fork: bool,
    extra: &[String],
    history: &mut history::History,
    now: u64,
) -> ExitCode {
    if !Path::new(&s.cwd).is_dir() {
        eprintln!(
            "ccs: {} no longer exists; claude resolves sessions by directory, so it can't be resumed there.",
            s.cwd
        );
        eprintln!("     session id: {}", s.id);
        return ExitCode::FAILURE;
    }
    history.record(&s.id, now);
    if let Some(f) = std::env::var_os("CCS_CD_FILE") {
        let _ = std::fs::write(f, &s.cwd);
    }
    eprintln!(
        "\x1b[2m→ {}  ·  {}\x1b[0m",
        s.title,
        util::short_path(&s.cwd)
    );

    let mut cmd = Command::new("claude");
    cmd.current_dir(&s.cwd)
        .env("PWD", &s.cwd)
        .arg("--resume")
        .arg(&s.id);
    if fork {
        cmd.arg("--fork-session");
    }
    cmd.args(extra);
    let err = cmd.exec(); // only returns on failure
    eprintln!("ccs: couldn't start claude: {err}");
    ExitCode::FAILURE
}

fn print_list(
    sessions: &[index::Session],
    ctx: &rank::Ctx,
    query: &str,
    here: bool,
    limit: usize,
    json: bool,
) {
    let hits = rank::rank(sessions, query, ctx, here);
    let top = hits.iter().take(limit).map(|h| &sessions[h.idx]);
    if json {
        let rows: Vec<_> = top
            .map(|s| {
                serde_json::json!({
                    "id": s.id,
                    "cwd": s.cwd,
                    "title": s.title,
                    "branch": s.branch,
                    "prompts": s.prompt_count,
                    "last_used": rank::last_used(s, ctx.history),
                })
            })
            .collect();
        println!(
            "{}",
            serde_json::to_string_pretty(&rows).unwrap_or_default()
        );
        return;
    }
    for s in top {
        println!(
            "{:>4}  {:<36}  {}  {}",
            util::rel_time(ctx.now, rank::last_used(s, ctx.history)),
            util::short_path(&s.cwd),
            s.title,
            s.id
        );
    }
}
