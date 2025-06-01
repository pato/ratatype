use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::{Backend, CrosstermBackend},
    layout::{Constraint, Layout},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Axis, Block, Borders, Chart, Dataset, GraphType, Paragraph},
    Frame, Terminal,
};
use rand::Rng;
use std::{
    error::Error,
    io,
    time::{Duration, Instant},
};

struct App {
    target_text: String,
    user_input: String,
    current_position: usize,
    start_time: Option<Instant>,
    wpm_history: Vec<f64>,
    wpm_data_points: Vec<(f64, f64)>, // (time, wpm) for graphing
    test_duration: Duration,
    is_finished: bool,
    errors: usize,
}

impl App {
    fn new() -> App {
        let sample_texts = vec![
            "The quick brown fox jumps over the lazy dog. This pangram contains every letter of the alphabet at least once.",
            "In a hole in the ground there lived a hobbit. Not a nasty, dirty, wet hole filled with the ends of worms.",
            "To be or not to be, that is the question. Whether 'tis nobler in the mind to suffer the slings and arrows.",
            "It was the best of times, it was the worst of times, it was the age of wisdom, it was the age of foolishness.",
        ];
        
        let mut rng = rand::thread_rng();
        let target_text = sample_texts[rng.gen_range(0..sample_texts.len())].to_string();
        
        App {
            target_text,
            user_input: String::new(),
            current_position: 0,
            start_time: None,
            wpm_history: Vec::new(),
            wpm_data_points: Vec::new(),
            test_duration: Duration::from_secs(30),
            is_finished: false,
            errors: 0,
        }
    }

    fn handle_key_event(&mut self, key: KeyCode) {
        if self.is_finished {
            return;
        }

        if self.start_time.is_none() {
            self.start_time = Some(Instant::now());
        }

        match key {
            KeyCode::Char(c) => {
                if self.current_position < self.target_text.len() {
                    let target_char = self.target_text.chars().nth(self.current_position).unwrap();
                    self.user_input.push(c);
                    
                    if c == target_char {
                        self.current_position += 1;
                    } else {
                        self.errors += 1;
                    }
                    
                    self.update_wpm();
                    
                    if self.current_position >= self.target_text.len() {
                        self.is_finished = true;
                    }
                }
            }
            KeyCode::Backspace => {
                if !self.user_input.is_empty() {
                    self.user_input.pop();
                    if self.current_position > 0 {
                        self.current_position -= 1;
                    }
                }
            }
            _ => {}
        }

        if let Some(start) = self.start_time {
            if start.elapsed() >= self.test_duration {
                self.is_finished = true;
            }
        }
    }

    fn update_wpm(&mut self) {
        if let Some(start) = self.start_time {
            let elapsed_seconds = start.elapsed().as_secs_f64();
            let elapsed_minutes = elapsed_seconds / 60.0;
            if elapsed_minutes > 0.0 {
                let words_typed = self.current_position as f64 / 5.0; // Standard: 5 characters = 1 word
                let wpm = words_typed / elapsed_minutes;
                self.wpm_history.push(wpm);
                self.wpm_data_points.push((elapsed_seconds, wpm));
            }
        }
    }

    fn get_current_wpm(&self) -> f64 {
        self.wpm_history.last().copied().unwrap_or(0.0)
    }

    fn get_average_wpm(&self) -> f64 {
        if self.wpm_history.is_empty() {
            0.0
        } else {
            self.wpm_history.iter().sum::<f64>() / self.wpm_history.len() as f64
        }
    }

    fn get_accuracy(&self) -> f64 {
        if self.current_position == 0 {
            100.0
        } else {
            ((self.current_position - self.errors) as f64 / self.current_position as f64) * 100.0
        }
    }

    fn get_elapsed_time(&self) -> Duration {
        self.start_time.map_or(Duration::ZERO, |start| start.elapsed())
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new();
    let res = run_app(&mut terminal, &mut app);

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    if let Err(err) = res {
        println!("{err:?}");
    }

    Ok(())
}

fn run_app<B: Backend>(terminal: &mut Terminal<B>, app: &mut App) -> io::Result<()> {
    loop {
        terminal.draw(|f| ui(f, app))?;

        if event::poll(Duration::from_millis(50))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    match key.code {
                        KeyCode::Esc => return Ok(()),
                        _ => app.handle_key_event(key.code),
                    }
                }
            }
        }

        if app.is_finished {
            break;
        }
    }
    
    // Show final results
    loop {
        terminal.draw(|f| ui(f, app))?;
        
        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press && key.code == KeyCode::Esc {
                    return Ok(());
                }
            }
        }
    }
}

fn ui(f: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(ratatui::layout::Direction::Vertical)
        .constraints([
            Constraint::Length(3),  // Stats
            Constraint::Min(10),    // Text area
            Constraint::Length(8),  // WPM Graph
            Constraint::Length(3),  // Instructions
        ])
        .split(f.area());

    // Stats bar
    let elapsed = app.get_elapsed_time();
    let remaining = if elapsed < app.test_duration {
        app.test_duration - elapsed
    } else {
        Duration::ZERO
    };
    
    let stats_text = if app.is_finished {
        format!(
            "Test Complete! WPM: {:.1} | Accuracy: {:.1}% | Press ESC to exit",
            app.get_average_wpm(),
            app.get_accuracy()
        )
    } else {
        format!(
            "Time: {:.1}s | WPM: {:.1} | Accuracy: {:.1}%",
            remaining.as_secs_f64(),
            app.get_current_wpm(),
            app.get_accuracy()
        )
    };

    let stats = Paragraph::new(stats_text)
        .block(Block::default().borders(Borders::ALL).title("Stats"));
    f.render_widget(stats, chunks[0]);

    // Text display
    let mut spans = Vec::new();
    let chars: Vec<char> = app.target_text.chars().collect();
    let user_chars: Vec<char> = app.user_input.chars().collect();

    for (i, &target_char) in chars.iter().enumerate() {
        let style = if i < user_chars.len() {
            // Typed character
            if user_chars[i] == target_char {
                Style::default().fg(Color::Green)
            } else {
                Style::default().fg(Color::Red)
            }
        } else if i == app.current_position {
            // Current cursor position
            Style::default().fg(Color::Yellow).bg(Color::DarkGray)
        } else {
            // Untyped characters
            Style::default().fg(Color::Gray)
        };
        
        spans.push(Span::styled(target_char.to_string(), style));
    }

    let text_paragraph = Paragraph::new(Line::from(spans))
        .block(Block::default().borders(Borders::ALL).title("Type the text below"))
        .wrap(ratatui::widgets::Wrap { trim: true });
    f.render_widget(text_paragraph, chunks[1]);

    // WPM Graph
    if !app.wpm_data_points.is_empty() {
        let max_wpm = app.wpm_data_points.iter()
            .map(|(_, wpm)| *wpm)
            .fold(0.0, f64::max)
            .max(60.0); // Minimum scale of 60 WPM
        
        let test_duration_secs = app.test_duration.as_secs_f64();
        
        let dataset = Dataset::default()
            .name("WPM")
            .marker(ratatui::symbols::Marker::Braille)
            .graph_type(GraphType::Line)
            .style(Style::default().fg(Color::Cyan))
            .data(&app.wpm_data_points);

        let chart = Chart::new(vec![dataset])
            .block(Block::default().borders(Borders::ALL).title("WPM Over Time"))
            .x_axis(
                Axis::default()
                    .title("Time (s)")
                    .style(Style::default().fg(Color::Gray))
                    .bounds([0.0, test_duration_secs])
                    .labels(vec![Line::from("0"), Line::from(format!("{:.0}", test_duration_secs / 2.0)), Line::from(format!("{:.0}", test_duration_secs))]),
            )
            .y_axis(
                Axis::default()
                    .title("WPM")
                    .style(Style::default().fg(Color::Gray))
                    .bounds([0.0, max_wpm])
                    .labels(vec![Line::from("0"), Line::from(format!("{:.0}", max_wpm / 2.0)), Line::from(format!("{:.0}", max_wpm))]),
            );
        
        f.render_widget(chart, chunks[2]);
    } else {
        let placeholder = Paragraph::new("WPM graph will appear here once you start typing...")
            .block(Block::default().borders(Borders::ALL).title("WPM Over Time"))
            .style(Style::default().fg(Color::Gray));
        f.render_widget(placeholder, chunks[2]);
    }

    // Instructions
    let instructions = if app.is_finished {
        "Test complete! Press ESC to exit"
    } else {
        "Type the text above. Press ESC to quit."
    };
    
    let instruction_paragraph = Paragraph::new(instructions)
        .block(Block::default().borders(Borders::ALL).title("Instructions"));
    f.render_widget(instruction_paragraph, chunks[3]);
}
