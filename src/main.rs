use std::collections::HashMap;
use std::env;
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;

use rand::seq::SliceRandom;
use rand::thread_rng;
use serde::{Deserialize, Serialize};

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
    my_board: Vec<Vec<bool>>,
    opponent_moves: Vec<(usize, usize)>,
    first_move: bool,
    all_debuts: Vec<Vec<(usize, usize)>>,
    total_moves: Vec<(usize, usize)>,
}

impl GameState {
    fn new() -> Self {
        let all_debuts = vec![
            vec![(15, 15), (16, 15), (14, 14), (13, 15)],
            vec![(15, 15), (15, 14), (16, 16), (14, 14)],
            vec![(15, 15), (14, 15), (16, 14), (13, 16)],
            vec![(15, 15), (16, 16), (14, 14), (15, 13)],
            vec![(15, 15), (17, 15), (13, 13), (14, 16)],
            vec![(15, 15), (16, 16), (17, 17), (18, 18)],
            vec![(15, 15), (14, 14), (13, 13), (12, 12)],
            vec![(15, 15), (14, 16), (13, 17), (12, 18)],
            vec![(15, 15), (16, 14), (17, 13), (18, 12)],
            vec![(15, 15), (15, 16), (15, 17), (15, 18)],
            vec![(15, 15), (15, 14), (15, 13), (15, 12)],
            vec![(15, 15), (16, 15), (17, 15), (18, 15)],
            vec![(15, 15), (14, 16), (13, 17), (14, 18)],
            vec![(15, 15), (16, 14), (17, 13), (16, 12)],
            vec![(15, 15), (14, 14), (13, 13), (14, 12)],
            vec![(15, 15), (18, 15), (16, 16), (14, 15)],
            vec![(15, 15), (12, 15), (14, 14), (16, 15)],
        ];

        Self {
            my_board: vec![vec![false; BOARD_SIZE]; BOARD_SIZE],
            opponent_moves: Vec::new(),
            first_move: true,
            all_debuts,
            total_moves: Vec::new(),
        }
    }

    fn get_debut_move(&self) -> Option<(usize, usize)> {
        let total_moves_count = self.total_moves.len();

        let valid_debuts = self
            .all_debuts
            .iter()
            .filter(|debut| {
                for i in 0..self.total_moves.len() {
                    if i >= debut.len() || debut[i] != self.total_moves[i] {
                        return false;
                    }
                }
                true
            })
            .collect::<Vec<_>>();

        if valid_debuts.is_empty() {
            return None;
        }

        let debut = valid_debuts.choose(&mut thread_rng())?;

        if total_moves_count < debut.len() {
            let (x, y) = debut[total_moves_count];
            if self.is_empty(x, y) {
                return Some((x, y));
            }
        }

        None
    }

    fn reset(&mut self) {
        for row in &mut self.my_board {
            row.fill(false);
        }
        self.opponent_moves.clear();
        self.first_move = true;
        self.total_moves.clear();
    }

    fn is_my_move(&self, x: usize, y: usize) -> bool {
        self.my_board[y][x]
    }
    fn is_opponent_move(&self, x: usize, y: usize) -> bool {
        self.opponent_moves.contains(&(x, y))
    }
    fn is_empty(&self, x: usize, y: usize) -> bool {
        !self.is_my_move(x, y) && !self.is_opponent_move(x, y)
    }

    fn count_in_line(&self, x: usize, y: usize, dx: isize, dy: isize, is_my: bool) -> i32 {
        let mut count = 1;

        for dir in [-1, 1] {
            let mut step = 1;
            while step < 5 {
                let nx = x as isize + dir * dx * step;
                let ny = y as isize + dir * dy * step;
                if nx < 0 || ny < 0 || nx >= BOARD_SIZE as isize || ny >= BOARD_SIZE as isize {
                    break;
                }

                let (nx, ny) = (nx as usize, ny as usize);
                let occupied = if is_my {
                    self.is_my_move(nx, ny)
                } else {
                    self.is_opponent_move(nx, ny)
                };

                if occupied {
                    count += 1;
                } else {
                    break;
                }

                step += 1;
            }
        }

        count
    }

    fn find_best_move(&self) -> Option<(usize, usize)> {
        if let Some((x, y)) = self.get_debut_move() {
            return Some((x, y));
        }

        let mut best_score = i32::MIN;
        let mut best_moves = Vec::new();

        for y in 0..BOARD_SIZE {
            for x in 0..BOARD_SIZE {
                if !self.is_empty(x, y) {
                    continue;
                }

                let mut score = 0;
                let mut critical_block = false;
                let mut open_threes_my = 0;
                let mut open_threes_opp = 0;

                let directions = [(1, 0), (0, 1), (1, 1), (1, -1)];

                for &(dx, dy) in &directions {
                    let my_count = self.count_in_line(x, y, dx, dy, true);
                    let opp_count = self.count_in_line(x, y, dx, dy, false);

                    let my_open = self.is_open_ended(x, y, dx, dy, true);
                    let opp_open = self.is_open_ended(x, y, dx, dy, false);

                    if opp_count >= 4 {
                        critical_block = true;
                    }

                    if opp_count == 3 && opp_open {
                        score += 2500;
                        open_threes_opp += 1;
                    }

                    if my_count == 3 && my_open {
                        score += 1500;
                        open_threes_my += 1;
                    }

                    score += match opp_count {
                        5 => 100_000,
                        4 => 10_000,
                        3 => 2000,
                        2 => 400,
                        _ => 0,
                    };

                    score += match my_count {
                        5 => 100_000,
                        4 => 5000,
                        3 => 1000,
                        2 => 300,
                        _ => 0,
                    };
                }

                if open_threes_my >= 2 {
                    score += 5000;
                }

                if open_threes_opp >= 2 {
                    score += 7000;
                }

                if critical_block {
                    score += 100_000;
                }

                let dist =
                    (x as isize - CENTER as isize).abs() + (y as isize - CENTER as isize).abs();
                score -= dist as i32;

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

    fn is_open_ended(&self, x: usize, y: usize, dx: isize, dy: isize, is_my: bool) -> bool {
        let mut open_ends = 0;

        for &dir in &[-1, 1] {
            let nx = x as isize + dir * dx;
            let ny = y as isize + dir * dy;
            if nx >= 0 && ny >= 0 && nx < BOARD_SIZE as isize && ny < BOARD_SIZE as isize {
                let (nx, ny) = (nx as usize, ny as usize);
                if self.is_empty(nx, ny) {
                    open_ends += 1;
                }
            }
        }

        open_ends == 2
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
        if reader.read_line(&mut buf).is_err() {
            break;
        }
        let line = buf.trim();
        if line.is_empty() {
            continue;
        }

        let resp_json = {
            let mut game = state.lock().unwrap();
            match serde_json::from_str::<Command>(line) {
                Ok(cmd) => match cmd.command.as_str() {
                    "start" => {
                        if game.first_move {
                            game.my_board[CENTER][CENTER] = true;
                            game.first_move = false;
                            serde_json::to_string(&MoveResponse {
                                r#move: CoordOut::from_usize(CENTER, CENTER),
                            })
                            .unwrap()
                        } else {
                            serde_json::to_string(&error("Not first move")).unwrap()
                        }
                    }
                    "move" => {
                        if let Some(c) = cmd.opponentMove {
                            let x = c.x.as_usize();
                            let y = c.y.as_usize();

                            if !game.is_opponent_move(x, y) {
                                game.opponent_moves.push((x, y));
                                game.total_moves.push((x, y));
                            }
                            if let Some((bx, by)) = game.find_best_move() {
                                if game.is_empty(bx, by) {
                                    game.my_board[by][bx] = true;
                                    game.total_moves.push((bx, by));
                                    serde_json::to_string(&MoveResponse {
                                        r#move: CoordOut::from_usize(bx, by),
                                    })
                                    .unwrap()
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
