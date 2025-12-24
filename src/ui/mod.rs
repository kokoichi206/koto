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
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
};

use crate::app::{App, InputMode};
use crate::domain::todo::Todo;
use crate::repo::TodoRepository;

pub fn run<R: TodoRepository>(mut app: App<R>, tick_rate: Duration) -> Result<()> {
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

fn handle_key<R: TodoRepository>(app: &mut App<R>, code: KeyCode) -> Result<bool> {
    match app.mode {
        InputMode::Normal => match code {
            KeyCode::Char('q') => return Ok(true),
            KeyCode::Char('j') | KeyCode::Down => app.select_next(),
            KeyCode::Char('k') | KeyCode::Up => app.select_previous(),
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
    }

    Ok(false)
}

fn draw<R: TodoRepository>(f: &mut ratatui::Frame, app: &App<R>) {
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

    let mut list_state = ListState::default();
    if !app.todos.is_empty() {
        list_state.select(Some(app.selected));
    }

    let list = render_list(&app.todos, app.selected);
    f.render_stateful_widget(list, chunks[1], &mut list_state);

    let footer = render_footer(app);
    f.render_widget(footer, chunks[2]);
}

fn render_header<R: TodoRepository>(app: &App<R>) -> Paragraph<'static> {
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

fn render_list(todos: &[Todo], selected: usize) -> List<'_> {
    let items: Vec<ListItem> = todos
        .iter()
        .enumerate()
        .map(|(idx, todo)| {
            let symbol = if todo.done { "✔" } else { "•" };
            let mut line = vec![Span::raw(format!(" {symbol} {}", todo.title))];
            if todo.done {
                line.push(Span::styled("  done", Style::default().fg(Color::Green)));
            }

            let style = if idx == selected {
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD | Modifier::REVERSED)
            } else if todo.done {
                Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::CROSSED_OUT)
            } else {
                Style::default()
            };

            ListItem::new(Line::from(line)).style(style)
        })
        .collect();

    List::new(items)
        .block(
            Block::default()
                .title("Todos (j/k move ; a/n add ; Space/Enter toggle ; d delete ; c clear done ; g sync GitHub)")
                .borders(Borders::ALL),
        )
        .highlight_symbol("➤ ")
}

fn render_footer<R: TodoRepository>(app: &App<R>) -> Paragraph<'_> {
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
                    .title("Input (Enter to add / Esc to cancel)")
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
