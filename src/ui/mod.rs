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
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Paragraph, Row, Table, TableState, Wrap},
};

use crate::app::{App, InputMode};
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
            KeyCode::Char('a') | KeyCode::Char('n') => {
                app.mode = InputMode::Editing;
                app.input.clear();
                app.set_status("Type new task and press Enter");
            }
            KeyCode::Enter | KeyCode::Char(' ') => app.toggle_selected(),
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
                .title("Todos (j/k move ; a/n add ; Space/Enter toggle ; P cycle prio ; t set due ; [/ ] shift due ; D clear due ; d delete ; c clear done ; g sync GitHub)")
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
                .unwrap_or("q quit ; a add ; c clear done ; r reload");
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
