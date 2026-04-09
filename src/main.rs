mod chain;
mod ctx;
mod persist;
mod wrapped;

use crate::chain::publish_memo;
use anchor_lang::prelude::*;
use ansi_to_tui::IntoText;
use anyhow::Result;
use arboard::Clipboard;
use crossbeam_channel::{Receiver, Sender, unbounded};
use crossterm::event::{
    self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseEvent, MouseEventKind,
};
pub use ctx::Ctx;
use dotenvy::dotenv;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Cell, List, ListItem, ListState, Row, Table, TableState};
use ratatui::{
    DefaultTerminal, Frame,
    layout::Rect,
    widgets::{Block, Clear, Paragraph},
};
use std::collections::BTreeMap;
use std::io::{self, Write};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use tracing_subscriber::fmt::MakeWriter;

pub enum UiEvent {
    MemoPublished(usize),
    MemoInbox(Vec<(String, memos::Memo)>),
    Stored(Vec<(String, memos::Memo)>),
    Log(String),
}

struct ChannelWriter {
    tx: Sender<UiEvent>,
    buf: Vec<u8>,
}

impl Write for ChannelWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.buf.extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        if let Ok(s) = String::from_utf8(self.buf.drain(..).collect()) {
            let trimmed = s.trim().to_string();
            if !trimmed.is_empty() {
                let _ = self.tx.send(UiEvent::Log(trimmed));
            }
        }
        Ok(())
    }
}

impl Drop for ChannelWriter {
    fn drop(&mut self) {
        let _ = self.flush();
    }
}

struct ChannelMakeWriter {
    tx: Sender<UiEvent>,
}

impl<'a> MakeWriter<'a> for ChannelMakeWriter {
    type Writer = ChannelWriter;

    fn make_writer(&'a self) -> Self::Writer {
        ChannelWriter {
            tx: self.tx.clone(),
            buf: Vec::new(),
        }
    }
}

static ID: AtomicUsize = AtomicUsize::new(0);

struct App {
    ctx: Arc<Ctx>,
    clipboard: Option<Clipboard>,
    exit: bool,
    pending_memo: String,
    cursor_pos: usize,
    rx: Receiver<UiEvent>,

    focus: Pane,

    outbox: Vec<(usize, String)>,
    outbox_list_state: ListState,

    inbox: BTreeMap<(u64, String), String>,
    inbox_list_state: ListState,

    storebox: BTreeMap<(u64, String), String>,
    storebox_list_state: TableState,

    logs: Vec<String>,
    logs_list_state: ListState,
}

#[derive(PartialEq, Eq)]
enum Pane {
    Outbox,
    Inbox,
    Input,
    Stored,
    Logs,
}

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let vertical = Layout::vertical([
        Constraint::Percentage((100 - percent_y) / 2),
        Constraint::Percentage(percent_y),
        Constraint::Percentage((100 - percent_y) / 2),
    ])
    .split(area);

    Layout::horizontal([
        Constraint::Percentage((100 - percent_x) / 2),
        Constraint::Percentage(percent_x),
        Constraint::Percentage((100 - percent_x) / 2),
    ])
    .split(vertical[1])[1]
}

impl App {
    fn new() -> Result<Self> {
        let (tx, rx) = unbounded();
        Ok(Self {
            ctx: Arc::new(Ctx::new(tx)?),
            clipboard: Clipboard::new().ok(),
            exit: false,
            pending_memo: String::new(),
            cursor_pos: 0,
            rx,
            focus: Pane::Input,

            outbox: Vec::new(),
            outbox_list_state: ListState::default().with_selected(Some(0)),

            inbox: BTreeMap::new(),
            inbox_list_state: ListState::default().with_selected(Some(0)),

            storebox: BTreeMap::new(),
            storebox_list_state: TableState::default(),

            logs: Vec::new(),
            logs_list_state: ListState::default().with_selected(Some(0)),
        })
    }
    fn run(&mut self, terminal: &mut DefaultTerminal) -> Result<()> {
        while !self.exit {
            terminal.draw(|frame| self.draw(frame))?;
            self.handle_events()?;
        }

        Ok(())
    }

    fn draw(&mut self, frame: &mut Frame) {
        let left_width = 20;

        // Main body and text input on the bottom.
        let outer_layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints(&[
                Constraint::Fill(1),
                Constraint::Length(3),
                Constraint::Length(1),
            ])
            .split(frame.area());

        // Draw text input
        self.draw_input(frame, outer_layout[1]);

        // Split the main body into two columns.
        let column_layout = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(&[
                Constraint::Percentage(left_width),
                Constraint::Percentage(100 - left_width),
            ])
            .split(outer_layout[0]);

        // Split the left column into outbox and inbox
        let outbox_inbox_layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints(&[Constraint::Percentage(80), Constraint::Percentage(20)])
            .split(column_layout[0]);

        self.draw_outbox(frame, outbox_inbox_layout[1]);
        self.draw_inbox(frame, outbox_inbox_layout[0]);

        // Draw the storebox (What's in redis)
        self.draw_storebox(frame, column_layout[1]);

        self.draw_hotkeys(frame, outer_layout[2]);

        if self.focus == Pane::Logs {
            self.draw_logs_overlay(frame);
        }
    }

    fn draw_hotkeys(&self, frame: &mut Frame, rect: Rect) {
        let hints = " [Enter] Send  [Tab] Switch pane  [↑↓/Scroll] Navigate  [Ctrl+L] Logs [Ctrl+C] Copy memo to clipboard [Esc] Quit ";
        let bar = Paragraph::new(hints).style(Style::default().fg(Color::DarkGray));
        frame.render_widget(bar, rect);
    }

    fn draw_logs_overlay(&mut self, frame: &mut Frame) {
        let area = centered_rect(70, 60, frame.area());
        let items: Vec<ListItem> = self
            .logs
            .iter()
            .map(|s| {
                let text = s
                    .into_text()
                    .unwrap_or_else(|_| ratatui::text::Text::raw(s.as_str()));
                ListItem::new(text)
            })
            .collect();
        let list = List::new(items).block(Block::bordered().title("Logs (ctrl+l to close)"));
        frame.render_widget(Clear, area);
        frame.render_stateful_widget(list, area, &mut self.logs_list_state);
    }

    fn border_style(&self, pane: Pane) -> Style {
        let mut style = Style::new();
        if self.focus == pane {
            style = style.blue();
        }
        style
    }

    fn draw_input(&mut self, frame: &mut Frame, rect: Rect) {
        let input = Paragraph::new(self.pending_memo.as_str())
            .style(Style::default())
            .block(
                Block::bordered()
                    .title("Input")
                    .style(self.border_style(Pane::Input)),
            );
        frame.render_widget(input, rect);
        // Place the terminal cursor inside the border (1 cell padding each side)
        frame.set_cursor_position((rect.x + 1 + self.cursor_pos as u16, rect.y + 1));
    }

    fn draw_outbox(&mut self, frame: &mut Frame, rect: Rect) {
        let items: Vec<_> = self.outbox.iter().map(|(.., memo)| memo.as_str()).collect();

        let list = List::new(items)
            .style(Color::White)
            .highlight_style(Modifier::REVERSED)
            .highlight_symbol("> ")
            .block(
                Block::bordered()
                    .title("Outbox (sending to chain)")
                    .style(self.border_style(Pane::Outbox)),
            );

        frame.render_stateful_widget(list, rect, &mut self.outbox_list_state);
    }

    fn draw_storebox(&mut self, frame: &mut Frame, rect: Rect) {
        let rows: Vec<Row> = self
            .storebox
            .iter()
            .map(|((_ts, sig), memo)| {
                Row::new(vec![Cell::new(sig.as_str()), Cell::new(memo.as_str())])
            })
            .collect();

        let widths = [Constraint::Length(50), Constraint::Fill(1)];

        let table = Table::new(rows, widths)
            .header(Row::new(vec![
                Cell::new("Address").style(Style::default().add_modifier(Modifier::BOLD)),
                Cell::new("Memo").style(Style::default().add_modifier(Modifier::BOLD)),
            ]))
            .style(Color::White)
            .row_highlight_style(Modifier::REVERSED)
            .highlight_symbol("> ")
            .block(
                Block::bordered()
                    .title("Storebox (saved in redis))")
                    .style(self.border_style(Pane::Stored)),
            );

        frame.render_stateful_widget(table, rect, &mut self.storebox_list_state);
    }

    fn draw_inbox(&mut self, frame: &mut Frame, rect: Rect) {
        let items: Vec<_> = self.inbox.iter().map(|(.., memo)| memo.as_str()).collect();

        let list = List::new(items)
            .style(Color::White)
            .highlight_style(Modifier::REVERSED)
            .highlight_symbol("> ")
            .block(
                Block::bordered()
                    .title("Inbox (streamed from chain)")
                    .style(self.border_style(Pane::Inbox)),
            );

        if self.inbox_list_state.selected().is_none() {
            self.inbox_list_state.select_first();
        }

        frame.render_stateful_widget(list, rect, &mut self.inbox_list_state);
    }

    /// Keyboard events and events (data) from the app channel
    fn handle_events(&mut self) -> Result<()> {
        for event in self.rx.try_iter() {
            match event {
                UiEvent::MemoInbox(memos) => {
                    for (sig, memo) in memos {
                        let key = (memo.count, sig);
                        if self.storebox.contains_key(&key) {
                            continue;
                        }
                        self.inbox.insert(key, memo.memo);
                    }
                }
                UiEvent::MemoPublished(published_id) => {
                    self.outbox.retain(|(id, ..)| *id != published_id);
                }
                UiEvent::Log(line) => {
                    self.logs.push(line);
                }
                UiEvent::Stored(memos) => {
                    for (sig, memo) in memos {
                        let k = (memo.count, sig);
                        self.inbox.remove(&k);
                        self.storebox.insert(k, memo.memo);
                    }

                    self.storebox_list_state.select_last();
                }
            }
        }

        if event::poll(Duration::from_millis(50))? {
            match event::read()? {
                // it's important to check that the event is a key press event as
                // crossterm also emits key release and repeat events on Windows.
                Event::Key(key_event) if key_event.kind == KeyEventKind::Press => {
                    self.handle_key_event(key_event)
                }
                Event::Mouse(mouse_event) => self.handle_mouse_event(mouse_event),
                _ => {}
            }
        }

        Ok(())
    }

    fn handle_mouse_event(&mut self, mouse_event: MouseEvent) {
        match mouse_event.kind {
            MouseEventKind::ScrollUp => match self.focus {
                Pane::Inbox => self.inbox_list_state.select_previous(),
                Pane::Outbox => self.outbox_list_state.select_previous(),
                Pane::Stored => self.storebox_list_state.select_previous(),
                Pane::Logs => self.logs_list_state.select_previous(),
                Pane::Input => {}
            },
            MouseEventKind::ScrollDown => match self.focus {
                Pane::Inbox => self.inbox_list_state.select_next(),
                Pane::Outbox => self.outbox_list_state.select_next(),
                Pane::Stored => self.storebox_list_state.select_next(),
                Pane::Logs => self.logs_list_state.select_next(),
                Pane::Input => {}
            },
            _ => {}
        }
    }

    fn handle_key_event(&mut self, key_event: KeyEvent) {
        match key_event.code {
            KeyCode::Esc => {
                self.exit = true;
            }
            KeyCode::Char('l') if key_event.modifiers.contains(KeyModifiers::CONTROL) => {
                if self.focus == Pane::Logs {
                    self.focus = Pane::Input;
                } else {
                    self.focus = Pane::Logs;
                }
            }
            KeyCode::Char('c') if key_event.modifiers.contains(KeyModifiers::CONTROL) => {
                if let Some(idx) = self.storebox_list_state.selected() {
                    if let Some(((.., pubkey), memo)) = self.storebox.iter().nth(idx) {
                        let pubkey = pubkey.clone();
                        if let Some(clipboard) = &mut self.clipboard {
                            tracing::info!("Copying memo to clipboard: {pubkey}");
                            let _ = clipboard.set_text(format!("Pubkey: {pubkey}\nMemo: {memo}"));
                        }
                    }
                }
            }
            KeyCode::Char(ch) => {
                self.pending_memo.insert(self.cursor_pos, ch);
                self.cursor_pos += 1;
            }
            KeyCode::Backspace => {
                if self.cursor_pos > 0 {
                    self.cursor_pos -= 1;
                    self.pending_memo.remove(self.cursor_pos);
                }
            }
            KeyCode::Left => {
                if self.cursor_pos > 0 {
                    self.cursor_pos -= 1;
                }
            }
            KeyCode::Right => {
                if self.cursor_pos < self.pending_memo.len() {
                    self.cursor_pos += 1;
                }
            }
            KeyCode::Home => {
                self.cursor_pos = 0;
            }
            KeyCode::End => {
                self.cursor_pos = self.pending_memo.len();
            }
            KeyCode::Enter => {
                let memo = self.pending_memo.clone();
                self.pending_memo.clear();
                self.cursor_pos = 0;
                let id = ID.fetch_add(1, Ordering::SeqCst);

                self.outbox.push((id, memo.clone()));
                self.ctx.runtime.spawn({
                    let ctx = self.ctx.clone();
                    async move {
                        if let Err(err) = publish_memo(ctx, id, memo).await {
                            tracing::error!("Error publishing memo {err:?}");
                        }
                    }
                });
            }
            KeyCode::Up => match self.focus {
                Pane::Inbox => self.inbox_list_state.select_previous(),
                Pane::Outbox => self.outbox_list_state.select_previous(),
                Pane::Stored => self.storebox_list_state.select_previous(),
                Pane::Logs => self.logs_list_state.select_previous(),
                Pane::Input => {}
            },
            KeyCode::Down => match self.focus {
                Pane::Inbox => self.inbox_list_state.select_next(),
                Pane::Outbox => self.outbox_list_state.select_next(),
                Pane::Stored => self.storebox_list_state.select_next(),
                Pane::Logs => self.logs_list_state.select_next(),
                Pane::Input => {}
            },
            KeyCode::Tab => {
                self.focus = match self.focus {
                    Pane::Input => Pane::Outbox,
                    Pane::Outbox => Pane::Inbox,
                    Pane::Inbox => Pane::Stored,
                    Pane::Stored => Pane::Input,
                    Pane::Logs => Pane::Logs,
                };
            }
            _ => {}
        }
    }
}

fn main() -> Result<()> {
    let _ = dotenv();

    let mut app = App::new()?;

    tracing_subscriber::fmt()
        .with_writer(ChannelMakeWriter {
            tx: app.ctx.tx.clone(),
        })
        .init();

    app.ctx
        .runtime
        .spawn(chain::task_stream_chain(app.ctx.clone()));

    let mut terminal = ratatui::init();
    crossterm::execute!(std::io::stdout(), crossterm::event::EnableMouseCapture)?;
    let result = app.run(&mut terminal);
    crossterm::execute!(std::io::stdout(), crossterm::event::DisableMouseCapture)?;
    ratatui::restore();
    result
}
