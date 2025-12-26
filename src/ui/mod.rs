use std::io::{Stdout, stdout};
use std::time::{Duration, Instant};

use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Margin, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Cell, Clear, Paragraph, Row, Table, TableState, Wrap},
};

use crate::app::{App, HelpMode, InputMode};
use crate::domain::todo::{Priority, Todo};
use time::{OffsetDateTime, macros::format_description};

pub fn run(mut app: App, tick_rate: Duration) -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut last_tick = Instant::now();
    let res = loop {
        app.poll_sync();
        terminal.draw(|f| draw(f, &app))?;

        let timeout = tick_rate
            .checked_sub(last_tick.elapsed())
            .unwrap_or_else(|| Duration::from_secs(0));

        if event::poll(timeout)?
            && let Event::Key(key) = event::read()?
            && key.kind == KeyEventKind::Press
            && handle_key(&mut app, key.code)?
        {
            break Ok(());
        }

        if last_tick.elapsed() >= tick_rate {
            last_tick = Instant::now();
        }
    };

    cleanup_terminal(&mut terminal)?;
    res
}

fn handle_key(app: &mut App, code: KeyCode) -> Result<bool> {
    if app.mode == InputMode::Normal && app.help_mode != HelpMode::None {
        if app.help_mode == HelpMode::Full && app.help_searching {
            match code {
                KeyCode::Esc => {
                    app.help_searching = false;
                }
                KeyCode::Enter => {
                    if let Some(line) = help_matches(&app.help_search_query).first().copied() {
                        app.help_scroll = (line.saturating_sub(1)) as u16;
                        app.help_search_match = 0;
                    }
                    app.help_searching = false;
                }
                KeyCode::Backspace => {
                    app.help_search_query.pop();
                    app.help_search_match = 0;
                }
                KeyCode::Char(c) => {
                    if !c.is_control() {
                        app.help_search_query.push(c);
                        app.help_search_match = 0;
                    }
                }
                _ => {}
            }
            return Ok(false);
        }

        match code {
            KeyCode::Char('h') | KeyCode::Char('?') => app.toggle_help_quick(),
            KeyCode::Char('H') => app.toggle_help_full(),
            KeyCode::Esc => app.close_help(),
            KeyCode::Char('q') => return Ok(true),
            KeyCode::Char('/') if app.help_mode == HelpMode::Full => {
                app.help_searching = true;
                app.help_search_query.clear();
                app.help_search_match = 0;
            }
            KeyCode::Char('n') if app.help_mode == HelpMode::Full => {
                jump_to_next_match(app, true);
            }
            KeyCode::Char('N') if app.help_mode == HelpMode::Full => {
                jump_to_next_match(app, false);
            }
            KeyCode::Char('g') | KeyCode::Home => app.help_scroll = 0,
            KeyCode::Char('G') | KeyCode::End => {
                app.help_scroll = app.help_scroll.saturating_add(10_000)
            }
            KeyCode::Char('j') | KeyCode::Down => {
                app.help_scroll = app.help_scroll.saturating_add(1)
            }
            KeyCode::Char('k') | KeyCode::Up => app.help_scroll = app.help_scroll.saturating_sub(1),
            KeyCode::PageDown => app.help_scroll = app.help_scroll.saturating_add(10),
            KeyCode::PageUp => app.help_scroll = app.help_scroll.saturating_sub(10),
            _ => {}
        }
        return Ok(false);
    }

    match app.mode {
        InputMode::Normal => match code {
            KeyCode::Char('q') => return Ok(true),
            KeyCode::Char('j') | KeyCode::Down => app.select_next(),
            KeyCode::Char('k') | KeyCode::Up => app.select_previous(),
            KeyCode::Char('P') => app.cycle_priority_selected(),
            KeyCode::Char(']') => app.shift_due_selected(1),
            KeyCode::Char('[') => app.shift_due_selected(-1),
            KeyCode::Char('D') => app.clear_due_selected(),
            KeyCode::Char('t') => app.edit_due(),
            KeyCode::Char('h') | KeyCode::Char('?') => app.toggle_help_quick(),
            KeyCode::Char('H') => app.toggle_help_full(),
            KeyCode::Char('a') | KeyCode::Char('n') => {
                app.mode = InputMode::Editing;
                app.input.clear();
                app.set_status("Type new task and press Enter");
            }
            KeyCode::Enter => {
                if !app.open_selected_link() {
                    app.toggle_selected();
                }
            }
            KeyCode::Char(' ') => app.toggle_selected(),
            KeyCode::Char('d') | KeyCode::Delete => app.delete_selected(),
            KeyCode::Char('c') => app.clear_done(),
            KeyCode::Char('r') => {
                app.reload();
                app.set_status("Reloaded");
            }
            KeyCode::Char('g') => {
                app.start_sync_github();
            }
            _ => {}
        },
        InputMode::Editing => match code {
            KeyCode::Esc => {
                app.mode = InputMode::Normal;
                app.input.clear();
                app.set_status("Canceled");
            }
            KeyCode::Enter => app.add_todo(),
            KeyCode::Backspace => {
                app.input.pop();
            }
            KeyCode::Char(c) => app.input.push(c),
            _ => {}
        },
        InputMode::EditingDue => match code {
            KeyCode::Esc => {
                app.mode = InputMode::Normal;
                app.input.clear();
                app.set_status("Canceled");
            }
            KeyCode::Enter => app.apply_due_edit(),
            KeyCode::Backspace => {
                app.input.pop();
            }
            KeyCode::Char(c) => app.input.push(c),
            _ => {}
        },
    }

    Ok(false)
}

fn draw(f: &mut ratatui::Frame, app: &App) {
    let size = f.area();

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(1),
            Constraint::Length(3),
        ])
        .split(size);

    let header = render_header(app);
    f.render_widget(header, chunks[0]);

    let mut table_state = TableState::default();
    if !app.todos.is_empty() {
        table_state.select(Some(app.selected));
    }

    let table = render_table(&app.todos);
    f.render_stateful_widget(table, chunks[1], &mut table_state);

    let footer = render_footer(app);
    f.render_widget(footer, chunks[2]);

    if app.help_mode != HelpMode::None {
        // Keep a consistent 1-cell padding around the help modal, since percentage-based centering
        // can round the outer margin down to 0 on small terminals (making it look "stuck" to edges).
        let area = centered_rect(95, 95, size).inner(Margin::new(1, 1));
        f.render_widget(Clear, area);
        let scroll = clamp_help_scroll(app.help_mode, app.help_scroll, area);
        let title = help_title(app);
        let help = render_help(app.help_mode, scroll, title);
        f.render_widget(help, area);
    }
}

fn render_header(app: &App) -> Paragraph<'static> {
    let total = app.todos.len();
    let done = app.todos.iter().filter(|t| t.done).count();
    let summary = format!("Open: {} / All: {}", total.saturating_sub(done), total);
    let mut spans = vec![
        Span::styled("koto - todo", Style::default().fg(Color::Cyan)),
        Span::raw("  |  "),
        Span::styled(summary, Style::default().fg(Color::Yellow)),
    ];
    if app.is_syncing {
        spans.push(Span::raw("  |  "));
        spans.push(Span::styled(
            "⏳ Syncing GitHub...",
            Style::default().fg(Color::Magenta),
        ));
    }
    let line = Line::from(spans);
    Paragraph::new(line)
        .block(Block::default().title("Overview").borders(Borders::ALL))
        .wrap(Wrap { trim: true })
}

fn render_table(todos: &[Todo]) -> Table<'_> {
    let rows: Vec<Row> = todos
        .iter()
        .map(|todo| {
            let pri = render_priority(todo.priority);
            let (due_text, due_style) = render_due(todo.due);
            let symbol = if todo.done { "✔" } else { "•" };
            let title = format!("{symbol} {}", todo.title);

            let row_style = if todo.done {
                Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::CROSSED_OUT)
            } else {
                Style::default()
            };

            Row::new(vec![
                Cell::from(pri),
                Cell::from(due_text).style(due_style),
                Cell::from(title),
            ])
            .style(row_style)
        })
        .collect();

    Table::new(
        rows,
        [
            Constraint::Length(10),
            Constraint::Length(22),
            Constraint::Min(20),
        ],
    )
        .header(
            Row::new(vec!["Priority", "Due", "Title"]).style(
                Style::default().add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
            ),
        )
        .block(
            Block::default()
                .title("Todos (h help ; H manual ; j/k move ; a/n add ; Enter open link ; Space toggle ; P cycle prio ; t set due ; [/ ] shift due ; D clear due ; d delete ; c clear done ; g sync GitHub)")
                .borders(Borders::ALL),
        )
        .column_spacing(2)
        .highlight_symbol("➤ ")
        .row_highlight_style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD | Modifier::REVERSED),
        )
}

fn render_footer(app: &App) -> Paragraph<'_> {
    match app.mode {
        InputMode::Normal => {
            let msg = app
                .status
                .as_deref()
                .unwrap_or("q quit ; h help ; H manual ; a add ; c clear done ; r reload");
            Paragraph::new(msg).block(Block::default().title("Normal").borders(Borders::ALL))
        }
        InputMode::Editing => {
            let line = Line::from(vec![
                Span::raw("New task: "),
                Span::styled(&app.input, Style::default().fg(Color::Yellow)),
                Span::raw("█"),
            ]);
            Paragraph::new(line).block(
                Block::default()
                    .title("Input (e.g. \"buy milk p:1 d:+2\" / Enter to add / Esc to cancel)")
                    .borders(Borders::ALL),
            )
        }
        InputMode::EditingDue => {
            let line = Line::from(vec![
                Span::raw("Set due: "),
                Span::styled(&app.input, Style::default().fg(Color::Yellow)),
                Span::raw("█"),
            ]);
            Paragraph::new(line).block(
                Block::default()
                    .title("Set due (e.g. d:+3 / today / 2025-01-05 / Enter to confirm / Esc to cancel)")
                    .borders(Borders::ALL),
            )
        }
    }
}

fn cleanup_terminal(terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> Result<()> {
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(())
}

fn render_priority(priority: Priority) -> Span<'static> {
    match priority {
        Priority::High => Span::styled("▲ High", Style::default().fg(Color::Red)),
        Priority::Medium => Span::styled("△ Med", Style::default().fg(Color::Yellow)),
        Priority::Low => Span::styled("▽ Low", Style::default().fg(Color::Blue)),
    }
}

fn render_due(due: Option<std::time::SystemTime>) -> (String, Style) {
    let fmt = format_description!("[year]-[month]-[day]");
    match due {
        None => ("No due".to_string(), Style::default().fg(Color::Gray)),
        Some(t) => {
            let odt: OffsetDateTime = t.into();
            let date_str = odt.format(&fmt).unwrap_or_else(|_| "invalid".into());

            // Compute calendar-day difference (UTC) to avoid today becoming tomorrow around midnight.
            let today_date = OffsetDateTime::now_utc().date();
            let due_date = odt.date();
            let days_diff = (due_date.to_julian_day() - today_date.to_julian_day()) as i64;

            let (label, color) = match days_diff {
                d if d < 0 => (format!("{date_str} ({:>2}d overdue)", -d), Color::Red),
                0 => (format!("{date_str} (today)"), Color::Yellow),
                1 => (format!("{date_str} (tomorrow)"), Color::Yellow),
                d => (format!("{date_str} (in {}d)", d), Color::Green),
            };
            (label, Style::default().fg(color))
        }
    }
}

fn render_help<'a>(mode: HelpMode, scroll: u16, title: String) -> Paragraph<'a> {
    let (title, text) = match mode {
        HelpMode::None => (title, Text::from("")),
        HelpMode::Quick => (title, help_text_quick()),
        HelpMode::Full => (title, help_text_full()),
    };

    Paragraph::new(text)
        .block(Block::default().title(title).borders(Borders::ALL))
        .wrap(Wrap { trim: true })
        .scroll((scroll, 0))
        .style(Style::default().bg(Color::Black).fg(Color::White))
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

fn help_text_quick() -> Text<'static> {
    Text::from(vec![
        Line::from(vec![
            Span::styled("koto — quick help", Style::default().fg(Color::Cyan)),
            Span::raw("  "),
            Span::styled("(Esc to close)", Style::default().fg(Color::Gray)),
        ]),
        Line::from(""),
        Line::from("Navigation: j/k or Up/Down"),
        Line::from("Add task: a or n"),
        Line::from("Toggle done: Space or Enter"),
        Line::from("Delete task: d or Delete"),
        Line::from("Clear done: c"),
        Line::from("Priority: P (cycle)"),
        Line::from("Due date: t (edit), [ / ] (shift), D (clear)"),
        Line::from("Reload: r"),
        Line::from("GitHub sync: g"),
        Line::from("Quit: q"),
        Line::from(""),
        Line::from(vec![
            Span::styled("Tip:", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" press "),
            Span::styled("H", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" for the full manual."),
        ]),
    ])
}

fn help_text_full() -> Text<'static> {
    Text::from(vec![
        Line::from(vec![
            Span::styled("koto — manual", Style::default().fg(Color::Cyan)),
            Span::raw("  "),
            Span::styled(
                "j/k scroll • g/G top/bottom • Esc close",
                Style::default().fg(Color::Gray),
            ),
        ]),
        Line::from(""),
        Line::from(vec![Span::styled(
            "KEYS",
            Style::default().add_modifier(Modifier::BOLD),
        )]),
        Line::from("  j / k, Up / Down        Move selection (or scroll in this manual)"),
        Line::from("  a / n                   Add a new todo (type, then Enter)"),
        Line::from("  Enter / Space           Toggle done"),
        Line::from("  d / Delete              Delete selected"),
        Line::from("  c                       Clear all completed"),
        Line::from("  r                       Reload from storage"),
        Line::from("  P                       Cycle priority (High → Med → Low)"),
        Line::from("  t                       Edit due date for selected"),
        Line::from("  [ / ]                   Shift due date by -1 / +1 day"),
        Line::from("  D                       Clear due date"),
        Line::from("  g                       Sync GitHub review-requested PRs"),
        Line::from("  h / ?                   Quick help"),
        Line::from("  H                       This manual"),
        Line::from("  q                       Quit"),
        Line::from(""),
        Line::from(vec![Span::styled(
            "TASK INPUT",
            Style::default().add_modifier(Modifier::BOLD),
        )]),
        Line::from("You can type inline meta when adding a task:"),
        Line::from("  \"buy milk p:1 d:+2\""),
        Line::from("Priority tokens: p:1 / p:2 / p:3 (also: high/medium/low)"),
        Line::from("Due tokens: d:+N, today, tomorrow, YYYY-MM-DD"),
        Line::from(""),
        Line::from(vec![Span::styled(
            "GITHUB SYNC",
            Style::default().add_modifier(Modifier::BOLD),
        )]),
        Line::from("Press 'g' to fetch PRs that explicitly request you as a reviewer."),
        Line::from("Each PR becomes a todo: owner/repo#num by author: title"),
        Line::from("Sync runs in the background; the header shows status while syncing."),
        Line::from(""),
        Line::from("Auth resolution order:"),
        Line::from("  1) GITHUB_TOKEN (preferred)"),
        Line::from("  2) gh auth token (requires: gh auth login)"),
        Line::from("GitHub Enterprise: set GH_HOST (e.g. github.example.com) so"),
        Line::from("  gh auth token --hostname $GH_HOST"),
        Line::from("is used."),
        Line::from(""),
        Line::from(vec![Span::styled(
            "NOTES",
            Style::default().add_modifier(Modifier::BOLD),
        )]),
        Line::from("If GitHub auth is not available, the app still works without sync."),
    ])
}

fn help_line_count(mode: HelpMode) -> usize {
    match mode {
        HelpMode::None => 0,
        HelpMode::Quick => help_text_quick().lines.len(),
        HelpMode::Full => help_text_full().lines.len(),
    }
}

fn clamp_help_scroll(mode: HelpMode, requested: u16, area: Rect) -> u16 {
    let total_lines = help_line_count(mode);
    let viewport_lines = area.height.saturating_sub(2) as usize; // borders
    let max_scroll = total_lines.saturating_sub(viewport_lines);
    (requested as usize).min(max_scroll) as u16
}

fn help_title(app: &App) -> String {
    match app.help_mode {
        HelpMode::None => "Help".to_string(),
        HelpMode::Quick => "Help (Esc close)".to_string(),
        HelpMode::Full => {
            if app.help_searching {
                format!(
                    "Manual — /{}  (Enter jump, Esc cancel)",
                    app.help_search_query
                )
            } else if app.help_search_query.trim().is_empty() {
                "Manual (/ search, n/N next, g/G top/bottom, Esc close)".to_string()
            } else {
                format!(
                    "Manual — last /{}  (press / to search, n/N next, Esc close)",
                    app.help_search_query
                )
            }
        }
    }
}

fn help_matches(query: &str) -> Vec<usize> {
    let q = query.trim();
    if q.is_empty() {
        return Vec::new();
    }
    let q = q.to_lowercase();
    help_text_full()
        .lines
        .iter()
        .enumerate()
        .filter_map(|(idx, line)| {
            let plain = line
                .spans
                .iter()
                .map(|s| s.content.as_ref())
                .collect::<String>();
            if plain.to_lowercase().contains(&q) {
                Some(idx)
            } else {
                None
            }
        })
        .collect()
}

fn jump_to_next_match(app: &mut App, forward: bool) {
    let matches = help_matches(&app.help_search_query);
    if matches.is_empty() {
        return;
    }

    let len = matches.len();
    let cur = app.help_search_match.min(len.saturating_sub(1));
    let next = if forward {
        (cur + 1) % len
    } else {
        (cur + len - 1) % len
    };

    app.help_search_match = next;
    let line = matches[next];
    app.help_scroll = (line.saturating_sub(1)) as u16;
}
