# Ratatype

A fast, minimalist typing test application for the terminal.

## Developed using AI 🤖

This was developed using Claude Code (with Sonnet 4) for a grand total of
$8.80.

Every commit has alongside with it the prompt I used that generated the
contents of the commit (with the exception of commits marked as no ai, but
there was no code that wasn't written by the model).

```
> /cost
  ⎿  Total cost:            $8.80
     Total duration (API):  37m 40.6s
     Total duration (wall): 7h 48m 27.5s
     Total code changes:    928 lines added, 166 lines removed
     Token usage by model:
         claude-3-5-haiku:  261.2k input, 6.7k output, 0 cache read, 0 cache write
            claude-sonnet:  335 input, 55.1k output, 14.9m cache read, 867.6k cache write
```


## Features

- **30-second typing tests** (customizable duration)
- **Real-time WPM tracking** with performance graphs
- **Multiple text sources**: Google top 10k words (default), system dictionary, or built-in sample texts
- **Error correction mode** for accuracy training
- **Visual feedback** with color-coded characters
- **Test history** automatically saved to CSV

## Installation

```bash
git clone https://github.com/pato/ratatype.git
cd ratatype
cargo build --release
cargo install --path .
```

## Usage

```bash
# Basic 30-second test
ratatype

# 60-second test
ratatype -d 60

# Error correction mode (must fix mistakes)
ratatype -c

# Short words only (max 5 characters)
ratatype -m 5

# Use system dictionary words instead of google top 10k
ratatype -s system

# Use built-in sample texts
ratatype -s builtin

# Combine options
ratatype -d 120 -c -s system -m 4
```

## Command Line Options

- `-d, --duration <SECONDS>` - Test duration (default: 30)
- `-c, --require-correction` - Must correct errors before proceeding
- `-s, --text-source <SOURCE>` - Text source: google (top 10k words, default), system (/usr/share/dict/words), builtin (sample texts)
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
