# Ratatype

A fast, minimalist typing test application for the terminal.

## Features

- **30-second typing tests** (customizable duration)
- **Real-time WPM tracking** with performance graphs
- **Dictionary words** or built-in sample texts
- **Error correction mode** for accuracy training
- **Visual feedback** with color-coded characters
- **Test history** automatically saved to CSV

## Installation

```bash
git clone https://github.com/your-username/ratatype.git
cd ratatype
cargo build --release
```

## Usage

```bash
# Basic 30-second test
cargo run

# 60-second test
cargo run -- -d 60

# Error correction mode (must fix mistakes)
cargo run -- -c

# Short words only (max 5 characters)
cargo run -- -m 5

# Use built-in texts instead of dictionary words
cargo run -- -b

# Combine options
cargo run -- -d 120 -c -m 4
```

## Command Line Options

- `-d, --duration <SECONDS>` - Test duration (default: 30)
- `-c, --require-correction` - Must correct errors before proceeding
- `-b, --use-builtin-texts` - Use sample texts instead of dictionary words
- `-m, --max-word-length <LENGTH>` - Maximum word length for dictionary mode (default: 7)

## Color Coding

- **Green**: Correctly typed characters
- **Orange**: Corrected characters (had errors but fixed)
- **Red**: Wrong characters (normal mode only)
- **White**: Current cursor position
- **Gray**: Untyped characters

## History

Test results are automatically saved to `~/.ratatype_history.csv` with:
- Timestamp, duration, WPM stats, accuracy, errors
- Test settings (correction mode, text source, etc.)

## Controls

- **Type** to take the test
- **Backspace** to correct mistakes
- **ESC** to quit
- **Enter** to restart after test completion

## Requirements

- Rust 1.70+
- Terminal with color support
- `/usr/share/dict/words` file (for dictionary mode)
