
use std::collections::HashMap;
use std::env;
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;
use rand::seq::SliceRandom;
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
    team: &'static str,
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
    total_moves: Vec<(usize, usize)>, 
}
enum LineThreat {
    Five,
    OpenFour,
    BlockedFour,
    OpenThree,
    BlockedThree,
    Two,
    Other,
}



impl GameState {
    fn new() -> Self {
        Self {
            my_board: vec![vec![false; BOARD_SIZE]; BOARD_SIZE],
            opponent_moves: Vec::new(),
            first_move: true,
            total_moves: Vec::new(),
        }
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

    
fn evaluate_line_type(&self, x: usize, y: usize, dx: isize, dy: isize, is_my: bool) -> LineThreat {
        let mut line = Vec::new();

        for offset in -4..=4 {
            let nx = x as isize + offset * dx;
            let ny = y as isize + offset * dy;

            if nx < 0 || ny < 0 || nx >= BOARD_SIZE as isize || ny >= BOARD_SIZE as isize {
                line.push('B');
            } else {
                let (nx, ny) = (nx as usize, ny as usize);
                line.push(match (
                    self.is_my_move(nx, ny),
                    self.is_opponent_move(nx, ny),
                ) {
                    (true, false) if is_my => 'X',
                    (false, true) if !is_my => 'O',
                    (false, false) => '.',
                    _ => 'B',
                });
            }
        }

        let line_str: String = line.iter().collect();
        let s = line_str.as_str();

        let (five, open4, block4, open3, block3, two) = if is_my {
            ("XXXXX", ".XXXX.", ["XXXX.", ".XXXX", "X.XXX", "XX.XX"], ".XXX.", ["XXX.", ".XXX", "X.XX", "XX.X"], "XX")
        } else {
            ("OOOOO", ".OOOO.", ["OOOO.", ".OOOO", "O.OOO", "OO.OO"], ".OOO.", ["OOO.", ".OOO", "O.OO", "OO.O"], "OO")
        };

        if s.contains(five) {
            LineThreat::Five
        } else if s.contains(open4) {
            LineThreat::OpenFour
        } else if block4.iter().any(|pat| s.contains(pat)) {
            LineThreat::BlockedFour
        } else if s.contains(open3) {
            LineThreat::OpenThree
        } else if block3.iter().any(|pat| s.contains(pat)) {
            LineThreat::BlockedThree
        } else if s.contains(two) {
            LineThreat::Two
        } else {
            LineThreat::Other
        }
    }

   fn find_best_move(&self) -> Option<(usize, usize)> {
        let mut rng = rand::thread_rng();

        for y in 0..BOARD_SIZE {
            for x in 0..BOARD_SIZE {
                if !self.is_empty(x, y) {
                    continue;
                }
                let mut my_fours = 0;
                let mut my_threes = 0;
                for &(dx, dy) in &[(1, 0), (0, 1), (1, 1), (1, -1)] {
                    match self.evaluate_line_type(x, y, dx, dy, true) {
                        LineThreat::OpenFour => my_fours += 1,
                        LineThreat::OpenThree => my_threes += 1,
                        _ => {}
                    }
                }
                if my_fours > 0 || my_threes >= 2 {
                    return Some((x, y));
                }
            }
        }

        let mut best_moves = vec![];
        let mut best_score = i32::MIN;

        for y in 0..BOARD_SIZE {
            for x in 0..BOARD_SIZE {
                if !self.is_empty(x, y) {
                    continue;
                }

                let mut score = 0;
                let mut my_open_threes = 0;
                let mut my_open_fours = 0;
                let mut opp_open_threes = 0;
                let mut opp_open_fours = 0;

                for &(dx, dy) in &[(1, 0), (0, 1), (1, 1), (1, -1)] {
                    match self.evaluate_line_type(x, y, dx, dy, true) {
                        LineThreat::Five => score += 1_000_000,
                        LineThreat::OpenFour => { my_open_fours += 1; score += 80_000; },
                        LineThreat::BlockedFour => score += 12_000,
                        LineThreat::OpenThree => { my_open_threes += 1; score += 3_000; },
                        LineThreat::BlockedThree => score += 500,
                        LineThreat::Two => score += 100,
                        _ => {}
                    }
                    match self.evaluate_line_type(x, y, dx, dy, false) {
                        LineThreat::Five => score += 900_000,
                        LineThreat::OpenFour => { opp_open_fours += 1; score += 55_000; },
                        LineThreat::BlockedFour => score += 12_000,
                        LineThreat::OpenThree => { opp_open_threes += 1; score += 3_000; },
                        LineThreat::BlockedThree => score += 1000,
                        LineThreat::Two => score += 200,
                        _ => {}
                    }
                }

                if my_open_fours > 0 && my_open_threes > 0 {
                    score += 150_000;
                }
                if opp_open_fours > 0 && opp_open_threes > 0 {
                    score += 100_000;
                }
                if my_open_threes >= 2 {
                    score += 10_000;
                }
                if opp_open_threes >= 2 {
                    score += 7_000;
                }

                let dist = (x as isize - CENTER as isize).abs() + (y as isize - CENTER as isize).abs();
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

        best_moves.choose(&mut rng).copied()
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
                                team: "team crabs",
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
                                        team: "team crabs",
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