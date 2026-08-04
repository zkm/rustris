# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

`rustris` — a terminal Tetris clone in Rust. Binary/crate name matches the repo: `rustris` (package name in `Cargo.toml`). Rendering and input are done entirely through `crossterm` (no GPU/display server needed); piece randomization uses `rand`.

## Commands

```sh
cargo build                                          # debug build
cargo build --release                                # release build
cargo run --release                                   # play the game
cargo test                                             # run tests
cargo clippy --all-targets --all-features -- -D warnings   # lint (matches CI, must be warning-free)
cargo fmt --all                                        # format
cargo fmt --all -- --check                             # format check (matches CI)
```

CI (`.github/workflows/ci.yml`) runs `cargo fmt --check`, clippy with `-D warnings`, build, and test on every push/PR to `main`. Keep the tree clippy- and fmt-clean before committing — CI will fail otherwise.

Releases (`.github/workflows/release.yml`) trigger on tags matching `v*.*.*` pushed to origin: `git tag -a vX.Y.Z -m "..." && git push origin vX.Y.Z`. This cross-compiles release binaries for Linux (`x86_64-unknown-linux-gnu`), macOS (`x86_64-apple-darwin`, `aarch64-apple-darwin`), and Windows (`x86_64-pc-windows-msvc`), archives them (`tar.gz` on Unix, `zip` on Windows, named `rustris-<target>.<ext>`), and attaches them to an auto-generated GitHub Release.

## Architecture

Everything lives in a single file, `src/main.rs` (~570 lines). It has no submodules; read it top to bottom rather than searching for other source files.

- **Piece shapes (`SHAPES` const)**: each of the 7 tetrominoes is a `[u16; 4]` — one 16-bit mask per rotation state, each mask describing a 4x4 grid (bit 15 = row0/col0 ... bit 0 = row3/col3). The `cells()` function decodes a mask into `(row, col)` offsets. This compact representation is the key thing to understand before touching rotation/collision logic.
- **`Bag`**: implements 7-bag randomization (each of the 7 pieces appears once per shuffled bag before repeating).
- **`Piece`**: a live tetromino's kind, rotation index, and board position (`x`, `y`); `y` can be negative while spawning above the visible board.
- **`Game`**: owns the board (`Vec<Vec<Option<usize>>>`, `None` = empty cell, `Some(kind)` = locked piece color index), the current/next piece, score/lines/level, and pause/game-over flags. Core methods:
  - `fits()` — collision/bounds check against the board for a given kind/rotation/position; all movement and rotation logic routes through this.
  - `try_move`, `try_rotate` (rotation includes a small wall-kick offset table: `[0, -1, 1, -2, 2]`), `hard_drop`, `lock_piece`, `clear_lines`.
  - `drop_interval()` computes gravity speed from `level` (starts at 1000ms, decreases 75ms/level, floors at 100ms); `level` increases every 10 cleared lines.
  - `ghost_y()` computes the hard-drop landing row for the ghost-piece preview.
- **Rendering (`draw()`)**: redraws the whole frame each loop iteration directly via `crossterm::queue!` (border, ghost piece, locked board, active piece, side panel with next-piece preview/score/controls) — there's no diffing/dirty-rect optimization. `draw_game_over()` renders a bottom-up "fill" sweep animation over the board on loss, timed off `game_over_at` (an `Instant` set in `end_game()`) at `GAME_OVER_ROW_MS` per row.
- **`main()`**: sets up raw mode + alternate screen, then runs a loop that polls input (16ms timeout) via `crossterm::event`, applies input to `Game`, advances gravity when `drop_interval()` has elapsed since the last tick, and redraws. Terminal state is always restored (raw mode disabled, alternate screen left) after the loop, including on error, via the `result` closure pattern.

A `#[cfg(test)] mod tests` block at the bottom of `src/main.rs` covers game-over edge cases (e.g. locking a piece that's still above the visible board, topping out a column). When making changes, keep in mind there are no module boundaries — new logic typically extends `Game` methods and the single `SHAPES`/`cells()` piece representation rather than introducing new files, unless the change is substantial enough to warrant splitting the file up.
