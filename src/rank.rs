//! Frecency + text ranking.
//!
//! Empty query: last-used first, nudged up for sessions in the current
//! directory and ones you resume often. With a query: every word must appear
//! somewhere (title, path, branch, any prompt) or the title/path must fuzzy
//! match; title hits beat prompt hits, and recency still breaks ties.

use crate::history::History;
use crate::index::Session;
use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher, Utf32Str};

pub struct Ctx<'a> {
    pub now: u64,
    pub here: String,
    pub history: &'a History,
}

pub struct Hit {
    pub idx: usize,
    pub score: f64,
    /// Char indices into the title to highlight.
    pub title_marks: Vec<usize>,
}

pub fn last_used(s: &Session, h: &History) -> u64 {
    s.mtime.max(h.last(&s.id))
}

pub fn is_here(s: &Session, here: &str) -> bool {
    s.cwd == here || s.cwd.starts_with(&format!("{here}/"))
}

fn context_score(s: &Session, ctx: &Ctx) -> f64 {
    let age_days = ctx.now.saturating_sub(last_used(s, ctx.history)) as f64 / 86_400.0;
    let recency = 50.0 / (1.0 + age_days);
    // Proximity to the current dir scales recency instead of adding a flat
    // bonus, so an old session here doesn't outrank yesterday's work elsewhere.
    // A parent dir only counts if it's a real project, not $HOME or /.
    let home = dirs::home_dir().map(|h| h.to_string_lossy().into_owned()).unwrap_or_default();
    let is_parent = s.cwd != home && s.cwd != "/" && ctx.here.starts_with(&format!("{}/", s.cwd));
    let near = if s.cwd == ctx.here {
        2.0
    } else if is_here(s, &ctx.here) || is_parent {
        1.5
    } else {
        1.0
    };
    let freq = (5.0 * (1.0 + ctx.history.count(&s.id) as f64).ln()).min(15.0);
    recency * near + freq
}

pub fn rank(sessions: &[Session], query: &str, ctx: &Ctx, here_only: bool) -> Vec<Hit> {
    let query = query.trim();
    let tokens: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
    let pattern = Pattern::parse(query, CaseMatching::Smart, Normalization::Smart);
    let mut matcher = Matcher::new(Config::DEFAULT);
    let qchars = query.chars().filter(|c| !c.is_whitespace()).count().max(1) as f64;
    let mut buf = Vec::new();
    let mut idx_buf = Vec::new();

    let mut hits: Vec<Hit> = sessions
        .iter()
        .enumerate()
        .filter(|(_, s)| !here_only || is_here(s, &ctx.here))
        .filter_map(|(idx, s)| {
            let base = context_score(s, ctx);
            if tokens.is_empty() {
                return Some(Hit {
                    idx,
                    score: base,
                    title_marks: vec![],
                });
            }

            let in_text = tokens.iter().all(|t| s.haystack.contains(t.as_str()));
            let title_lc = s.title.to_lowercase();
            let in_title = tokens.iter().all(|t| title_lc.contains(t.as_str()));
            let cwd_lc = s.cwd.to_lowercase();
            let in_title_or_cwd = tokens
                .iter()
                .all(|t| title_lc.contains(t.as_str()) || cwd_lc.contains(t.as_str()));

            let hay = format!("{}  {}", s.title, s.cwd);
            idx_buf.clear();
            let fuzzy = pattern.indices(Utf32Str::new(&hay, &mut buf), &mut matcher, &mut idx_buf);
            let fuzzy_q = fuzzy.map_or(0.0, |f| f as f64 / (qchars * 16.0));
            // Without a literal hit, only accept tight fuzzy matches (gappy
            // matches over a long title match almost anything).
            if !in_text && fuzzy_q < 1.0 {
                return None;
            }

            let mut m = 0.0;
            if in_text {
                m += 40.0;
                if in_title {
                    m += 30.0;
                } else if in_title_or_cwd {
                    m += 15.0;
                }
            }
            m += fuzzy_q.min(2.0) * 15.0;

            let title_len = s.title.chars().count();
            let mut marks: Vec<usize> = if in_title {
                substring_marks(&s.title, &tokens)
            } else {
                idx_buf
                    .iter()
                    .map(|&i| i as usize)
                    .filter(|&i| i < title_len)
                    .collect()
            };
            marks.sort_unstable();
            marks.dedup();

            Some(Hit {
                idx,
                score: m + base * 0.6,
                title_marks: marks,
            })
        })
        .collect();

    hits.sort_by(|a, b| {
        b.score
            .total_cmp(&a.score)
            .then_with(|| sessions[b.idx].mtime.cmp(&sessions[a.idx].mtime))
    });
    hits
}

/// Char indices of every case-insensitive occurrence of each token in `text`.
pub fn substring_marks(text: &str, tokens: &[String]) -> Vec<usize> {
    let lower: Vec<char> = text.chars().flat_map(char::to_lowercase).collect();
    // Only map back when lowercasing kept char counts aligned (true for ~all text).
    if lower.len() != text.chars().count() {
        return vec![];
    }
    let mut marks = Vec::new();
    for t in tokens {
        let tc: Vec<char> = t.chars().collect();
        if tc.is_empty() || tc.len() > lower.len() {
            continue;
        }
        for start in 0..=lower.len() - tc.len() {
            if lower[start..start + tc.len()] == tc[..] {
                marks.extend(start..start + tc.len());
            }
        }
    }
    marks
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(id: &str, title: &str, cwd: &str, prompts: &[&str], mtime: u64) -> Session {
        let mut s = Session {
            id: id.into(),
            cwd: cwd.into(),
            title: title.into(),
            branch: None,
            prompts: prompts.iter().map(|p| p.to_string()).collect(),
            prompt_count: prompts.len(),
            mtime,
            haystack: String::new(),
        };
        s.haystack = format!("{}\n{}\n{}", s.title, s.cwd, s.prompts.join("\n")).to_lowercase();
        s
    }

    #[test]
    fn empty_query_prefers_recent_then_here() {
        let now = 10 * 86_400;
        let h = History::default();
        let ctx = Ctx {
            now,
            here: "/p/a".into(),
            history: &h,
        };
        let v = vec![
            s("old", "old", "/p/b", &["x"], now - 9 * 86_400),
            s("new", "new", "/p/b", &["x"], now - 60),
            s("here", "here", "/p/a", &["x"], now - 2 * 86_400),
        ];
        let order: Vec<&str> = rank(&v, "", &ctx, false)
            .iter()
            .map(|h| v[h.idx].id.as_str())
            .collect();
        assert_eq!(order, vec!["new", "here", "old"]);
    }

    #[test]
    fn title_hits_beat_prompt_hits_and_misses_drop() {
        let now = 1_000_000;
        let h = History::default();
        let ctx = Ctx {
            now,
            here: "/".into(),
            history: &h,
        };
        let v = vec![
            s("prompt", "something else", "/x", &["fix rate limits"], now),
            s(
                "title",
                "Rate limits for the public API",
                "/y",
                &["hi"],
                now - 86_400,
            ),
            s("none", "zzz", "/z", &["qqq"], now),
        ];
        let hits = rank(&v, "rate limits", &ctx, false);
        let ids: Vec<&str> = hits.iter().map(|h| v[h.idx].id.as_str()).collect();
        assert_eq!(ids, vec!["title", "prompt"]);
    }
}
