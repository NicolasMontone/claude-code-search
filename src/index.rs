//! Discovers Claude Code session transcripts and extracts what we search on.
//!
//! Sessions live in `<claude dir>/projects/<escaped cwd>/<session id>.jsonl`.
//! Parsing every transcript is expensive (hundreds of MB), so results are
//! cached per file and only re-parsed when the file's mtime or size changes.

use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

const CACHE_VERSION: u32 = 1;
const MAX_PROMPTS: usize = 60;
const MAX_PROMPT_CHARS: usize = 400;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub cwd: String,
    pub title: String,
    pub branch: Option<String>,
    /// Human-typed prompts, oldest first (capped at MAX_PROMPTS).
    pub prompts: Vec<String>,
    pub prompt_count: usize,
    /// Last activity, unix seconds (transcript mtime).
    pub mtime: u64,
    /// Lowercased title + cwd + branch + prompts, built after load.
    #[serde(skip)]
    pub haystack: String,
}

#[derive(Serialize, Deserialize)]
struct CacheEntry {
    path: PathBuf,
    mtime: u64,
    size: u64,
    session: Option<Session>,
}

#[derive(Serialize, Deserialize)]
struct Cache {
    version: u32,
    entries: Vec<CacheEntry>,
}

pub fn claude_dir() -> PathBuf {
    std::env::var_os("CLAUDE_CONFIG_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| dirs::home_dir().unwrap_or_default().join(".claude"))
}

fn cache_path() -> PathBuf {
    dirs::cache_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("ccs")
        .join("index.json")
}

/// Every top-level `projects/*/*.jsonl` with its mtime and size.
fn discover(root: &Path) -> Vec<(PathBuf, u64, u64)> {
    let mut out = Vec::new();
    let Ok(projects) = fs::read_dir(root.join("projects")) else {
        return out;
    };
    for project in projects.flatten() {
        let Ok(files) = fs::read_dir(project.path()) else {
            continue;
        };
        for file in files.flatten() {
            let path = file.path();
            if path.extension().is_none_or(|e| e != "jsonl") {
                continue;
            }
            let Ok(meta) = file.metadata() else { continue };
            if !meta.is_file() {
                continue;
            }
            let mtime = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map_or(0, |d| d.as_secs());
            out.push((path, mtime, meta.len()));
        }
    }
    out
}

pub fn load(force_reindex: bool) -> Vec<Session> {
    let root = claude_dir();
    let cache_file = cache_path();

    let old: HashMap<PathBuf, CacheEntry> = if force_reindex {
        HashMap::new()
    } else {
        fs::read(&cache_file)
            .ok()
            .and_then(|b| serde_json::from_slice::<Cache>(&b).ok())
            .filter(|c| c.version == CACHE_VERSION)
            .map(|c| c.entries.into_iter().map(|e| (e.path.clone(), e)).collect())
            .unwrap_or_default()
    };

    let files = discover(&root);
    let entries: Vec<(CacheEntry, bool)> = files
        .into_par_iter()
        .map(|(path, mtime, size)| match old.get(&path) {
            Some(e) if e.mtime == mtime && e.size == size => (
                CacheEntry {
                    path,
                    mtime,
                    size,
                    session: e.session.clone(),
                },
                false,
            ),
            _ => {
                let session = parse_session(&path, mtime);
                (
                    CacheEntry {
                        path,
                        mtime,
                        size,
                        session,
                    },
                    true,
                )
            }
        })
        .collect();

    let changed = entries.iter().any(|(_, fresh)| *fresh) || entries.len() != old.len();
    let entries: Vec<CacheEntry> = entries.into_iter().map(|(e, _)| e).collect();

    let mut sessions: Vec<Session> = entries.iter().filter_map(|e| e.session.clone()).collect();

    if changed {
        if let Some(dir) = cache_file.parent() {
            let _ = fs::create_dir_all(dir);
        }
        let cache = Cache {
            version: CACHE_VERSION,
            entries,
        };
        if let Ok(bytes) = serde_json::to_vec(&cache) {
            let tmp = cache_file.with_extension("json.tmp");
            if fs::write(&tmp, bytes).is_ok() {
                let _ = fs::rename(&tmp, &cache_file);
            }
        }
    }

    for s in &mut sessions {
        s.haystack = build_haystack(s);
    }
    sessions
}

fn build_haystack(s: &Session) -> String {
    let mut h = String::with_capacity(256);
    h.push_str(&s.title);
    h.push('\n');
    h.push_str(&s.cwd);
    h.push('\n');
    if let Some(b) = &s.branch {
        h.push_str(b);
        h.push('\n');
    }
    for p in &s.prompts {
        h.push_str(p);
        h.push('\n');
    }
    h.to_lowercase()
}

fn parse_session(path: &Path, mtime: u64) -> Option<Session> {
    let file = fs::File::open(path).ok()?;
    let reader = BufReader::with_capacity(256 * 1024, file);

    let mut cwd: Option<String> = None;
    let mut ai_title: Option<String> = None;
    let mut summary: Option<String> = None;
    let mut branch: Option<String> = None;
    let mut prompts: Vec<String> = Vec::new();
    let mut prompt_count = 0usize;

    for line in reader.split(b'\n') {
        let Ok(line) = line else { break };
        // Cheap substring checks so we only JSON-parse the few lines we need.
        // Tool results (often huge) are `"type":"user"` too, so skip them early.
        let is_user = contains(&line, br#""type":"user""#);
        let is_title = !is_user && contains(&line, br#""type":"ai-title""#);
        let is_summary = !is_user && !is_title && contains(&line, br#""type":"summary""#);
        if !(is_user || is_title || is_summary) {
            continue;
        }
        if is_user
            && (contains(&line, br#""tool_use_id""#)
                || contains(&line, br#""isMeta":true"#)
                || contains(&line, br#""isSidechain":true"#))
        {
            continue;
        }
        let Ok(v) = serde_json::from_slice::<Value>(&line) else {
            continue;
        };

        if is_title {
            if let Some(t) = v.get("aiTitle").and_then(Value::as_str) {
                ai_title = Some(t.trim().to_string());
            }
            continue;
        }
        if is_summary {
            if let Some(t) = v.get("summary").and_then(Value::as_str) {
                summary = Some(t.trim().to_string());
            }
            continue;
        }

        if cwd.is_none() {
            cwd = v.get("cwd").and_then(Value::as_str).map(str::to_string);
        }
        if let Some(b) = v.get("gitBranch").and_then(Value::as_str) {
            if !b.is_empty() && b != "HEAD" {
                branch = Some(b.to_string());
            }
        }
        let Some(text) = user_text(&v) else { continue };
        let Some(prompt) = clean_prompt(&text) else {
            continue;
        };
        prompt_count += 1;
        if prompts.len() < MAX_PROMPTS {
            prompts.push(prompt);
        } else {
            // Keep the most recent prompt visible: it's often what you remember.
            *prompts.last_mut().unwrap() = prompt;
        }
    }

    let cwd = cwd?;
    if prompts.is_empty() {
        return None;
    }
    let id = path.file_stem()?.to_string_lossy().into_owned();
    let title = ai_title
        .filter(|t| !t.is_empty())
        .or(summary.filter(|t| !t.is_empty()))
        .or_else(|| prompts.iter().find(|p| !p.starts_with('/')).cloned())
        .unwrap_or_else(|| prompts[0].clone());
    let title = truncate_chars(&title, 120);

    Some(Session {
        id,
        cwd,
        title,
        branch,
        prompts,
        prompt_count,
        mtime,
        haystack: String::new(),
    })
}

fn user_text(v: &Value) -> Option<String> {
    let content = v.get("message")?.get("content")?;
    match content {
        Value::String(s) => Some(s.clone()),
        Value::Array(items) => {
            let parts: Vec<&str> = items
                .iter()
                .filter(|i| i.get("type").and_then(Value::as_str) == Some("text"))
                .filter_map(|i| i.get("text").and_then(Value::as_str))
                .collect();
            if parts.is_empty() {
                None
            } else {
                Some(parts.join(" "))
            }
        }
        _ => None,
    }
}

/// Turns a raw user message into a one-line prompt, or drops harness noise.
pub fn clean_prompt(raw: &str) -> Option<String> {
    let t = raw.trim();
    if t.is_empty() {
        return None;
    }
    if let Some(name) = between(t, "<command-name>", "</command-name>") {
        let args = between(t, "<command-args>", "</command-args>").unwrap_or("");
        let cmd = format!("{} {}", name.trim(), args.trim());
        return Some(truncate_chars(cmd.trim(), MAX_PROMPT_CHARS));
    }
    const NOISE: [&str; 8] = [
        "<local-command",
        "<system-reminder>",
        "<bash-",
        "<task-notification",
        "<command-message>",
        "[Request interrupted",
        "Caveat:",
        "<side-sessions-event>",
    ];
    if NOISE.iter().any(|p| t.starts_with(p)) {
        return None;
    }
    let one_line = t.split_whitespace().collect::<Vec<_>>().join(" ");
    Some(truncate_chars(&one_line, MAX_PROMPT_CHARS))
}

fn between<'a>(s: &'a str, open: &str, close: &str) -> Option<&'a str> {
    let start = s.find(open)? + open.len();
    let end = s[start..].find(close)? + start;
    Some(&s[start..end])
}

pub fn truncate_chars(s: &str, max: usize) -> String {
    match s.char_indices().nth(max) {
        Some((i, _)) => format!("{}…", &s[..i]),
        None => s.to_string(),
    }
}

fn contains(hay: &[u8], needle: &[u8]) -> bool {
    hay.windows(needle.len()).any(|w| w == needle)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cleans_slash_commands_and_noise() {
        assert_eq!(
            clean_prompt("<command-name>/model</command-name><command-message>model</command-message><command-args>opus</command-args>").as_deref(),
            Some("/model opus")
        );
        assert_eq!(
            clean_prompt("<local-command-caveat>x</local-command-caveat>"),
            None
        );
        assert_eq!(
            clean_prompt("  fix   the\n bug "),
            Some("fix the bug".into())
        );
    }

    #[test]
    fn parses_a_transcript() {
        let dir = std::env::temp_dir().join(format!("ccs-test-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("abc-123.jsonl");
        let lines = [
            r#"{"type":"last-prompt","sessionId":"abc-123"}"#,
            r#"{"type":"user","message":{"role":"user","content":"add a search cli"},"cwd":"/tmp/proj","gitBranch":"main"}"#,
            r#"{"type":"user","message":{"role":"user","content":[{"tool_use_id":"t1","type":"tool_result","content":"huge"}]},"cwd":"/tmp/proj"}"#,
            r#"{"type":"user","isMeta":true,"message":{"role":"user","content":"meta"},"cwd":"/tmp/proj"}"#,
            r#"{"type":"user","message":{"role":"user","content":[{"type":"text","text":"now in rust"}]},"cwd":"/tmp/proj"}"#,
            r#"{"type":"ai-title","aiTitle":"Session search CLI"}"#,
        ];
        fs::write(&path, lines.join("\n")).unwrap();
        let s = parse_session(&path, 42).unwrap();
        assert_eq!(s.id, "abc-123");
        assert_eq!(s.cwd, "/tmp/proj");
        assert_eq!(s.title, "Session search CLI");
        assert_eq!(s.branch.as_deref(), Some("main"));
        assert_eq!(s.prompts, vec!["add a search cli", "now in rust"]);
        fs::remove_dir_all(dir).unwrap();
    }
}
