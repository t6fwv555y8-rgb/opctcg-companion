//! One look at the simulator: capture the window, read the screen cues and the
//! log panel, and say what changed.

use super::capture::{capture_window, CaptureError, Frame};
use super::livelog::{merge_wrapped, LiveGame, LogFeed};
use super::ocr::{OcrLine, OcrServer};
use super::window::{find_sim_window, screen_access_granted};
use super::board::{
    read_characters, read_hand, region_signature, signature_change, Side, SlotRead, HAND_AREA,
    OPPONENT_CHARACTERS_AREA, YOUR_CHARACTERS_AREA,
};
use crate::art::{art_hash, ArtIndex};
use crate::bridge_protocol::{BrowserGameSnapshot, ObservedCard};

/// Where things are on the simulator's screen, as fractions of the window
/// picture (x, y, width, height). Measured on the 2048x1015 window.
pub struct Regions;

impl Regions {
    pub const LOG: (f32, f32, f32, f32) = (0.115, 0.28, 0.19, 0.38);
    pub const MY_NAME: (f32, f32, f32, f32) = (0.40, 0.955, 0.20, 0.045);
    pub const OPPONENT_NAME: (f32, f32, f32, f32) = (0.435, 0.02, 0.13, 0.055);
    pub const TURN_LABEL: (f32, f32, f32, f32) = (0.665, 0.075, 0.075, 0.07);
    pub const END_TURN_BUTTON: (f32, f32, f32, f32) = (0.675, 0.655, 0.23, 0.11);
    /// The column of buttons ("Concede", "Download Combat Log") that only a game screen has.
    pub const GAME_BUTTONS: (f32, f32, f32, f32) = (0.79, 0.08, 0.10, 0.31);
    /// The two Leader cards on the board (x0, y0, x1, y1).
    pub const MY_LEADER: (f32, f32, f32, f32) = (0.4985, 0.676, 0.545, 0.802);
    pub const OPPONENT_LEADER: (f32, f32, f32, f32) = (0.456, 0.224, 0.501, 0.351);
}

/// Window shapes this layout was measured for. Others are read less reliably.
const EXPECTED_ASPECT: f32 = 2048.0 / 1015.0;

fn crop(frame: &Frame, r: (f32, f32, f32, f32), fast: bool) -> (u32, u32, u32, u32, bool) {
    let (w, h) = (frame.image.width() as f32, frame.image.height() as f32);
    ((r.0 * w) as u32, (r.1 * h) as u32, (r.2 * w) as u32, (r.3 * h) as u32, fast)
}

fn joined(lines: &[OcrLine]) -> String {
    lines.iter().map(|l| l.text.as_str()).collect::<Vec<_>>().join(" ")
}

/// What the HUD should be told about the reader itself.
#[derive(Debug, Clone, PartialEq)]
pub enum ReaderStatus {
    Reading,
    NoWindow,
    /// The simulator is open but not on a game board (menus, deck editor, lobby).
    NotInGame,
    NeedsScreenAccess,
    NeedsTextReader(String),
    OddWindowShape,
    Error(String),
}

impl ReaderStatus {
    pub fn message(&self) -> String {
        match self {
            Self::Reading => "Reading OPTCGSim".into(),
            Self::NoWindow => "OPTCGSim isn't open (or is minimised).".into(),
            Self::NotInGame => "OPTCGSim is open, waiting for a game to start.".into(),
            Self::NeedsScreenAccess => super::window::SCREEN_ACCESS_HELP.into(),
            Self::NeedsTextReader(m) | Self::Error(m) => m.clone(),
            Self::OddWindowShape => "The OPTCGSim window has an unusual shape: resize it to roughly 2:1 for reliable reading.".into(),
        }
    }
}

/// A reading that is only trusted once two looks in a row agree, so a card
/// mid-animation never flickers into the game state.
#[derive(Debug, Clone, PartialEq)]
struct Steady<T: Clone + PartialEq> {
    stable: T,
    pending: Option<T>,
}

impl<T: Clone + PartialEq> Steady<T> {
    fn new(value: T) -> Self {
        Self { stable: value, pending: None }
    }

    fn see(&mut self, value: T) {
        if value == self.stable {
            self.pending = None;
        } else if self.pending.as_ref() == Some(&value) {
            self.stable = value;
            self.pending = None;
        } else {
            self.pending = Some(value);
        }
    }
}

type Row = Vec<Option<(String, bool)>>;

/// What was last read from the board, and the fingerprints used to know when to read again.
struct BoardMemory {
    hand: Steady<(usize, Vec<Option<String>>)>,
    mine: Steady<Row>,
    theirs: Steady<Row>,
    signatures: [Vec<u8>; 3],
}

impl BoardMemory {
    fn new() -> Self {
        Self {
            hand: Steady::new((0, Vec::new())),
            mine: Steady::new(Vec::new()),
            theirs: Steady::new(Vec::new()),
            signatures: [Vec::new(), Vec::new(), Vec::new()],
        }
    }
}

fn row_of(reads: Vec<Option<SlotRead>>) -> Row {
    reads.into_iter().map(|r| r.map(|s| (s.card_id, s.rested))).collect()
}

pub struct SimReader {
    board: BoardMemory,
    art: Option<ArtIndex>,
    art_tried: bool,
    ocr: Option<OcrServer>,
    feed: LogFeed,
    game: LiveGame,
    last_leaders: (Option<String>, Option<String>),
    /// Leaders recognised from the board, kept until a new game starts.
    seen_leader_mine: Option<String>,
    seen_leader_theirs: Option<String>,
}

pub struct Poll {
    pub status: ReaderStatus,
    pub snapshot: Option<BrowserGameSnapshot>,
    /// New log messages since the last poll (for the match recorder and debugging).
    pub new_messages: Vec<String>,
}

impl Default for SimReader {
    fn default() -> Self {
        Self::new()
    }
}

impl SimReader {
    pub fn new() -> Self {
        Self { board: BoardMemory::new(), art: None, art_tried: false, ocr: None, feed: LogFeed::default(), game: LiveGame::default(), last_leaders: (None, None), seen_leader_mine: None, seen_leader_theirs: None }
    }

    fn ocr(&mut self) -> Result<&mut OcrServer, String> {
        if self.ocr.is_none() {
            self.ocr = Some(OcrServer::start()?);
        }
        Ok(self.ocr.as_mut().expect("just started"))
    }

    /// The card art index, built the first time it is needed from the
    /// simulator's own pictures.
    fn art_index(&mut self) -> Option<&ArtIndex> {
        if self.art.is_none() && !self.art_tried {
            self.art_tried = true;
            self.art = find_cards_dir().map(|dir| ArtIndex::build(&dir)).filter(|i| !i.is_empty());
        }
        self.art.as_ref()
    }

    /// Recognise a Leader card on the board by its artwork.
    fn leader_at(&mut self, frame: &Frame, r: (f32, f32, f32, f32)) -> Option<String> {
        let (w, h) = (frame.image.width() as f32, frame.image.height() as f32);
        let card = frame.image.crop_imm((r.0 * w) as u32, (r.1 * h) as u32, ((r.2 - r.0) * w) as u32, ((r.3 - r.1) * h) as u32);
        self.art_index()?.identify(&art_hash(&card)).map(|m| m.card_id)
    }

    /// Re-read a part of the board only when that part of the picture changed.
    fn read_board(&mut self, frame: &Frame) {
        if self.art_index().is_none() {
            return;
        }
        let areas = [HAND_AREA, YOUR_CHARACTERS_AREA, OPPONENT_CHARACTERS_AREA];
        let mut changed = [false; 3];
        for (i, a) in areas.iter().enumerate() {
            let sig = region_signature(&frame.image, a.0, a.1, a.2, a.3);
            changed[i] = signature_change(&self.board.signatures[i], &sig) > 1.5;
            if changed[i] {
                self.board.signatures[i] = sig;
            }
        }
        // Take the index out so the board memory can be updated while it is borrowed.
        let Some(art) = self.art.take() else { return };
        if changed[0] {
            self.board.hand.see(read_hand(&frame.image, &art));
        }
        if changed[1] {
            self.board.mine.see(row_of(read_characters(&frame.image, &art, Side::You)));
        }
        if changed[2] {
            self.board.theirs.see(row_of(read_characters(&frame.image, &art, Side::Opponent)));
        }
        self.art = Some(art);
    }

    /// Put what was read off the board into the snapshot.
    fn apply_board(&self, snapshot: &mut BrowserGameSnapshot) {
        let (count, hand) = &self.board.hand.stable;
        let characters = |row: &Row, pid: &str| -> Vec<ObservedCard> {
            row.iter()
                .enumerate()
                .filter_map(|(slot, c)| {
                    let (id, rested) = c.as_ref()?;
                    Some(ObservedCard {
                        card_id: Some(id.clone()),
                        instance_key: Some(format!("{pid}:character:{slot}:{id}:s{slot}")),
                        rested: Some(*rested),
                        ..Default::default()
                    })
                })
                .collect()
        };
        if let Some(me) = snapshot.self_player.as_mut() {
            if *count > 0 {
                me.hand_count = Some((*count).min(255) as u8);
                let ids: Vec<String> = hand.iter().flatten().cloned().collect();
                for id in &ids {
                    if !me.known_cards.contains(id) {
                        me.known_cards.push(id.clone());
                    }
                }
                me.hand = ids
                    .into_iter()
                    .map(|id| ObservedCard { card_id: Some(id), ..Default::default() })
                    .collect();
            }
            me.board = characters(&self.board.mine.stable, "0");
            for c in &me.board {
                if let Some(id) = &c.card_id {
                    if !me.known_cards.contains(id) {
                        me.known_cards.push(id.clone());
                    }
                }
            }
        }
        if let Some(opp) = snapshot.opponent.as_mut() {
            opp.board = characters(&self.board.theirs.stable, "1");
            for c in &opp.board {
                if let Some(id) = &c.card_id {
                    if !opp.known_cards.contains(id) {
                        opp.known_cards.push(id.clone());
                    }
                }
            }
        }
    }

    /// Look once. Cheap enough to call every second or two.
    pub fn poll(&mut self) -> Poll {
        let fail = |status| Poll { status, snapshot: None, new_messages: Vec::new() };
        let Some(window) = find_sim_window() else {
            return fail(ReaderStatus::NoWindow);
        };
        if !screen_access_granted() {
            // Capture may still work (permission can belong to a parent app); try it.
        }
        let frame = match capture_window(&window) {
            Ok(f) => f,
            Err(CaptureError::PermissionDenied) => return fail(ReaderStatus::NeedsScreenAccess),
            Err(CaptureError::WindowGone) => return fail(ReaderStatus::NoWindow),
            Err(CaptureError::Other(m)) => return fail(ReaderStatus::Error(m)),
        };
        let aspect = frame.image.width() as f32 / frame.image.height().max(1) as f32;
        let odd = (aspect / EXPECTED_ASPECT - 1.0).abs() > 0.08;

        // The log needs careful reading; the short labels do not. Your name is
        // only read until it is known.
        let need_name = self.game.me().is_none();
        let mut crops = vec![
            crop(&frame, Regions::LOG, false),
            crop(&frame, Regions::TURN_LABEL, true),
            crop(&frame, Regions::END_TURN_BUTTON, true),
            crop(&frame, Regions::GAME_BUTTONS, true),
        ];
        if need_name {
            crops.push(crop(&frame, Regions::MY_NAME, false));
        }
        let regions = match self.ocr().and_then(|o| o.read(&frame.path, &crops)) {
            Ok(r) => r,
            Err(m) if m.contains("command line tools") => return fail(ReaderStatus::NeedsTextReader(m)),
            Err(m) => {
                self.ocr = None; // restart the reader next time
                return fail(ReaderStatus::Error(m));
            }
        };

        // Only a game board has these buttons. Anywhere else (menus, deck editor)
        // there is nothing to read, and what is on screen must not be mistaken for a log.
        let buttons = joined(&regions[3]).to_lowercase();
        if !(buttons.contains("concede") || buttons.contains("combat") || buttons.contains("cancel match")) {
            // A game that ended is over: start fresh for the next one.
            let me = self.game.me().map(str::to_string);
            self.game = LiveGame::default();
            self.feed = LogFeed::default();
            self.seen_leader_mine = None;
            self.seen_leader_theirs = None;
            self.last_leaders = (None, None);
            self.board = BoardMemory::new();
            if let Some(me) = me {
                self.game.set_me(&me);
            }
            return fail(ReaderStatus::NotInGame);
        }

        // Who you are, from the name under your side of the board.
        if need_name {
            if let Some(me) = regions[4].iter().map(|l| l.text.trim()).find(|t| t.contains('#')) {
                self.game.set_me(me);
            }
        }

        // New log messages.
        let screen: Vec<String> = regions[0].iter().map(|l| l.text.clone()).collect();
        let window_messages = merge_wrapped(&screen);
        let fresh = self.feed.update(window_messages);
        for message in &fresh {
            self.game.apply(message);
        }

        // Screen cues.
        let turn_number = joined(&regions[1])
            .to_lowercase()
            .split("turn")
            .nth(1)
            .and_then(|rest| rest.trim().chars().take_while(|c| c.is_ascii_digit()).collect::<String>().parse::<u32>().ok());
        let button = joined(&regions[2]).to_lowercase();
        let my_turn = button.contains("end turn").then_some(true);

        self.read_board(&frame);

        let mut snapshot = self.game.snapshot(my_turn, turn_number);
        snapshot.timestamp = chrono::Utc::now().timestamp_millis();
        self.apply_board(&mut snapshot);
        // The log only names the Leaders at the start of a game. Joining later,
        // read them off the board instead.
        let needs_mine = snapshot.self_player.as_ref().and_then(|p| p.leader_id.as_ref()).is_none();
        let needs_theirs = snapshot.opponent.as_ref().and_then(|p| p.leader_id.as_ref()).is_none();
        if needs_mine {
            if let Some(id) = self.leader_at(&frame, Regions::MY_LEADER) {
                self.seen_leader_mine = Some(id);
            }
        }
        if needs_theirs {
            if let Some(id) = self.leader_at(&frame, Regions::OPPONENT_LEADER) {
                self.seen_leader_theirs = Some(id);
            }
        }
        if let (Some(p), Some(id)) = (snapshot.self_player.as_mut(), self.seen_leader_mine.clone()) {
            p.leader_id.get_or_insert(id);
        }
        if let (Some(p), Some(id)) = (snapshot.opponent.as_mut(), self.seen_leader_theirs.clone()) {
            p.leader_id.get_or_insert(id);
        }
        // A leader never changes within a game; a new one means a new game.
        let leaders = (
            snapshot.self_player.as_ref().and_then(|p| p.leader_id.clone()),
            snapshot.opponent.as_ref().and_then(|p| p.leader_id.clone()),
        );
        if leaders != self.last_leaders && self.last_leaders.0.is_some() && leaders.0 != self.last_leaders.0 {
            let me = self.game.me().map(str::to_string);
            self.game = LiveGame::default();
            self.seen_leader_mine = None;
            self.seen_leader_theirs = None;
            self.board = BoardMemory::new();
            if let Some(me) = me {
                self.game.set_me(&me);
            }
        }
        self.last_leaders = leaders;

        Poll {
            status: if odd { ReaderStatus::OddWindowShape } else { ReaderStatus::Reading },
            snapshot: Some(snapshot),
            new_messages: fresh,
        }
    }
}


/// Where the simulator keeps its card pictures, wherever it is installed.
pub fn find_cards_dir() -> Option<std::path::PathBuf> {
    use std::path::PathBuf;
    let tail = "Contents/Resources/Data/StreamingAssets/Cards";
    let mut apps: Vec<PathBuf> = Vec::new();
    if let Ok(custom) = std::env::var("OPTCG_SIM_APP") {
        apps.push(PathBuf::from(custom));
    }
    // The app that is running right now: its path comes from its process.
    if let Some(window) = find_sim_window() {
        if let Ok(out) = std::process::Command::new("/bin/ps")
            .args(["-p", &window.owner_pid.to_string(), "-o", "comm="])
            .output()
        {
            let exe = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if let Some(end) = exe.find(".app/") {
                apps.push(PathBuf::from(&exe[..end + 4]));
            }
        }
    }
    apps.push(PathBuf::from("/Applications/OPTCGSim.app"));
    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        for dir in ["Applications", "Downloads", "Desktop", "Documents"] {
            let base = home.join(dir);
            apps.push(base.join("OPTCGSim.app"));
            if let Ok(entries) = std::fs::read_dir(&base) {
                for e in entries.flatten().filter(|e| e.path().is_dir()) {
                    apps.push(e.path().join("OPTCGSim.app"));
                }
            }
        }
    }
    apps.into_iter().map(|a| a.join(tail)).find(|p| p.is_dir())
}
