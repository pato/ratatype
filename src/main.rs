use clap::Parser;
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
    widgets::{Axis, Block, Borders, Chart, Dataset, GraphType, Paragraph, Table, Row, Cell},
    Frame, Terminal,
};
use rand::Rng;
use std::{
    error::Error,
    fs,
    io,
    time::{Duration, Instant},
};

#[derive(Parser)]
#[command(name = "ratatype")]
#[command(about = "A TUI-based typing test application")]
#[command(version)]
struct Args {
    /// Duration of the typing test in seconds
    #[arg(short, long, default_value_t = 30)]
    duration: u64,
    
    /// Require errors to be corrected before proceeding
    #[arg(short = 'c', long, default_value_t = false)]
    require_correction: bool,
    
    /// Use built-in sample texts instead of random dictionary words
    #[arg(short = 'b', long, default_value_t = false)]
    use_builtin_texts: bool,
}

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
    total_keystrokes: usize,
    last_wpm_update: Option<Instant>,
    require_correction: bool,
    correction_attempts: Vec<bool>, // Track which positions had errors
    use_builtin_texts: bool,
    sample_texts: Vec<String>,
}

impl App {
    fn new(duration_secs: u64, require_correction: bool, use_builtin_texts: bool) -> App {
        let sample_texts = vec![
            "The quick brown fox jumps over the lazy dog. This pangram contains every letter of the alphabet at least once.".to_string(),
            "In a hole in the ground there lived a hobbit. Not a nasty, dirty, wet hole filled with the ends of worms and an oozy smell.".to_string(),
            "To be or not to be, that is the question. Whether 'tis nobler in the mind to suffer the slings and arrows of outrageous fortune.".to_string(),
            "It was the best of times, it was the worst of times, it was the age of wisdom, it was the age of foolishness and doubt.".to_string(),
            "All human beings are born free and equal in dignity and rights. They are endowed with reason and conscience.".to_string(),
            "The only way to do great work is to love what you do. If you haven't found it yet, keep looking and don't settle.".to_string(),
            "Two things are infinite: the universe and human stupidity; and I'm not sure about the universe and its vast mysteries.".to_string(),
            "In the midst of winter, I found there was, within me, an invincible summer that could not be defeated by any force.".to_string(),
        ];
        
        let mut app = App {
            target_text: String::new(),
            user_input: String::new(),
            current_position: 0,
            start_time: None,
            wpm_history: Vec::new(),
            wpm_data_points: Vec::new(),
            test_duration: Duration::from_secs(duration_secs),
            is_finished: false,
            errors: 0,
            total_keystrokes: 0,
            last_wpm_update: None,
            require_correction,
            correction_attempts: Vec::new(),
            use_builtin_texts,
            sample_texts,
        };
        
        app.generate_text();
        app
    }
    
    fn generate_text(&mut self) {
        let text = if self.use_builtin_texts {
            self.generate_builtin_text()
        } else {
            self.generate_dictionary_text()
        };
        
        self.target_text = text;
        // Initialize correction_attempts vector with false for each character
        self.correction_attempts = vec![false; self.target_text.chars().count()];
    }
    
    fn generate_builtin_text(&self) -> String {
        let mut rng = rand::thread_rng();
        let mut text = String::new();
        
        // Generate enough text for fast typers (aim for ~500 characters minimum)
        while text.len() < 500 {
            let sample = &self.sample_texts[rng.gen_range(0..self.sample_texts.len())];
            if !text.is_empty() {
                text.push(' ');
            }
            text.push_str(sample);
        }
        
        text
    }
    
    fn generate_dictionary_text(&self) -> String {
        match self.load_dictionary_words() {
            Ok(words) => {
                if words.is_empty() {
                    return self.generate_builtin_text(); // Fallback
                }
                
                let mut rng = rand::thread_rng();
                let mut text = String::new();
                
                // Generate enough words for ~500 characters
                while text.len() < 500 {
                    let word = &words[rng.gen_range(0..words.len())];
                    if !text.is_empty() {
                        text.push(' ');
                    }
                    text.push_str(word);
                }
                
                text
            }
            Err(_) => {
                // Fallback to built-in texts if dictionary not available
                self.generate_builtin_text()
            }
        }
    }
    
    fn load_dictionary_words(&self) -> Result<Vec<String>, Box<dyn Error>> {
        let dict_content = fs::read_to_string("/usr/share/dict/words")?;
        let words: Vec<String> = dict_content
            .lines()
            .filter(|line| {
                let word = line.trim();
                // Filter for reasonable words: 3-12 characters, only letters, no proper nouns
                word.len() >= 3 
                    && word.len() <= 12 
                    && word.chars().all(|c| c.is_ascii_lowercase())
            })
            .map(|s| s.trim().to_string())
            .collect();
        
        Ok(words)
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
                    
                    if self.require_correction {
                        // In correction mode, only accept the correct character
                        if c == target_char {
                            self.user_input.push(c);
                            self.total_keystrokes += 1;
                            self.current_position += 1;
                            self.update_wpm();
                        } else {
                            // Wrong character - mark this position as needing correction
                            self.errors += 1;
                            self.total_keystrokes += 1;
                            if self.current_position < self.correction_attempts.len() {
                                self.correction_attempts[self.current_position] = true;
                            }
                        }
                    } else {
                        // In normal mode, allow proceeding with errors
                        self.user_input.push(c);
                        self.total_keystrokes += 1;
                        
                        if c == target_char {
                            self.current_position += 1;
                            self.update_wpm(); // Only update WPM on correct characters
                        } else {
                            self.errors += 1;
                            // Mark this position as having had an error
                            if self.current_position < self.correction_attempts.len() {
                                self.correction_attempts[self.current_position] = true;
                            }
                            self.current_position += 1; // Move forward even with errors
                        }
                    }
                    
                    if self.current_position >= self.target_text.len() {
                        self.is_finished = true;
                    }
                }
            }
            KeyCode::Backspace => {
                if !self.user_input.is_empty() {
                    self.user_input.pop();
                    self.total_keystrokes += 1;
                    if self.current_position > 0 {
                        self.current_position -= 1;
                    }
                }
            }
            _ => {}
        }
    }

    fn update_wpm(&mut self) {
        if let Some(start) = self.start_time {
            let now = Instant::now();
            let elapsed_seconds = start.elapsed().as_secs_f64();
            
            // Only update WPM if at least 1 second has passed since last update
            // and at least 2 seconds have passed since start (to avoid huge initial values)
            let should_update = if let Some(last_update) = self.last_wpm_update {
                now.duration_since(last_update).as_secs_f64() >= 1.0
            } else {
                elapsed_seconds >= 2.0  // Wait 2 seconds before first WPM calculation
            };
            
            if should_update && elapsed_seconds >= 2.0 {
                let elapsed_minutes = elapsed_seconds / 60.0;
                let words_typed = self.current_position as f64 / 5.0; // Standard: 5 characters = 1 word
                let wpm = words_typed / elapsed_minutes;
                
                // Cap the WPM at reasonable maximum (500 WPM is extremely fast)
                let capped_wpm = wpm.min(500.0);
                
                self.wpm_history.push(capped_wpm);
                self.wpm_data_points.push((elapsed_seconds, capped_wpm));
                self.last_wpm_update = Some(now);
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
        if self.total_keystrokes == 0 {
            100.0
        } else {
            let correct_keystrokes = self.total_keystrokes - self.errors;
            (correct_keystrokes as f64 / self.total_keystrokes as f64) * 100.0
        }
    }

    fn get_elapsed_time(&self) -> Duration {
        self.start_time.map_or(Duration::ZERO, |start| start.elapsed())
    }
    
    fn restart(&mut self) {
        self.user_input.clear();
        self.current_position = 0;
        self.start_time = None;
        self.wpm_history.clear();
        self.wpm_data_points.clear();
        self.is_finished = false;
        self.errors = 0;
        self.total_keystrokes = 0;
        self.last_wpm_update = None;
        self.correction_attempts.clear();
        self.generate_text();
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let args = Args::parse();
    
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new(args.duration, args.require_correction, args.use_builtin_texts);
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
        // Main typing test loop
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

            // Check if time is up even without keystroke
            if let Some(start) = app.start_time {
                if start.elapsed() >= app.test_duration {
                    app.is_finished = true;
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
                    if key.kind == KeyEventKind::Press {
                        match key.code {
                            KeyCode::Esc => return Ok(()),
                            KeyCode::Enter => {
                                app.restart();
                                break; // Return to main typing loop
                            }
                            _ => {} // Ignore other keys to prevent accidental dismissal
                        }
                    }
                }
            }
        }
    }
}

fn ui(f: &mut Frame, app: &App) {
    if app.is_finished {
        render_summary_screen(f, app);
    } else {
        render_typing_screen(f, app);
    }
}

fn render_typing_screen(f: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(ratatui::layout::Direction::Vertical)
        .constraints([
            Constraint::Length(1),  // Timer
            Constraint::Length(1),  // Spacer
            Constraint::Min(5),     // Text area (minimalist)
            Constraint::Length(1),  // Spacer  
            Constraint::Length(1),  // Simple stats
        ])
        .split(f.area());

    // Simple timer display
    let elapsed = app.get_elapsed_time();
    let remaining = if elapsed < app.test_duration {
        app.test_duration - elapsed
    } else {
        Duration::ZERO
    };
    
    let timer_text = format!("{:.0}s", remaining.as_secs_f64());
    let timer = Paragraph::new(timer_text)
        .style(Style::default().fg(Color::Yellow))
        .alignment(ratatui::layout::Alignment::Center);
    f.render_widget(timer, chunks[0]);

    // Text display - clean and minimal
    let mut spans = Vec::new();
    let chars: Vec<char> = app.target_text.chars().collect();
    let user_chars: Vec<char> = app.user_input.chars().collect();

    // Show text from beginning with fixed positioning - no scrolling
    let visible_chars = 300; // Show more characters
    let end_pos = visible_chars.min(chars.len());

    for i in 0..end_pos {
        let target_char = chars[i];
        let style = if i < user_chars.len() {
            // Character has been typed - compare what was typed vs what should be typed
            let typed_char = user_chars[i];
            if typed_char == target_char {
                // Correct character was typed
                if i < app.correction_attempts.len() && app.correction_attempts[i] {
                    // Correct but required correction attempts
                    Style::default().fg(Color::Rgb(255, 165, 0)) // Orange
                } else {
                    // Correct on first try
                    Style::default().fg(Color::Green)
                }
            } else {
                // Wrong character was typed (only possible in normal mode)
                Style::default().fg(Color::Red)
            }
        } else if i == app.current_position {
            // Current cursor position
            Style::default().fg(Color::Black).bg(Color::White)
        } else {
            // Untyped characters
            Style::default().fg(Color::DarkGray)
        };
        
        spans.push(Span::styled(target_char.to_string(), style));
    }

    let text_paragraph = Paragraph::new(Line::from(spans))
        .wrap(ratatui::widgets::Wrap { trim: true })
        .alignment(ratatui::layout::Alignment::Left);
    f.render_widget(text_paragraph, chunks[2]);

    // Simple stats line
    let stats_text = format!("WPM: {:.0} | Accuracy: {:.0}%", app.get_current_wpm(), app.get_accuracy());
    let stats = Paragraph::new(stats_text)
        .style(Style::default().fg(Color::Cyan))
        .alignment(ratatui::layout::Alignment::Center);
    f.render_widget(stats, chunks[4]);
}

fn render_summary_screen(f: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(ratatui::layout::Direction::Vertical)
        .constraints([
            Constraint::Length(3),  // Title
            Constraint::Length(8),  // Stats table
            Constraint::Min(8),     // WPM Graph
            Constraint::Length(2),  // Instructions
        ])
        .split(f.area());

    // Title
    let title = Paragraph::new("Test Complete!")
        .style(Style::default().fg(Color::Green))
        .alignment(ratatui::layout::Alignment::Center)
        .block(Block::default().borders(Borders::ALL));
    f.render_widget(title, chunks[0]);

    // Stats Table
    let rows = vec![
        Row::new(vec![
            Cell::from("Average WPM"),
            Cell::from(format!("{:.1}", app.get_average_wpm())),
        ]),
        Row::new(vec![
            Cell::from("Peak WPM"),
            Cell::from(format!("{:.1}", app.wpm_history.iter().fold(0.0f64, |acc, &x| acc.max(x)))),
        ]),
        Row::new(vec![
            Cell::from("Accuracy"),
            Cell::from(format!("{:.1}%", app.get_accuracy())),
        ]),
        Row::new(vec![
            Cell::from("Characters Typed"),
            Cell::from(format!("{}", app.current_position)),
        ]),
        Row::new(vec![
            Cell::from("Errors"),
            Cell::from(format!("{}", app.errors)),
        ]),
        Row::new(vec![
            Cell::from("Test Duration"),
            Cell::from(format!("{:.0}s", app.test_duration.as_secs())),
        ]),
    ];

    let table = Table::new(rows, [Constraint::Percentage(50), Constraint::Percentage(50)])
        .block(Block::default().borders(Borders::ALL).title("Results"))
        .style(Style::default().fg(Color::White));
    f.render_widget(table, chunks[1]);

    // WPM Graph
    if !app.wpm_data_points.is_empty() {
        let max_wpm = app.wpm_data_points.iter()
            .map(|(_, wpm)| *wpm)
            .fold(0.0, f64::max)
            .max(60.0);
        
        let test_duration_secs = app.test_duration.as_secs_f64();
        
        let dataset = Dataset::default()
            .name("WPM")
            .marker(ratatui::symbols::Marker::Braille)
            .graph_type(GraphType::Line)
            .style(Style::default().fg(Color::Cyan))
            .data(&app.wpm_data_points);

        let chart = Chart::new(vec![dataset])
            .block(Block::default().borders(Borders::ALL).title("WPM Performance"))
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
    }

    // Instructions
    let instructions = Paragraph::new("Press ESC to exit or ENTER to restart")
        .style(Style::default().fg(Color::Yellow))
        .alignment(ratatui::layout::Alignment::Center);
    f.render_widget(instructions, chunks[3]);
}
