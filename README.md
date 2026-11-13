# ccs

Fuzzy-find every Claude Code session on your machine and resume it in its project directory.

```
 ccs  › rate limit                                            2/20 · all
  2h ~/code/storefront   Checkout rate limiting with Upstash      │ Checkout rate limiting with Upstash
  3d ~/code/api          Rate limiter middleware for public API   │ dir     ~/code/storefront
                                                                  │ branch  feat/checkout-ratelimit
                                                                  │ matching
                                                                  │ › add rate limiting to the checkout api…
 enter resume  ctrl-f fork  tab here/all  ctrl-u clear  esc quit
```

`claude --resume` only lists sessions for the directory you're in. `ccs` reads all of `~/.claude/projects/*/*.jsonl`, lets you search titles, paths, branches and every prompt you typed, then runs `claude --resume <id>` from the session's original directory.

## Install

```sh
cargo install --path .
```

## Use

```sh
ccs                      # picker, most relevant first
ccs rate limits          # picker with a query
ccs --here               # only sessions in/under the current dir (tab toggles)
ccs -1 swift review      # resume the top match, no picker
ccs --fork               # resume as a new forked session
ccs -l -n 30 [query]     # print matches (also used when not a tty)
ccs -l --json            # machine-readable
ccs -- --model opus      # anything after -- goes to claude
```

Keys: type to search · ↑/↓ or ctrl-n/p · enter resume · ctrl-f fork · tab here/all · ctrl-u clear · ctrl-w delete word · esc quit.

## Ranking

- **No query:** frecency. Last activity decays (`50 / (1 + days)`), plus a boost for sessions in the current directory (or parent/child of it) and for sessions you resume often through `ccs`.
- **Query:** every word must appear somewhere (title, path, branch, any prompt), or the title/path must be a tight fuzzy match. Title hits outrank prompt hits; frecency breaks ties.

Titles come from Claude's own `ai-title`, then `summary`, then the first non-slash prompt.

## Staying in the project after claude exits

`ccs` hands the terminal to `claude` with `exec`, so your shell's directory doesn't change. To `cd` there when claude exits, wrap it:

```zsh
ccs() { local f=$(mktemp); CCS_CD_FILE=$f command ccs "$@"; local d=$(<$f); rm -f $f; [[ -n $d ]] && cd "$d"; }
```

## Files

- Index cache: `~/Library/Caches/ccs/index.json` (macOS) / `~/.cache/ccs/` — re-parses only transcripts whose mtime or size changed. `--reindex` rebuilds it.
- Resume history: `~/Library/Application Support/ccs/history.json` / `~/.local/share/ccs/`.
- Honors `CLAUDE_CONFIG_DIR`.
