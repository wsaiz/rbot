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
}

impl GameState {
    fn new() -> Self {
        Self {
            my_board: vec![vec![false; BOARD_SIZE]; BOARD_SIZE],
            opponent_moves: Vec::new(),
            first_move: true,
        }
    }

    fn reset(&mut self) {
        for row in &mut self.my_board {
            row.fill(false);
        }
        self.opponent_moves.clear();
        self.first_move = true;
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
        let mut best_score = -1;
        let mut best_moves = Vec::new();

        for y in 0..BOARD_SIZE {
            for x in 0..BOARD_SIZE {
                if !self.is_empty(x, y) {
                    continue;
                }

                let mut my_max = 0;
                let mut opp_max = 0;
                let directions = [(1, 0), (0, 1), (1, 1), (1, -1)];

                for &(dx, dy) in &directions {
                    my_max = my_max.max(self.count_in_line(x, y, dx, dy, true));
                    opp_max = opp_max.max(self.count_in_line(x, y, dx, dy, false));
                }

                let score = match (my_max, opp_max) {
                    (5, _) | (_, 5) => 10000,
                    (4, _) => 1000,
                    (_, 4) => 900,
                    (3, _) => 500,
                    (_, 3) => 450,
                    (2, _) => 100,
                    (_, 2) => 90,
                    _ => {
                        let dist = (x as isize - CENTER as isize).abs()
                            + (y as isize - CENTER as isize).abs();
                        (BOARD_SIZE as isize - dist) as i32
                    }
                };

                if score > best_score {
                    best_score = score;
                    best_moves.clear();
                }

                if score == best_score && !self.is_opponent_move(x, y) {
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
                            }
                            if let Some((bx, by)) = game.find_best_move() {
                                if game.is_empty(bx, by) {
                                    game.my_board[by][bx] = true;
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
