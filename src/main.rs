use clap::Parser;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use rand::Rng;
use ratatui::{
    Frame, Terminal,
    backend::{Backend, CrosstermBackend},
    layout::{Constraint, Layout},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Axis, Block, Borders, Cell, Chart, Dataset, GraphType, Paragraph, Row, Table},
};
use std::{
    collections::HashMap,
    env,
    error::Error,
    fs::{self, OpenOptions},
    io::{self, Write},
    path::PathBuf,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

// Application constants
const MIN_TEXT_LENGTH: usize = 500;
const WPM_UPDATE_INTERVAL_SECS: f64 = 1.0;
const INITIAL_WPM_DELAY_SECS: f64 = 2.0;
const CHARS_PER_WORD: f64 = 5.0;
const MAX_WPM_CAP: f64 = 500.0;
const POLL_INTERVAL_MS: u64 = 50;
const RENDER_INTERVAL_MS: u64 = 100;
const VISIBLE_CHAR_LIMIT: usize = 300;
const MIN_WORD_LENGTH: usize = 3;
const HISTORY_FILENAME: &str = ".ratatype_history.csv";
const DICT_PATH: &str = "/usr/share/dict/words";

// Embedded word list
const GOOGLE_10000_WORDS: &str = include_str!("../data/google-10000.txt");

#[derive(Debug, Clone, Copy, PartialEq)]
enum TextSource {
    Google10k,
    SystemDict,
    Builtin,
}

impl std::str::FromStr for TextSource {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "google" | "google10k" | "top10k" => Ok(TextSource::Google10k),
            "system" | "dict" | "dictionary" => Ok(TextSource::SystemDict),
            "builtin" | "built-in" | "samples" => Ok(TextSource::Builtin),
            _ => Err(format!(
                "Invalid text source '{}'. Valid options: google, system, builtin",
                s
            )),
        }
    }
}

impl std::fmt::Display for TextSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TextSource::Google10k => write!(f, "google"),
            TextSource::SystemDict => write!(f, "system"),
            TextSource::Builtin => write!(f, "builtin"),
        }
    }
}

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

    /// Text source for typing test
    #[arg(
        short = 's',
        long,
        default_value = "google",
        help = "Text source: google (top 10k words), system (/usr/share/dict/words), builtin (sample texts)"
    )]
    text_source: TextSource,

    /// Maximum word length when using dictionary words
    #[arg(short = 'm', long, default_value_t = 7, value_parser = validate_word_length)]
    max_word_length: usize,
}

fn validate_word_length(s: &str) -> Result<usize, String> {
    let value: usize = s.parse().map_err(|_| "Must be a positive integer")?;
    if value < MIN_WORD_LENGTH {
        Err(format!("Word length must be at least {}", MIN_WORD_LENGTH))
    } else if value > 20 {
        Err("Word length must be 20 or less".to_string())
    } else {
        Ok(value)
    }
}

#[derive(Debug)]
struct TestHistory {
    timestamp: u64,
    duration_seconds: u64,
    avg_wpm: f64,
    peak_wpm: f64,
    accuracy: f64,
    characters_typed: usize,
    errors: usize,
    correction_mode: bool,
    text_source: String,
    max_word_length: usize,
}

#[derive(Debug, Clone)]
struct KeyMetrics {
    times: Vec<Duration>,
    errors: usize,
}

impl KeyMetrics {
    fn new() -> Self {
        Self {
            times: Vec::new(),
            errors: 0,
        }
    }

    fn average_time(&self) -> Option<Duration> {
        if self.times.is_empty() {
            None
        } else {
            let total_nanos: u64 = self.times.iter().map(|d| d.as_nanos() as u64).sum();
            Some(Duration::from_nanos(total_nanos / self.times.len() as u64))
        }
    }
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
    text_source: TextSource,
    max_word_length: usize,
    sample_texts: Vec<String>,
    // Cache for performance
    target_chars: Vec<char>,
    // Key analytics tracking
    key_metrics: HashMap<char, KeyMetrics>,
    last_keystroke_time: Option<Instant>,
    current_key_start_time: Option<Instant>,
}

impl App {
    fn new(
        duration_secs: u64,
        require_correction: bool,
        text_source: TextSource,
        max_word_length: usize,
    ) -> App {
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
            text_source,
            max_word_length,
            sample_texts,
            target_chars: Vec::new(),
            key_metrics: HashMap::new(),
            last_keystroke_time: None,
            current_key_start_time: None,
        };

        app.generate_text();
        app.start_timing_current_key();
        app
    }

    fn start_timing_current_key(&mut self) {
        if self.current_position < self.target_chars.len() {
            self.current_key_start_time = Some(Instant::now());
        }
    }

    fn generate_text(&mut self) {
        let text = match self.text_source {
            TextSource::Google10k => self.generate_google10k_text(),
            TextSource::SystemDict => self.generate_system_dict_text(),
            TextSource::Builtin => self.generate_builtin_text(),
        };

        self.target_text = text;
        // Cache character vector for performance and initialize correction_attempts
        self.target_chars = self.target_text.chars().collect();
        self.correction_attempts = vec![false; self.target_chars.len()];
    }

    fn generate_builtin_text(&self) -> String {
        let mut rng = rand::thread_rng();
        let mut text = String::new();

        // Generate enough text for fast typers
        while text.len() < MIN_TEXT_LENGTH {
            let sample = &self.sample_texts[rng.gen_range(0..self.sample_texts.len())];
            if !text.is_empty() {
                text.push(' ');
            }
            text.push_str(sample);
        }

        text
    }

    fn generate_google10k_text(&self) -> String {
        let words = self.load_google10k_words();
        self.generate_word_text(&words)
    }

    fn generate_system_dict_text(&self) -> String {
        match self.load_system_dict_words() {
            Ok(words) => {
                if words.is_empty() {
                    return self.generate_builtin_text(); // Fallback
                }
                self.generate_word_text(&words)
            }
            Err(e) => {
                // Log warning and fallback to built-in texts if dictionary not available
                eprintln!(
                    "Warning: Could not load dictionary from {}: {}. Using built-in texts.",
                    DICT_PATH, e
                );
                self.generate_builtin_text()
            }
        }
    }

    fn generate_word_text(&self, words: &[String]) -> String {
        let mut rng = rand::thread_rng();
        let mut text = String::new();

        // Generate enough words
        while text.len() < MIN_TEXT_LENGTH {
            let word = &words[rng.gen_range(0..words.len())];
            if !text.is_empty() {
                text.push(' ');
            }
            text.push_str(word);
        }

        text
    }

    fn load_google10k_words(&self) -> Vec<String> {
        GOOGLE_10000_WORDS
            .lines()
            .filter(|line| {
                let word = line.trim();
                // Filter for reasonable words: MIN_WORD_LENGTH to max_word_length characters, only letters
                word.len() >= MIN_WORD_LENGTH
                    && word.len() <= self.max_word_length
                    && word.chars().all(|c| c.is_ascii_lowercase())
            })
            .map(|s| s.trim().to_string())
            .collect()
    }

    fn load_system_dict_words(&self) -> Result<Vec<String>, Box<dyn Error>> {
        let dict_content = fs::read_to_string(DICT_PATH)?;
        let words: Vec<String> = dict_content
            .lines()
            .filter(|line| {
                let word = line.trim();
                // Filter for reasonable words: MIN_WORD_LENGTH to max_word_length characters, only letters, no proper nouns
                word.len() >= MIN_WORD_LENGTH
                    && word.len() <= self.max_word_length
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
            self.last_keystroke_time = Some(Instant::now());
            self.start_timing_current_key();
        }

        let now = Instant::now();

        match key {
            KeyCode::Char(c) => {
                if self.current_position < self.target_chars.len() {
                    let target_char = self.target_chars[self.current_position];

                    // Record timing data only when we get the target character (correct or as an attempt)
                    if let Some(key_start_time) = self.current_key_start_time {
                        let key_response_time = now.duration_since(key_start_time);
                        // Always record timing for target character attempts
                        self.key_metrics
                            .entry(target_char)
                            .or_insert_with(KeyMetrics::new)
                            .times
                            .push(key_response_time);
                    }

                    if self.require_correction {
                        // In correction mode, only accept the correct character
                        if c == target_char {
                            self.user_input.push(c);
                            self.total_keystrokes += 1;
                            self.current_position += 1;
                            self.start_timing_current_key(); // Start timing next key
                            self.update_wpm();
                        } else {
                            // Wrong character - mark this position as needing correction and track error
                            self.errors += 1;
                            self.total_keystrokes += 1;
                            self.key_metrics
                                .entry(target_char)
                                .or_insert_with(KeyMetrics::new)
                                .errors += 1;
                            if self.current_position < self.correction_attempts.len() {
                                self.correction_attempts[self.current_position] = true;
                            }
                            // Don't start timing next key yet - stay on current key until correct
                        }
                    } else {
                        // In normal mode, allow proceeding with errors
                        self.user_input.push(c);
                        self.total_keystrokes += 1;

                        if c == target_char {
                            self.current_position += 1;
                            self.start_timing_current_key(); // Start timing next key
                            self.update_wpm(); // Only update WPM on correct characters
                        } else {
                            self.errors += 1;
                            self.key_metrics
                                .entry(target_char)
                                .or_insert_with(KeyMetrics::new)
                                .errors += 1;
                            // Mark this position as having had an error
                            if self.current_position < self.correction_attempts.len() {
                                self.correction_attempts[self.current_position] = true;
                            }
                            self.current_position += 1; // Move forward even with errors
                            self.start_timing_current_key(); // Start timing next key
                        }
                    }

                    self.last_keystroke_time = Some(now);

                    if self.current_position >= self.target_chars.len() {
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
                        self.start_timing_current_key(); // Start timing the key we're now on
                    }
                }
                self.last_keystroke_time = Some(now);
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
                now.duration_since(last_update).as_secs_f64() >= WPM_UPDATE_INTERVAL_SECS
            } else {
                elapsed_seconds >= INITIAL_WPM_DELAY_SECS
            };

            if should_update && elapsed_seconds >= INITIAL_WPM_DELAY_SECS {
                let elapsed_minutes = elapsed_seconds / 60.0;
                let words_typed = self.current_position as f64 / CHARS_PER_WORD;
                let wpm = words_typed / elapsed_minutes;

                // Cap the WPM at reasonable maximum
                let capped_wpm = wpm.min(MAX_WPM_CAP);

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
        self.start_time
            .map_or(Duration::ZERO, |start| start.elapsed())
    }

    fn save_history(&self) -> Result<(), Box<dyn Error>> {
        let history_record = TestHistory {
            timestamp: SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs(),
            duration_seconds: self.test_duration.as_secs(),
            avg_wpm: self.get_average_wpm(),
            peak_wpm: self.wpm_history.iter().fold(0.0f64, |acc, &x| acc.max(x)),
            accuracy: self.get_accuracy(),
            characters_typed: self.current_position,
            errors: self.errors,
            correction_mode: self.require_correction,
            text_source: self.text_source.to_string(),
            max_word_length: self.max_word_length,
        };

        let history_path = self.get_history_file_path()?;

        // Check if file exists to determine if we need to write header
        let file_exists = history_path.exists();

        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&history_path)?;

        // Write CSV header if file is new
        if !file_exists {
            writeln!(
                file,
                "timestamp,duration_seconds,avg_wpm,peak_wpm,accuracy,characters_typed,errors,correction_mode,text_source,max_word_length"
            )?;
        }

        // Write the record
        writeln!(
            file,
            "{},{},{:.2},{:.2},{:.2},{},{},{},{},{}",
            history_record.timestamp,
            history_record.duration_seconds,
            history_record.avg_wpm,
            history_record.peak_wpm,
            history_record.accuracy,
            history_record.characters_typed,
            history_record.errors,
            history_record.correction_mode,
            history_record.text_source,
            history_record.max_word_length
        )?;

        Ok(())
    }

    fn get_history_file_path(&self) -> Result<PathBuf, Box<dyn Error>> {
        let mut path = if let Ok(home) = env::var("HOME") {
            PathBuf::from(home)
        } else {
            env::current_dir()?
        };

        path.push(HISTORY_FILENAME);
        Ok(path)
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
        self.target_chars.clear();
        self.key_metrics.clear();
        self.last_keystroke_time = None;
        self.current_key_start_time = None;
        self.generate_text();
        self.start_timing_current_key();
    }

    fn get_fastest_keys(&self, count: usize) -> Vec<(char, Duration)> {
        let mut key_times: Vec<(char, Duration)> = self
            .key_metrics
            .iter()
            .filter_map(|(key, metrics)| metrics.average_time().map(|avg_time| (*key, avg_time)))
            .collect();

        key_times.sort_by_key(|(_, time)| *time);
        key_times.into_iter().take(count).collect()
    }

    fn get_slowest_keys(&self, count: usize) -> Vec<(char, Duration)> {
        let mut key_times: Vec<(char, Duration)> = self
            .key_metrics
            .iter()
            .filter_map(|(key, metrics)| metrics.average_time().map(|avg_time| (*key, avg_time)))
            .collect();

        key_times.sort_by_key(|(_, time)| std::cmp::Reverse(*time));
        key_times.into_iter().take(count).collect()
    }

    fn get_most_error_prone_keys(&self, count: usize) -> Vec<(char, usize)> {
        let mut key_errors: Vec<(char, usize)> = self
            .key_metrics
            .iter()
            .filter(|(_, metrics)| metrics.errors > 0)
            .map(|(key, metrics)| (*key, metrics.errors))
            .collect();

        key_errors.sort_by_key(|(_, errors)| std::cmp::Reverse(*errors));
        key_errors.into_iter().take(count).collect()
    }

    fn get_most_accurate_keys(&self, count: usize) -> Vec<(char, f64)> {
        let mut key_accuracy: Vec<(char, f64)> = self
            .key_metrics
            .iter()
            .filter_map(|(key, metrics)| {
                if !metrics.times.is_empty() {
                    let total_attempts = metrics.times.len();
                    let accuracy =
                        (total_attempts - metrics.errors) as f64 / total_attempts as f64 * 100.0;
                    Some((*key, accuracy))
                } else {
                    None
                }
            })
            .collect();

        key_accuracy
            .sort_by(|(_, a), (_, b)| b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal));
        key_accuracy.into_iter().take(count).collect()
    }

    fn get_key_speed_color(&self, key: char) -> Color {
        if let Some(metrics) = self.key_metrics.get(&key) {
            if let Some(avg_time) = metrics.average_time() {
                // Calculate all average times to determine relative performance
                let all_times: Vec<Duration> = self
                    .key_metrics
                    .values()
                    .filter_map(|m| m.average_time())
                    .collect();

                if all_times.len() < 2 {
                    return Color::Gray; // Not enough data
                }

                let min_time = all_times.iter().min().unwrap();
                let max_time = all_times.iter().max().unwrap();
                let time_range = max_time.as_millis() - min_time.as_millis();

                if time_range == 0 {
                    return Color::Gray; // All times are the same
                }

                // Calculate relative position (0.0 = fastest, 1.0 = slowest)
                let relative_pos =
                    (avg_time.as_millis() - min_time.as_millis()) as f64 / time_range as f64;

                // Map to colors: green for fast, red for slow
                if relative_pos < 0.33 {
                    // Fast keys (green shades)
                    if relative_pos < 0.16 {
                        Color::Green // Fastest
                    } else {
                        Color::Rgb(144, 238, 144) // Light green
                    }
                } else if relative_pos < 0.67 {
                    // Medium keys (yellow/white)
                    Color::Yellow
                } else {
                    // Slow keys (red shades)
                    if relative_pos > 0.83 {
                        Color::Red // Slowest
                    } else {
                        Color::Rgb(255, 99, 71) // Light red
                    }
                }
            } else {
                Color::Gray // No timing data
            }
        } else {
            Color::DarkGray // Key not used
        }
    }

    fn get_key_accuracy_color(&self, key: char) -> Color {
        if let Some(metrics) = self.key_metrics.get(&key) {
            if !metrics.times.is_empty() {
                let total_attempts = metrics.times.len();
                let accuracy = (total_attempts - metrics.errors) as f64 / total_attempts as f64;

                // Map accuracy to colors: green for high accuracy, red for low accuracy
                if accuracy >= 0.95 {
                    Color::Green // 95%+ accuracy
                } else if accuracy >= 0.85 {
                    Color::Rgb(144, 238, 144) // Light green (85-94%)
                } else if accuracy >= 0.70 {
                    Color::Yellow // Medium accuracy (70-84%)
                } else if accuracy >= 0.50 {
                    Color::Rgb(255, 99, 71) // Light red (50-69%)
                } else {
                    Color::Red // Low accuracy (<50%)
                }
            } else {
                Color::Gray // No data
            }
        } else {
            Color::DarkGray // Key not used
        }
    }

    fn render_speed_keyboard(&self) -> Vec<Line> {
        // QWERTY layout with proper spacing and indentation
        let keyboard_rows = vec![
            ("qwertyuiop", "  "), // (keys, indent)
            ("asdfghjkl", "   "), // home row more indented
            ("zxcvbnm", "     "), // bottom row most indented
        ];

        let mut lines = Vec::new();

        for (row, indent) in keyboard_rows {
            let mut spans = Vec::new();

            // Add indentation
            spans.push(Span::styled(indent, Style::default()));

            for ch in row.chars() {
                let color = self.get_key_speed_color(ch);
                // Create key with background color and small spacing
                spans.push(Span::styled(
                    format!(" {} ", ch),
                    Style::default().fg(Color::Black).bg(color),
                ));
                spans.push(Span::styled(" ", Style::default())); // Small space between keys
            }

            lines.push(Line::from(spans));
        }

        lines
    }

    fn render_accuracy_keyboard(&self) -> Vec<Line> {
        // QWERTY layout with proper spacing and indentation
        let keyboard_rows = vec![
            ("qwertyuiop", "  "), // (keys, indent)
            ("asdfghjkl", "   "), // home row more indented
            ("zxcvbnm", "     "), // bottom row most indented
        ];

        let mut lines = Vec::new();

        for (row, indent) in keyboard_rows {
            let mut spans = Vec::new();

            // Add indentation
            spans.push(Span::styled(indent, Style::default()));

            for ch in row.chars() {
                let color = self.get_key_accuracy_color(ch);
                // Create key with background color and small spacing
                spans.push(Span::styled(
                    format!(" {} ", ch),
                    Style::default().fg(Color::Black).bg(color),
                ));
                spans.push(Span::styled(" ", Style::default())); // Small space between keys
            }

            lines.push(Line::from(spans));
        }

        lines
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let args = Args::parse();

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new(
        args.duration,
        args.require_correction,
        args.text_source,
        args.max_word_length,
    );
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

            if event::poll(Duration::from_millis(POLL_INTERVAL_MS))? {
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
                // Save test history
                if let Err(e) = app.save_history() {
                    eprintln!("Warning: Failed to save test history: {}", e);
                }
                break;
            }
        }

        // Show final results
        loop {
            terminal.draw(|f| ui(f, app))?;

            if event::poll(Duration::from_millis(RENDER_INTERVAL_MS))? {
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
            Constraint::Length(1), // Timer
            Constraint::Length(1), // Spacer
            Constraint::Min(5),    // Text area (minimalist)
            Constraint::Length(1), // Spacer
            Constraint::Length(1), // Simple stats
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
    let chars = &app.target_chars;
    let user_chars: Vec<char> = app.user_input.chars().collect();

    // Show text from beginning with fixed positioning - no scrolling
    let visible_chars = VISIBLE_CHAR_LIMIT;
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
    let stats_text = format!(
        "WPM: {:.0} | Accuracy: {:.0}%",
        app.get_current_wpm(),
        app.get_accuracy()
    );
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
            Constraint::Length(18), // Key analytics (compact keyboard heatmaps)
            Constraint::Min(6),     // WPM Graph
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
            Cell::from(format!(
                "{:.1}",
                app.wpm_history.iter().fold(0.0f64, |acc, &x| acc.max(x))
            )),
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

    let table = Table::new(
        rows,
        [Constraint::Percentage(50), Constraint::Percentage(50)],
    )
    .block(Block::default().borders(Borders::ALL).title("Results"))
    .style(Style::default().fg(Color::White));
    f.render_widget(table, chunks[1]);

    // Key Analytics Section
    let key_analytics_chunks = Layout::default()
        .direction(ratatui::layout::Direction::Horizontal)
        .constraints([
            Constraint::Percentage(50), // Fastest/Slowest keys
            Constraint::Percentage(50), // Most/Least error-prone keys
        ])
        .split(chunks[2]);

    // Fastest and Slowest Keys
    let fastest_keys = app.get_fastest_keys(3);
    let slowest_keys = app.get_slowest_keys(3);

    let mut speed_rows = vec![Row::new(vec![
        Cell::from("Fastest Keys"),
        Cell::from("Time (ms)"),
    ])];
    if fastest_keys.is_empty() {
        speed_rows.push(Row::new(vec![Cell::from("No data"), Cell::from("-")]));
    } else {
        for (key, time) in fastest_keys {
            speed_rows.push(Row::new(vec![
                Cell::from(format!("'{}'", key)),
                Cell::from(format!("{}", time.as_millis())),
            ]));
        }
    }
    speed_rows.push(Row::new(vec![Cell::from(""), Cell::from("")])); // Spacer
    speed_rows.push(Row::new(vec![
        Cell::from("Slowest Keys"),
        Cell::from("Time (ms)"),
    ]));
    if slowest_keys.is_empty() {
        speed_rows.push(Row::new(vec![Cell::from("No data"), Cell::from("-")]));
    } else {
        for (key, time) in slowest_keys {
            speed_rows.push(Row::new(vec![
                Cell::from(format!("'{}'", key)),
                Cell::from(format!("{}", time.as_millis())),
            ]));
        }
    }

    // Add speed heatmap to the table
    speed_rows.push(Row::new(vec![Cell::from(""), Cell::from("")])); // Spacer
    speed_rows.push(Row::new(vec![Cell::from("Speed Heatmap:"), Cell::from("")]));

    let speed_keyboard_lines = app.render_speed_keyboard();
    for line in speed_keyboard_lines {
        speed_rows.push(Row::new(vec![Cell::from(line), Cell::from("")]));
    }

    let speed_table = Table::new(
        speed_rows,
        [Constraint::Percentage(60), Constraint::Percentage(40)],
    )
    .block(Block::default().borders(Borders::ALL).title("Key Speed"))
    .style(Style::default().fg(Color::White));
    f.render_widget(speed_table, key_analytics_chunks[0]);

    // Most Error-Prone and Most Accurate Keys
    let error_prone_keys = app.get_most_error_prone_keys(3);
    let accurate_keys = app.get_most_accurate_keys(3);

    let mut accuracy_rows = vec![Row::new(vec![
        Cell::from("Problem Keys"),
        Cell::from("Errors"),
    ])];
    for (key, errors) in error_prone_keys {
        accuracy_rows.push(Row::new(vec![
            Cell::from(format!("'{}'", key)),
            Cell::from(format!("{}", errors)),
        ]));
    }
    accuracy_rows.push(Row::new(vec![Cell::from(""), Cell::from("")])); // Spacer
    accuracy_rows.push(Row::new(vec![
        Cell::from("Best Keys"),
        Cell::from("Accuracy"),
    ]));
    for (key, accuracy) in accurate_keys {
        accuracy_rows.push(Row::new(vec![
            Cell::from(format!("'{}'", key)),
            Cell::from(format!("{:.0}%", accuracy)),
        ]));
    }

    // Add accuracy heatmap to the table
    accuracy_rows.push(Row::new(vec![Cell::from(""), Cell::from("")])); // Spacer
    accuracy_rows.push(Row::new(vec![
        Cell::from("Accuracy Heatmap:"),
        Cell::from(""),
    ]));

    let accuracy_keyboard_lines = app.render_accuracy_keyboard();
    for line in accuracy_keyboard_lines {
        accuracy_rows.push(Row::new(vec![Cell::from(line), Cell::from("")]));
    }

    let accuracy_table = Table::new(
        accuracy_rows,
        [Constraint::Percentage(60), Constraint::Percentage(40)],
    )
    .block(Block::default().borders(Borders::ALL).title("Key Accuracy"))
    .style(Style::default().fg(Color::White));
    f.render_widget(accuracy_table, key_analytics_chunks[1]);

    // WPM Graph
    if !app.wpm_data_points.is_empty() {
        let max_wpm = app
            .wpm_data_points
            .iter()
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
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("WPM Performance"),
            )
            .x_axis(
                Axis::default()
                    .title("Time (s)")
                    .style(Style::default().fg(Color::Gray))
                    .bounds([0.0, test_duration_secs])
                    .labels(vec![
                        Line::from("0"),
                        Line::from(format!("{:.0}", test_duration_secs / 2.0)),
                        Line::from(format!("{:.0}", test_duration_secs)),
                    ]),
            )
            .y_axis(
                Axis::default()
                    .title("WPM")
                    .style(Style::default().fg(Color::Gray))
                    .bounds([0.0, max_wpm])
                    .labels(vec![
                        Line::from("0"),
                        Line::from(format!("{:.0}", max_wpm / 2.0)),
                        Line::from(format!("{:.0}", max_wpm)),
                    ]),
            );

        f.render_widget(chart, chunks[3]);
    }

    // Instructions
    let instructions = Paragraph::new("Press ESC to exit or ENTER to restart")
        .style(Style::default().fg(Color::Yellow))
        .alignment(ratatui::layout::Alignment::Center);
    f.render_widget(instructions, chunks[4]);
}
