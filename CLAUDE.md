# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Ratatype is a TUI-based typing game built with Rust and ratatui. It provides 30-second typing tests with real-time WPM tracking, visual feedback, and performance graphs.

## Development Commands

- **Build**: `cargo build`
- **Run**: `cargo run`
- **Test**: `cargo test`
- **Check (fast compile check)**: `cargo check`
- **Format code**: `cargo fmt`
- **Lint**: `cargo clippy`

## Architecture

### Core Components

- **App struct**: Main application state containing target text, user input, timing, and metrics
- **TUI Layout**: Four-panel interface with stats, text display, WPM graph, and instructions
- **Input Handling**: Real-time keyboard input processing with character-by-character validation
- **Performance Tracking**: WPM calculation based on 5-character word standard with accuracy metrics

### Key Features

- Random text selection from predefined samples
- Color-coded character display (gray=untyped, green=correct, red=error, yellow=cursor)
- Real-time WPM graphing with time-series data points
- 30-second timer with automatic test completion
- Final statistics display with average WPM and accuracy percentage

### Dependencies

- `ratatui`: TUI framework for terminal interface
- `crossterm`: Cross-platform terminal manipulation
- `rand`: Random text selection

The application uses a single-file architecture with clear separation between data structures, event handling, and UI rendering.