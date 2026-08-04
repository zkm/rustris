use std::io::{Write, stdout};
use std::time::{Duration, Instant};

use crossterm::{
    cursor,
    event::{self, Event, KeyCode, KeyEventKind},
    execute, queue,
    style::{Color, Print, ResetColor, SetBackgroundColor, SetForegroundColor},
    terminal::{self, ClearType},
};
use rand::seq::SliceRandom;

const BOARD_W: usize = 10;
const BOARD_H: usize = 20;

// Each tetromino has 4 rotation states, each packed into a 16-bit mask
// describing a 4x4 grid (bit 15 = row0/col0 ... bit 0 = row3/col3).
const SHAPES: [[u16; 4]; 7] = [
    [0x0F00, 0x2222, 0x00F0, 0x4444], // I
    [0x0660, 0x0660, 0x0660, 0x0660], // O
    [0x0E40, 0x4C40, 0x4E00, 0x4640], // T
    [0x06C0, 0x8C40, 0x6C00, 0x4620], // S
    [0x0C60, 0x4C80, 0xC600, 0x2640], // Z
    [0x44C0, 0x8E00, 0x6440, 0x0E20], // J
    [0x4460, 0x0E80, 0xC440, 0x2E00], // L
];

const COLORS: [Color; 7] = [
    Color::Cyan,
    Color::Yellow,
    Color::Magenta,
    Color::Green,
    Color::Red,
    Color::Blue,
    Color::DarkYellow,
];

fn cells(piece: usize, rot: usize) -> Vec<(i32, i32)> {
    let mask = SHAPES[piece][rot];
    let mut out = Vec::with_capacity(4);
    for row in 0..4 {
        for col in 0..4 {
            let bit = 15 - (row * 4 + col);
            if (mask >> bit) & 1 == 1 {
                out.push((row, col));
            }
        }
    }
    out
}

struct Bag {
    queue: Vec<usize>,
}

impl Bag {
    fn new() -> Self {
        Bag { queue: Vec::new() }
    }

    fn next(&mut self) -> usize {
        if self.queue.is_empty() {
            let mut bag: Vec<usize> = (0..7).collect();
            bag.shuffle(&mut rand::rng());
            self.queue = bag;
        }
        self.queue.pop().unwrap()
    }
}

struct Piece {
    kind: usize,
    rot: usize,
    x: i32,
    y: i32,
}

impl Piece {
    fn spawn(kind: usize) -> Self {
        Piece {
            kind,
            rot: 0,
            x: 3,
            y: -1,
        }
    }

    fn cells(&self) -> Vec<(i32, i32)> {
        cells(self.kind, self.rot)
    }
}

struct Game {
    board: Vec<Vec<Option<usize>>>,
    bag: Bag,
    current: Piece,
    next_kind: usize,
    score: u32,
    lines: u32,
    level: u32,
    game_over: bool,
    game_over_at: Option<Instant>,
    paused: bool,
}

impl Game {
    fn new() -> Self {
        let mut bag = Bag::new();
        let first = bag.next();
        let next_kind = bag.next();
        Game {
            board: vec![vec![None; BOARD_W]; BOARD_H],
            bag,
            current: Piece::spawn(first),
            next_kind,
            score: 0,
            lines: 0,
            level: 1,
            game_over: false,
            game_over_at: None,
            paused: false,
        }
    }

    fn fits(&self, kind: usize, rot: usize, x: i32, y: i32) -> bool {
        for (dr, dc) in cells(kind, rot) {
            let px = x + dc;
            let py = y + dr;
            if px < 0 || px >= BOARD_W as i32 || py >= BOARD_H as i32 {
                return false;
            }
            if py >= 0 && self.board[py as usize][px as usize].is_some() {
                return false;
            }
        }
        true
    }

    fn spawn_next(&mut self) {
        let kind = self.next_kind;
        self.next_kind = self.bag.next();
        self.current = Piece::spawn(kind);
        if !self.fits(kind, 0, self.current.x, self.current.y) {
            self.end_game();
        }
    }

    fn end_game(&mut self) {
        self.game_over = true;
        self.game_over_at = Some(Instant::now());
    }

    fn try_move(&mut self, dx: i32, dy: i32) -> bool {
        let (nx, ny) = (self.current.x + dx, self.current.y + dy);
        if self.fits(self.current.kind, self.current.rot, nx, ny) {
            self.current.x = nx;
            self.current.y = ny;
            true
        } else {
            false
        }
    }

    fn try_rotate(&mut self) {
        let new_rot = (self.current.rot + 1) % 4;
        for kick in [0, -1, 1, -2, 2] {
            if self.fits(
                self.current.kind,
                new_rot,
                self.current.x + kick,
                self.current.y,
            ) {
                self.current.rot = new_rot;
                self.current.x += kick;
                return;
            }
        }
    }

    fn hard_drop(&mut self) {
        let mut dist = 0;
        while self.fits(
            self.current.kind,
            self.current.rot,
            self.current.x,
            self.current.y + dist + 1,
        ) {
            dist += 1;
        }
        self.current.y += dist;
        self.score += (dist as u32) * 2;
        self.lock_piece();
    }

    fn lock_piece(&mut self) {
        let mut blocked_out = false;
        for (dr, dc) in self.current.cells() {
            let px = self.current.x + dc;
            let py = self.current.y + dr;
            if py < 0 {
                blocked_out = true;
                continue;
            }
            if py < BOARD_H as i32 && px >= 0 && px < BOARD_W as i32 {
                self.board[py as usize][px as usize] = Some(self.current.kind);
            }
        }
        if blocked_out {
            self.end_game();
            return;
        }
        self.clear_lines();
        self.spawn_next();
    }

    fn clear_lines(&mut self) {
        let mut cleared = 0;
        let mut new_board: Vec<Vec<Option<usize>>> = Vec::with_capacity(BOARD_H);
        for row in self.board.iter() {
            if row.iter().all(|c| c.is_some()) {
                cleared += 1;
            } else {
                new_board.push(row.clone());
            }
        }
        for _ in 0..cleared {
            new_board.insert(0, vec![None; BOARD_W]);
        }
        self.board = new_board;

        if cleared > 0 {
            self.lines += cleared as u32;
            let points = match cleared {
                1 => 40,
                2 => 100,
                3 => 300,
                4 => 1200,
                _ => 0,
            };
            self.score += points * self.level;
            self.level = 1 + self.lines / 10;
        }
    }

    fn drop_interval(&self) -> Duration {
        let ms = 1000i64 - (self.level as i64 - 1) * 75;
        Duration::from_millis(ms.max(100) as u64)
    }

    fn tick(&mut self) {
        if self.paused || self.game_over {
            return;
        }
        if !self.try_move(0, 1) {
            self.lock_piece();
        }
    }

    fn ghost_y(&self) -> i32 {
        let mut y = self.current.y;
        while self.fits(self.current.kind, self.current.rot, self.current.x, y + 1) {
            y += 1;
        }
        y
    }
}

// Milliseconds between each row of the bottom-up "fill" sweep shown on loss.
const GAME_OVER_ROW_MS: u128 = 25;

fn draw_game_over(out: &mut std::io::Stdout, game: &Game, ox: u16, oy: u16) -> std::io::Result<()> {
    let elapsed_ms = game
        .game_over_at
        .map(|t| t.elapsed().as_millis())
        .unwrap_or(0);
    let rows_filled = ((elapsed_ms / GAME_OVER_ROW_MS) as usize).min(BOARD_H);

    for i in 0..rows_filled {
        let r = BOARD_H - 1 - i;
        let sy = oy + 1 + r as u16;
        queue!(
            out,
            cursor::MoveTo(ox + 1, sy),
            SetBackgroundColor(Color::Grey),
            Print(" ".repeat(BOARD_W * 2)),
            ResetColor
        )?;
    }

    if rows_filled == BOARD_H {
        let label = " GAME OVER ";
        let board_w = (BOARD_W * 2) as u16;
        let label_x = ox + 1 + board_w.saturating_sub(label.len() as u16) / 2;
        let label_y = oy + 1 + BOARD_H as u16 / 2;
        queue!(
            out,
            cursor::MoveTo(label_x, label_y),
            SetForegroundColor(Color::White),
            SetBackgroundColor(Color::Red),
            Print(label),
            ResetColor
        )?;
    }

    Ok(())
}

fn draw(game: &Game) -> std::io::Result<()> {
    let mut out = stdout();
    queue!(out, terminal::Clear(ClearType::All), cursor::MoveTo(0, 0))?;

    let ox: u16 = 2; // board origin (column)
    let oy: u16 = 1; // board origin (row)

    // border
    queue!(
        out,
        cursor::MoveTo(ox, oy),
        Print("+"),
        Print("-".repeat(BOARD_W * 2)),
        Print("+")
    )?;
    for r in 0..BOARD_H {
        queue!(out, cursor::MoveTo(ox, oy + 1 + r as u16), Print("|"))?;
        queue!(
            out,
            cursor::MoveTo(ox + 1 + (BOARD_W * 2) as u16, oy + 1 + r as u16),
            Print("|")
        )?;
    }
    queue!(
        out,
        cursor::MoveTo(ox, oy + 1 + BOARD_H as u16),
        Print("+"),
        Print("-".repeat(BOARD_W * 2)),
        Print("+")
    )?;

    if !game.game_over {
        // ghost piece
        let ghost_y = game.ghost_y();
        for (dr, dc) in game.current.cells() {
            let px = game.current.x + dc;
            let py = ghost_y + dr;
            if py >= 0 && (py as usize) < BOARD_H {
                let sx = ox + 1 + (px as u16) * 2;
                let sy = oy + 1 + py as u16;
                queue!(
                    out,
                    cursor::MoveTo(sx, sy),
                    SetForegroundColor(Color::DarkGrey),
                    Print("::"),
                    ResetColor
                )?;
            }
        }
    }

    // settled board
    for r in 0..BOARD_H {
        for c in 0..BOARD_W {
            if let Some(kind) = game.board[r][c] {
                let sx = ox + 1 + (c as u16) * 2;
                let sy = oy + 1 + r as u16;
                queue!(
                    out,
                    cursor::MoveTo(sx, sy),
                    SetBackgroundColor(COLORS[kind]),
                    Print("  "),
                    ResetColor
                )?;
            }
        }
    }

    if !game.game_over {
        // active piece
        for (dr, dc) in game.current.cells() {
            let px = game.current.x + dc;
            let py = game.current.y + dr;
            if py >= 0 && (py as usize) < BOARD_H {
                let sx = ox + 1 + (px as u16) * 2;
                let sy = oy + 1 + py as u16;
                queue!(
                    out,
                    cursor::MoveTo(sx, sy),
                    SetBackgroundColor(COLORS[game.current.kind]),
                    Print("  "),
                    ResetColor
                )?;
            }
        }
    }

    if game.game_over {
        draw_game_over(&mut out, game, ox, oy)?;
    }

    // side panel
    let panel_x = ox + (BOARD_W as u16) * 2 + 4;
    queue!(out, cursor::MoveTo(panel_x, oy), Print("NEXT"))?;
    for (dr, dc) in cells(game.next_kind, 0) {
        let sx = panel_x + (dc as u16) * 2;
        let sy = oy + 2 + dr as u16;
        queue!(
            out,
            cursor::MoveTo(sx, sy),
            SetBackgroundColor(COLORS[game.next_kind]),
            Print("  "),
            ResetColor
        )?;
    }

    queue!(
        out,
        cursor::MoveTo(panel_x, oy + 8),
        Print(format!("Score: {}", game.score))
    )?;
    queue!(
        out,
        cursor::MoveTo(panel_x, oy + 9),
        Print(format!("Lines: {}", game.lines))
    )?;
    queue!(
        out,
        cursor::MoveTo(panel_x, oy + 10),
        Print(format!("Level: {}", game.level))
    )?;

    queue!(out, cursor::MoveTo(panel_x, oy + 12), Print("Controls:"))?;
    queue!(out, cursor::MoveTo(panel_x, oy + 13), Print("<- ->  move"))?;
    queue!(
        out,
        cursor::MoveTo(panel_x, oy + 14),
        Print("v      soft drop")
    )?;
    queue!(
        out,
        cursor::MoveTo(panel_x, oy + 15),
        Print("space  hard drop")
    )?;
    queue!(
        out,
        cursor::MoveTo(panel_x, oy + 16),
        Print("up/x   rotate")
    )?;
    queue!(out, cursor::MoveTo(panel_x, oy + 17), Print("p      pause"))?;
    queue!(out, cursor::MoveTo(panel_x, oy + 18), Print("q      quit"))?;

    if game.paused {
        queue!(out, cursor::MoveTo(panel_x, oy + 20), Print("-- PAUSED --"))?;
    }
    if game.game_over {
        queue!(
            out,
            cursor::MoveTo(panel_x, oy + 20),
            Print("-- GAME OVER --")
        )?;
        queue!(
            out,
            cursor::MoveTo(panel_x, oy + 21),
            Print("press q to quit")
        )?;
    }

    queue!(out, cursor::MoveTo(0, oy + BOARD_H as u16 + 3))?;
    out.flush()?;
    Ok(())
}

fn main() -> std::io::Result<()> {
    let mut out = stdout();
    terminal::enable_raw_mode()?;
    execute!(out, terminal::EnterAlternateScreen, cursor::Hide)?;

    let mut game = Game::new();
    let mut last_tick = Instant::now();

    let result = (|| -> std::io::Result<()> {
        loop {
            let timeout = Duration::from_millis(16);
            if event::poll(timeout)?
                && let Event::Key(key) = event::read()?
                && key.kind == KeyEventKind::Press
            {
                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => break,
                    KeyCode::Char('p') => {
                        if !game.game_over {
                            game.paused = !game.paused;
                        }
                    }
                    _ if game.paused || game.game_over => {}
                    KeyCode::Left => {
                        game.try_move(-1, 0);
                    }
                    KeyCode::Right => {
                        game.try_move(1, 0);
                    }
                    KeyCode::Down => {
                        if game.try_move(0, 1) {
                            game.score += 1;
                        }
                    }
                    KeyCode::Up | KeyCode::Char('x') | KeyCode::Char('X') => {
                        game.try_rotate();
                    }
                    KeyCode::Char(' ') => {
                        game.hard_drop();
                    }
                    _ => {}
                }
            }

            if last_tick.elapsed() >= game.drop_interval() {
                game.tick();
                last_tick = Instant::now();
            }

            draw(&game)?;
        }
        Ok(())
    })();

    execute!(out, terminal::LeaveAlternateScreen, cursor::Show)?;
    terminal::disable_raw_mode()?;

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    // L (and J) at rot 0 occupy row 0 of their 4x4 mask, which maps to a
    // cell above the visible board (py = -1) right at spawn. That cell was
    // previously invisible to `fits()`'s spawn check, so if the very next
    // tick found the piece already blocked from descending, `lock_piece`
    // silently dropped the off-board cell instead of ending the game.
    #[test]
    fn locking_above_the_board_ends_the_game() {
        let mut game = Game::new();
        game.current = Piece::spawn(6); // L piece
        game.board[0][4] = Some(0); // blocks the piece from descending at all
        game.tick();
        assert!(game.game_over);
    }

    #[test]
    fn stacking_a_column_to_the_top_ends_the_game() {
        let mut game = Game::new();
        for _ in 0..15 {
            if game.game_over {
                return;
            }
            game.hard_drop();
        }
        panic!("expected top-out after repeatedly dropping into the spawn column");
    }
}
