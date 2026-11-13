use crate::index::Session;
use crate::rank::{self, Ctx, Hit};
use crate::util::{rel_time, short_path};
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap};
use ratatui::{DefaultTerminal, Frame};

pub enum Outcome {
    Quit,
    Resume { idx: usize, fork: bool },
}

struct App<'a> {
    sessions: &'a [Session],
    ctx: &'a Ctx<'a>,
    query: String,
    here_only: bool,
    hits: Vec<Hit>,
    state: ListState,
}

impl App<'_> {
    fn refresh(&mut self) {
        self.hits = rank::rank(self.sessions, &self.query, self.ctx, self.here_only);
        self.state
            .select(if self.hits.is_empty() { None } else { Some(0) });
        *self.state.offset_mut() = 0;
    }

    fn move_by(&mut self, delta: isize) {
        if self.hits.is_empty() {
            return;
        }
        let cur = self.state.selected().unwrap_or(0) as isize;
        let next = (cur + delta).clamp(0, self.hits.len() as isize - 1);
        self.state.select(Some(next as usize));
    }

    fn selected(&self) -> Option<&Hit> {
        self.state.selected().and_then(|i| self.hits.get(i))
    }
}

pub fn run(
    sessions: &[Session],
    ctx: &Ctx,
    query: String,
    here_only: bool,
) -> std::io::Result<Outcome> {
    let mut app = App {
        sessions,
        ctx,
        query,
        here_only,
        hits: vec![],
        state: ListState::default(),
    };
    app.refresh();
    let mut terminal = ratatui::init();
    let result = event_loop(&mut terminal, &mut app);
    ratatui::restore();
    result
}

fn event_loop(terminal: &mut DefaultTerminal, app: &mut App) -> std::io::Result<Outcome> {
    loop {
        terminal.draw(|f| draw(f, app))?;
        let Event::Key(key) = event::read()? else {
            continue;
        };
        if key.kind != KeyEventKind::Press {
            continue;
        }
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            KeyCode::Esc => return Ok(Outcome::Quit),
            KeyCode::Char('c') | KeyCode::Char('d') if ctrl => return Ok(Outcome::Quit),
            KeyCode::Enter => {
                if let Some(h) = app.selected() {
                    return Ok(Outcome::Resume {
                        idx: h.idx,
                        fork: false,
                    });
                }
            }
            KeyCode::Char('f') if ctrl => {
                if let Some(h) = app.selected() {
                    return Ok(Outcome::Resume {
                        idx: h.idx,
                        fork: true,
                    });
                }
            }
            KeyCode::Up => app.move_by(-1),
            KeyCode::Down => app.move_by(1),
            KeyCode::Char('p') | KeyCode::Char('k') if ctrl => app.move_by(-1),
            KeyCode::Char('n') | KeyCode::Char('j') if ctrl => app.move_by(1),
            KeyCode::PageUp => app.move_by(-10),
            KeyCode::PageDown => app.move_by(10),
            KeyCode::Tab | KeyCode::BackTab => {
                app.here_only = !app.here_only;
                app.refresh();
            }
            KeyCode::Char('u') if ctrl => {
                app.query.clear();
                app.refresh();
            }
            KeyCode::Char('w') if ctrl => {
                let trimmed = app.query.trim_end().len();
                app.query.truncate(trimmed);
                let cut = app.query.rfind(char::is_whitespace).map_or(0, |i| i + 1);
                app.query.truncate(cut);
                app.refresh();
            }
            KeyCode::Backspace => {
                app.query.pop();
                app.refresh();
            }
            KeyCode::Char(c) if !ctrl => {
                app.query.push(c);
                app.refresh();
            }
            _ => {}
        }
    }
}

const ACCENT: Color = Color::Rgb(217, 119, 87);

fn draw(f: &mut Frame, app: &mut App) {
    let [top, body, footer] = Layout::vertical([
        Constraint::Length(2),
        Constraint::Min(1),
        Constraint::Length(1),
    ])
    .areas(f.area());

    // Prompt line + scope/count.
    let scope = if app.here_only { "here" } else { "all" };
    let count = format!(" {}/{} · {} ", app.hits.len(), app.sessions.len(), scope);
    let prompt = Line::from(vec![
        Span::styled(" ccs ", Style::new().fg(Color::Black).bg(ACCENT).bold()),
        Span::raw(" › "),
        Span::raw(app.query.clone()),
    ]);
    let [prompt_area, count_area] =
        Layout::horizontal([Constraint::Min(10), Constraint::Length(count.len() as u16)])
            .areas(top);
    f.render_widget(
        Paragraph::new(prompt).block(
            Block::new()
                .borders(Borders::BOTTOM)
                .border_style(Style::new().dim()),
        ),
        prompt_area,
    );
    f.render_widget(
        Paragraph::new(count.dim()).block(
            Block::new()
                .borders(Borders::BOTTOM)
                .border_style(Style::new().dim()),
        ),
        count_area,
    );
    let cursor_x = prompt_area.x + 8 + app.query.chars().count() as u16;
    f.set_cursor_position((
        cursor_x.min(prompt_area.right().saturating_sub(1)),
        prompt_area.y,
    ));

    let show_preview = body.width >= 110;
    let (list_area, preview_area) = if show_preview {
        let [l, p] = Layout::horizontal([Constraint::Percentage(58), Constraint::Percentage(42)])
            .areas(body);
        (l, Some(p))
    } else {
        (body, None)
    };

    draw_list(f, app, list_area);
    if let Some(area) = preview_area {
        draw_preview(f, app, area);
    }

    let keys = Line::from(vec![
        " enter".bold(),
        " resume  ".dim(),
        "ctrl-f".bold(),
        " fork  ".dim(),
        "tab".bold(),
        " here/all  ".dim(),
        "ctrl-u".bold(),
        " clear  ".dim(),
        "esc".bold(),
        " quit".dim(),
    ]);
    f.render_widget(Paragraph::new(keys), footer);
}

fn draw_list(f: &mut Frame, app: &mut App, area: Rect) {
    let path_w = ((area.width as usize) / 4).clamp(14, 32);
    let items: Vec<ListItem> = app
        .hits
        .iter()
        .map(|h| {
            let s = &app.sessions[h.idx];
            let when = rel_time(app.ctx.now, rank::last_used(s, app.ctx.history));
            let here = rank::is_here(s, &app.ctx.here);
            let path = fit_left(&short_path(&s.cwd), path_w);
            let mut spans = vec![
                Span::styled(format!(" {when:>4} "), Style::new().dim()),
                Span::styled(
                    format!("{path:<path_w$} "),
                    if here {
                        Style::new().fg(ACCENT)
                    } else {
                        Style::new().fg(Color::Cyan)
                    },
                ),
            ];
            spans.extend(highlight(&s.title, &h.title_marks));
            ListItem::new(Line::from(spans))
        })
        .collect();

    let list = List::new(items)
        .highlight_style(
            Style::new()
                .bg(Color::Rgb(50, 50, 60))
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▌");
    if app.hits.is_empty() {
        let msg = if app.sessions.is_empty() {
            "  no sessions found in ~/.claude/projects"
        } else {
            "  no matches"
        };
        f.render_widget(Paragraph::new(msg.dim()), area);
    } else {
        f.render_stateful_widget(list, area, &mut app.state);
    }
}

fn draw_preview(f: &mut Frame, app: &App, area: Rect) {
    let block = Block::new()
        .borders(Borders::LEFT)
        .border_style(Style::new().dim())
        .padding(ratatui::widgets::Padding::horizontal(1));
    let Some(h) = app.selected() else {
        f.render_widget(block, area);
        return;
    };
    let s = &app.sessions[h.idx];
    let tokens: Vec<String> = app
        .query
        .split_whitespace()
        .map(str::to_lowercase)
        .collect();

    let mut lines = vec![
        Line::from(s.title.clone().bold()),
        Line::from(""),
        kv("dir", short_path(&s.cwd)),
    ];
    if let Some(b) = &s.branch {
        lines.push(kv("branch", b.clone()));
    }
    lines.push(kv(
        "used",
        format!(
            "{} ago",
            rel_time(app.ctx.now, rank::last_used(s, app.ctx.history))
        ),
    ));
    lines.push(kv("prompts", s.prompt_count.to_string()));
    let resumed = app.ctx.history.count(&s.id);
    if resumed > 0 {
        lines.push(kv("resumed", format!("{resumed}× via ccs")));
    }
    lines.push(kv("id", s.id.clone()));
    if !std::path::Path::new(&s.cwd).is_dir() {
        lines.push(Line::from("dir no longer exists".fg(Color::Red)));
    }
    lines.push(Line::from(""));

    // Prompts that match the query first, then the rest in order.
    let (matching, rest): (Vec<&String>, Vec<&String>) = s.prompts.iter().partition(|p| {
        !tokens.is_empty() && {
            let lc = p.to_lowercase();
            tokens.iter().any(|t| lc.contains(t.as_str()))
        }
    });
    if !matching.is_empty() {
        lines.push(Line::from("matching".fg(ACCENT)));
        for p in matching.iter().take(8) {
            lines.push(prompt_line(p, &tokens));
        }
        lines.push(Line::from(""));
    }
    lines.push(Line::from("prompts".dim()));
    for p in rest.iter().take(40) {
        lines.push(prompt_line(p, &tokens));
    }

    f.render_widget(
        Paragraph::new(lines).block(block).wrap(Wrap { trim: true }),
        area,
    );
}

fn kv(k: &str, v: String) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{k:<8}"), Style::new().dim()),
        Span::raw(v),
    ])
}

fn prompt_line(p: &str, tokens: &[String]) -> Line<'static> {
    let shown = crate::index::truncate_chars(p, 220);
    let marks = rank::substring_marks(&shown, tokens);
    let mut spans = vec![Span::styled("› ", Style::new().dim())];
    spans.extend(highlight(&shown, &marks));
    Line::from(spans)
}

/// Splits `text` into spans, styling the chars at `marks` (sorted char indices).
fn highlight(text: &str, marks: &[usize]) -> Vec<Span<'static>> {
    if marks.is_empty() {
        return vec![Span::raw(text.to_string())];
    }
    let hl = Style::new().fg(ACCENT).add_modifier(Modifier::BOLD);
    let mut spans = Vec::new();
    let mut cur = String::new();
    let mut cur_hl = false;
    let mut m = marks.iter().peekable();
    for (i, c) in text.chars().enumerate() {
        let is_hl = m.peek().is_some_and(|&&x| x == i);
        if is_hl {
            m.next();
        }
        if is_hl != cur_hl && !cur.is_empty() {
            let chunk = std::mem::take(&mut cur);
            spans.push(if cur_hl {
                Span::styled(chunk, hl)
            } else {
                Span::raw(chunk)
            });
        }
        cur_hl = is_hl;
        cur.push(c);
    }
    if !cur.is_empty() {
        spans.push(if cur_hl {
            Span::styled(cur, hl)
        } else {
            Span::raw(cur)
        });
    }
    spans
}

/// Keeps the end of a path (the interesting part) when it doesn't fit.
fn fit_left(s: &str, w: usize) -> String {
    let n = s.chars().count();
    if n <= w {
        return s.to_string();
    }
    let tail: String = s.chars().skip(n - (w - 1)).collect();
    format!("…{tail}")
}
