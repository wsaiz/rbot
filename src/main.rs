use std::collections::HashMap;
use std::env;
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;

use serde::{Deserialize, Serialize};
use rand::seq::SliceRandom;
use rand::thread_rng;

const BOARD_SIZE: usize = 31;
const CENTER: usize = 15;

#[derive(Deserialize, Clone, Debug)]
#[serde(untagged)]
enum StrOrUsize {
    Str(String),
    Num(usize),
}
impl StrOrUsize {
    fn as_usize(&self) -> usize {
        match self {
            StrOrUsize::Str(s) => s.parse::<usize>().unwrap(),
            StrOrUsize::Num(n) => *n,
        }
    }
}

#[derive(Deserialize, Clone, Debug)]
struct CoordIn {
    x: StrOrUsize,
    y: StrOrUsize,
}

#[derive(Serialize, Clone, Debug)]
struct CoordOut {
    x: usize,
    y: usize,
}
impl CoordOut {
    fn from_usize(x: usize, y: usize) -> Self {
        CoordOut { x, y }
    }
}

#[derive(Deserialize)]
struct Command {
    command: String,
    #[serde(default)]
    opponentMove: Option<CoordIn>,
}

#[derive(Serialize)]
struct MoveResponse {
    r#move: CoordOut,
}

#[derive(Serialize)]
struct Reply {
    reply: String,
}

#[derive(Clone)]
struct GameState {
    board: Vec<Vec<Option<bool>>>,
    first_move: bool,
}

impl GameState {
    fn new() -> Self {
        Self {
            board: vec![vec![None; BOARD_SIZE]; BOARD_SIZE],
            first_move: true,
        }
    }

    fn reset(&mut self) {
        for row in &mut self.board {
            row.fill(None);
        }
        self.first_move = true;
    }

    fn make_move(&mut self, x: usize, y: usize, is_white: bool) -> bool {
        if x >= BOARD_SIZE || y >= BOARD_SIZE || self.board[y][x].is_some() {
            return false;
        }
        self.board[y][x] = Some(is_white);
        true
    }

    fn evaluate(&self, x: usize, y: usize, is_white: bool) -> i32 {
        let mut best = 0;
        let dirs = [(1,0),(0,1),(1,1),(1,-1)];
        for &(dx,dy) in &dirs {
            let mut cnt = 1;
            for d in 1..5 {
                let nx = x as isize + dx*d;
                let ny = y as isize + dy*d;
                if nx < 0 || ny < 0 || nx >= BOARD_SIZE as isize || ny >= BOARD_SIZE as isize { break; }
                if self.board[ny as usize][nx as usize] == Some(is_white) { cnt += 1; } else { break; }
            }
            for d in 1..5 {
                let nx = x as isize - dx*d;
                let ny = y as isize - dy*d;
                if nx < 0 || ny < 0 || nx >= BOARD_SIZE as isize || ny >= BOARD_SIZE as isize { break; }
                if self.board[ny as usize][nx as usize] == Some(is_white) { cnt += 1; } else { break; }
            }
            best = best.max(cnt);
        }
        best
    }

    fn is_forbidden_black_move(&self, x: usize, y: usize) -> bool {
        if self.max_in_row_if_move(x, y, false) > 5 {
            return true;
        }
        let (open_threes, open_fours) = self.count_open_patterns_if_move(x, y, false);
        if open_threes >= 2 { return true; }
        if open_fours >= 2 { return true; }
        if open_fours >= 1 && open_threes >= 1 { return true; }
        false
    }

    fn max_in_row_if_move(&self, x: usize, y: usize, is_white: bool) -> i32 {
        let mut max = 0;
        let dirs = [(1,0),(0,1),(1,1),(1,-1)];
        for &(dx,dy) in &dirs {
            let mut cnt = 1;
            for d in 1..5 {
                let nx = x as isize + dx*d;
                let ny = y as isize + dy*d;
                if nx < 0 || ny < 0 || nx >= BOARD_SIZE as isize || ny >= BOARD_SIZE as isize { break; }
                let cell = if nx as usize == x && ny as usize == y { Some(is_white) }
                    else { self.board[ny as usize][nx as usize] };
                if cell == Some(is_white) { cnt += 1; } else { break; }
            }
            for d in 1..5 {
                let nx = x as isize - dx*d;
                let ny = y as isize - dy*d;
                if nx < 0 || ny < 0 || nx >= BOARD_SIZE as isize || ny >= BOARD_SIZE as isize { break; }
                let cell = if nx as usize == x && ny as usize == y { Some(is_white) }
                    else { self.board[ny as usize][nx as usize] };
                if cell == Some(is_white) { cnt += 1; } else { break; }
            }
            max = max.max(cnt);
        }
        max
    }

    fn count_open_patterns_if_move(&self, x: usize, y: usize, is_white: bool) -> (i32, i32) {
        let dirs = [(1,0),(0,1),(1,1),(1,-1)];
        let mut open_threes = 0;
        let mut open_fours = 0;
        for &(dx, dy) in &dirs {
            let mut line = Vec::new();
            for step in -4..=4 {
                let nx = x as isize + dx * step;
                let ny = y as isize + dy * step;
                if nx < 0 || ny < 0 || nx >= BOARD_SIZE as isize || ny >= BOARD_SIZE as isize {
                    line.push(None);
                } else if nx as usize == x && ny as usize == y {
                    line.push(Some(is_white));
                } else {
                    line.push(self.board[ny as usize][nx as usize]);
                }
            }
            for i in 0..=line.len()-5 {
                let five = &line[i..i+5];
                if five == &[None, Some(is_white), Some(is_white), Some(is_white), None] {
                    open_threes += 1;
                }
                if five == &[None, Some(is_white), Some(is_white), Some(is_white), Some(is_white)] &&
                   (i+5 < line.len() && line[i+5] == None)
                {
                    open_fours += 1;
                }
                if (i > 0 && line[i-1] == None) && five == &[Some(is_white), Some(is_white), Some(is_white), Some(is_white), None] {
                    open_fours += 1;
                }
            }
        }
        (open_threes, open_fours)
    }

    fn find_best_move(&mut self) -> Option<(usize, usize)> {
        let mut best_score = -1;
        let mut best_moves = Vec::new();
        for y in 0..BOARD_SIZE {
            for x in 0..BOARD_SIZE {
                if self.board[y][x].is_some() { continue; }
                if self.is_forbidden_black_move(x, y) { continue; }
                self.board[y][x] = Some(false);
                if self.max_in_row_if_move(x, y, false) == 5 {
                    self.board[y][x] = None;
                    return Some((x, y));
                }
                self.board[y][x] = None;
                self.board[y][x] = Some(true);
                if self.max_in_row_if_move(x, y, true) == 5 {
                    self.board[y][x] = None;
                    return Some((x, y));
                }
                self.board[y][x] = None;
            }
        }
        for y in 0..BOARD_SIZE {
            for x in 0..BOARD_SIZE {
                if self.board[y][x].is_some() { continue; }
                if self.is_forbidden_black_move(x, y) { continue; }
                let my_score  = self.evaluate(x, y, false);
                let opp_score = self.evaluate(x, y, true);
                let mut score = match (my_score, opp_score) {
                    (4, _) => 1000,
                    (_, 4) => 900,
                    (3, _) => 400,
                    (_, 3) => 350,
                    (2, _) => 100,
                    (_, 2) => 90,
                    _ => 0
                };
                let dist = (x as isize - CENTER as isize).abs()
                         + (y as isize - CENTER as isize).abs();
                score += (BOARD_SIZE as i32 - dist as i32) / 2;
                if score > best_score {
                    best_score = score;
                    best_moves.clear();
                    best_moves.push((x, y));
                } else if score == best_score {
                    best_moves.push((x, y));
                }
            }
        }
        best_moves.choose(&mut thread_rng()).copied()
    }
}

fn error(msg: &str) -> HashMap<&str, &str> {
    HashMap::from([("error", msg)])
}

fn handle_client(mut sock: TcpStream, state: Arc<Mutex<GameState>>) {
    let peer = sock.peer_addr().unwrap();
    let mut reader = BufReader::new(sock.try_clone().unwrap());

    loop {
        let mut buf = String::new();
        if reader.read_line(&mut buf).is_err() { break; }
        let line = buf.trim();
        if line.is_empty() { continue; }

        let resp_json = {
            let mut game = state.lock().unwrap();
            match serde_json::from_str::<Command>(line) {
                Ok(cmd) => match cmd.command.as_str() {
                    "start" => {
                        if game.first_move {
                            game.make_move(CENTER, CENTER, false);
                            game.first_move = false;
                            serde_json::to_string(&MoveResponse { r#move: CoordOut::from_usize(CENTER, CENTER) })
                                .unwrap()
                        } else {
                            serde_json::to_string(&error("Not first move")).unwrap()
                        }
                    }
                    "move" => {
    if let Some(c) = cmd.opponentMove {
        let x = c.x.as_usize();
        let y = c.y.as_usize();

        if game.board[y][x].is_none() {
            game.board[y][x] = Some(true);
        }
        if let Some((bx, by)) = game.find_best_move() {
            if game.board[by][bx].is_none() {
                game.board[by][bx] = Some(false);
                serde_json::to_string(&MoveResponse { r#move: CoordOut::from_usize(bx, by) }).unwrap()
            } else {
                serde_json::to_string(&error("Move already taken")).unwrap()
            }
        } else {
            serde_json::to_string(&error("No valid move found")).unwrap()
        }
    } else {
        serde_json::to_string(&error("No opponent move")).unwrap()
    }
}
                    "reset" => {
                        game.reset();
                        serde_json::to_string(&Reply { reply: "ok".into() }).unwrap()
                    }
                    _ => serde_json::to_string(&error("Unknown command")).unwrap(),
                },
                Err(_) => serde_json::to_string(&error("Wrong JSON format")).unwrap(),
            }
        } + "\n";

        if sock.write_all(resp_json.as_bytes()).is_err() {
            eprintln!("Lost connection to {}", peer);
            break;
        }
    }
}

fn main() -> std::io::Result<()> {
    let port = env::args()
        .find_map(|arg| arg.strip_prefix("-p")?.parse::<u16>().ok())
        .unwrap_or(54321);
    let addr = format!("0.0.0.0:{}", port);

    let listener = TcpListener::bind(&addr)?;
    println!("Server running on {}", addr);

    let state = Arc::new(Mutex::new(GameState::new()));
    for stream in listener.incoming() {
        match stream {
            Ok(sock) => {
                let st = Arc::clone(&state);
                thread::spawn(move || handle_client(sock, st));
            }
            Err(e) => eprintln!("Connection error: {}", e),
        }
    }
    Ok(())
}
