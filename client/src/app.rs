use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind};
use rust_decimal::Decimal;
use std::collections::VecDeque;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::str::FromStr;

use tokio::sync::mpsc;
use trade_shared::{
    calculate_position_size, round_quantity, AutostopParams, AutostopType, Candle, ClientMessage,
    ClientPayload, ClosePositionRequest, Order, OrderRequest, OrderType, ActivityStopRequest,
    PendingAutostopCommand, PendingAutostopConfig, Position, RiskError, ServerMessage, ServerPayload,
    Side, Symbol, TimeInForce, Trade, TrailingStopRequest, TrailingStopTarget,
};

use crate::audio::AudioPlayer;

struct Cmd {
    name: &'static str,
    aliases: &'static [&'static str],
}

const COMMANDS: &[Cmd] = &[
    Cmd { name: "buy", aliases: &["b"] },
    Cmd { name: "sell", aliases: &["s"] },
    Cmd { name: "buyrisk", aliases: &["br"] },
    Cmd { name: "sellrisk", aliases: &["sr"] },
    Cmd { name: "cancel", aliases: &["c"] },
    Cmd { name: "cancelall", aliases: &["ca"] },
    Cmd { name: "close", aliases: &["cl"] },
    Cmd { name: "symbol", aliases: &["sym"] },
    Cmd { name: "positions", aliases: &["pos"] },
    Cmd { name: "orders", aliases: &["ord"] },
    Cmd { name: "ts", aliases: &[] },
    Cmd { name: "help", aliases: &["h"] },
    Cmd { name: "chart", aliases: &["ch"] },
    Cmd { name: "tf", aliases: &[] },
    Cmd { name: "level", aliases: &["lv"] },
    Cmd { name: "clevel", aliases: &["clv"] },
    Cmd { name: "sound", aliases: &["snd"] },
    Cmd { name: "activityStop", aliases: &["as"] },
];

fn match_command(input: &str) -> Option<&'static str> {
    for cmd in COMMANDS {
        if cmd.name == input || cmd.aliases.contains(&input) {
            return Some(cmd.name);
        }
    }
    None
}

fn all_command_names() -> Vec<&'static str> {
    let mut names = Vec::new();
    for cmd in COMMANDS {
        names.push(cmd.name);
        names.extend(cmd.aliases.iter());
    }
    names
}

#[derive(Debug, Clone)]
struct PendingRiskOrder {
    side: Side,
    risk_usdt: Decimal,
    sl_price: Decimal,
    sl_percent: Option<Decimal>,
    symbol: String,
    tp_value: Option<Value>,
}

#[derive(Debug, Clone)]
struct PendingAction {
    symbol: String,
    side: Side,
    actions: Vec<String>,
}

#[derive(Debug, Clone)]
enum Value {
    Percent(Decimal),
    Absolute(Decimal),
    Ratio(u32, u32),
}

fn parse_value(s: &str) -> Option<Value> {
    if s.ends_with('%') {
        let num = s.trim_end_matches('%');
        Decimal::from_str(num).ok().map(Value::Percent)
    } else if s.contains('/') {
        let parts: Vec<&str> = s.split('/').collect();
        if parts.len() == 2 {
            if let (Ok(num), Ok(denom)) = (parts[0].parse::<u32>(), parts[1].parse::<u32>()) {
                if denom > 0 {
                    return Some(Value::Ratio(num, denom));
                }
            }
        }
        None
    } else {
        Decimal::from_str(s).ok().map(Value::Absolute)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputMode {
    Normal,
    Command,
    Copy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Trade,
    Chart,
    Orders,
    Positions,
    Autostops,
}

pub struct App {
    pub input_mode: InputMode,
    pub input: String,
    pub input_cursor: usize,
    pub tab: Tab,
    pub orders: Vec<Order>,
    pub positions: Vec<Position>,
    pub messages: Vec<String>,
    pub connected: bool,
    pub symbol: String,
    pub last_price: Option<Decimal>,
    conn_tx: mpsc::Sender<ClientMessage>,
    server_rx: mpsc::Receiver<ServerMessage>,
    pending_risk_order: Option<PendingRiskOrder>,
    pending_action: Option<PendingAction>,
    command_history: Vec<String>,
    history_index: Option<usize>,
    pub candles: Vec<Candle>,
    pub trades: VecDeque<Trade>,
    pub chart_interval: String,
    pub chart_levels: Vec<Decimal>,
    pub chart_offset: usize,
    pub chart_zoom: u8,
    pub chart_zoom_v: u8,
    pub chart_offset_v: i32,
    pub sound_enabled: bool,
    audio_player: Option<AudioPlayer>,
    pub copy_index: usize,
    clipboard: Option<arboard::Clipboard>,
    pub activity_stop_remaining_ms: Option<u64>,
    pub activity_stop_active: bool,
    pub activity_stop_trigger_price: Option<Decimal>,
    pub mouse_position: Option<(u16, u16)>,
    pub chart_area: Option<ratatui::layout::Rect>,
    pub price_tracking: bool,
    pub pending_autostops: Vec<PendingAutostopCommand>,
    pub last_placed_order_id: Option<String>,
}

const HISTORY_FILE: &str = ".trade_history";
const MAX_HISTORY: usize = 500;

impl App {
    fn find_word_start_left(input: &str, cursor: usize) -> usize {
        if cursor == 0 {
            return 0;
        }
        let chars: Vec<char> = input.chars().collect();
        let mut pos = cursor.saturating_sub(1);

        while pos > 0 && chars[pos].is_whitespace() {
            pos -= 1;
        }

        while pos > 0 && !chars[pos - 1].is_whitespace() {
            pos -= 1;
        }

        pos
    }

    fn find_word_end_right(input: &str, cursor: usize) -> usize {
        let chars: Vec<char> = input.chars().collect();
        if cursor >= chars.len() {
            return chars.len();
        }
        let mut pos = cursor;

        while pos < chars.len() && chars[pos].is_whitespace() {
            pos += 1;
        }

        while pos < chars.len() && !chars[pos].is_whitespace() {
            pos += 1;
        }

        pos
    }

    pub fn new(
        conn_tx: mpsc::Sender<ClientMessage>,
        server_rx: mpsc::Receiver<ServerMessage>,
    ) -> Self {
        Self {
            input_mode: InputMode::Normal,
            input: String::new(),
            input_cursor: 0,
            tab: Tab::Trade,
            orders: Vec::new(),
            positions: Vec::new(),
            messages: Vec::new(),
            connected: false,
            symbol: "BTCUSDT".to_string(),
            last_price: None,
            conn_tx,
            server_rx,
            pending_risk_order: None,
            pending_action: None,
            command_history: Self::load_history(),
            history_index: None,
            candles: Vec::new(),
            trades: VecDeque::with_capacity(100),
            chart_interval: "5".to_string(),
            chart_levels: Vec::new(),
            chart_offset: 0,
            chart_zoom: 2,
            chart_zoom_v: 1,
            chart_offset_v: 0,
            sound_enabled: true,
            audio_player: AudioPlayer::new(),
            copy_index: 0,
            clipboard: arboard::Clipboard::new().ok(),
            activity_stop_remaining_ms: None,
            activity_stop_active: false,
            activity_stop_trigger_price: None,
            mouse_position: None,
            chart_area: None,
            price_tracking: false,
            pending_autostops: Vec::new(),
            last_placed_order_id: None,
        }
    }

    fn history_path() -> PathBuf {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(HISTORY_FILE)
    }

    fn load_history() -> Vec<String> {
        let path = Self::history_path();
        if let Ok(file) = fs::File::open(&path) {
            let reader = BufReader::new(file);
            reader
                .lines()
                .map_while(Result::ok)
                .filter(|l| !l.is_empty())
                .collect()
        } else {
            Vec::new()
        }
    }

    fn save_history(&self) {
        let path = Self::history_path();
        if let Ok(mut file) = fs::File::create(&path) {
            let start = self.command_history.len().saturating_sub(MAX_HISTORY);
            for cmd in &self.command_history[start..] {
                let _ = writeln!(file, "{}", cmd);
            }
        }
    }

    pub fn is_input_mode(&self) -> bool {
        self.input_mode == InputMode::Command
    }

    pub async fn handle_key(&mut self, key: KeyEvent) -> Result<()> {
        match self.input_mode {
            InputMode::Normal => match key.code {
                KeyCode::Char(':') => {
                    self.input_mode = InputMode::Command;
                    self.input.clear();
                }
                KeyCode::Char('y') => {
                    if !self.messages.is_empty() {
                        self.input_mode = InputMode::Copy;
                        self.copy_index = 0;
                    }
                }
                KeyCode::Tab => {
                    self.tab = match self.tab {
                        Tab::Trade => Tab::Chart,
                        Tab::Chart => Tab::Orders,
                        Tab::Orders => Tab::Positions,
                        Tab::Positions => Tab::Autostops,
                        Tab::Autostops => Tab::Trade,
                    };
                    if self.tab == Tab::Chart && self.candles.is_empty() {
                        self.refresh_candles().await?;
                    }
                }
                KeyCode::BackTab => {
                    self.tab = match self.tab {
                        Tab::Trade => Tab::Autostops,
                        Tab::Chart => Tab::Trade,
                        Tab::Orders => Tab::Chart,
                        Tab::Positions => Tab::Orders,
                        Tab::Autostops => Tab::Positions,
                    };
                    if self.tab == Tab::Chart && self.candles.is_empty() {
                        self.refresh_candles().await?;
                    }
                }
                KeyCode::Char('r') => {
                    self.refresh().await?;
                }
                KeyCode::Left | KeyCode::Char('h') if self.tab == Tab::Chart => {
                    if self.chart_offset + 10 < self.candles.len() {
                        self.chart_offset += 10;
                    }
                }
                KeyCode::Right | KeyCode::Char('l') if self.tab == Tab::Chart => {
                    self.chart_offset = self.chart_offset.saturating_sub(10);
                }
                KeyCode::Char('+') | KeyCode::Char('=') if self.tab == Tab::Chart => {
                    if self.chart_zoom < 5 {
                        self.chart_zoom += 1;
                    }
                }
                KeyCode::Char('-') if self.tab == Tab::Chart => {
                    if self.chart_zoom > 1 {
                        self.chart_zoom -= 1;
                    }
                }
                KeyCode::Char('}') if self.tab == Tab::Chart => {
                    self.chart_zoom_v = (self.chart_zoom_v + 10).min(200);
                }
                KeyCode::Char('{') if self.tab == Tab::Chart => {
                    self.chart_zoom_v = self.chart_zoom_v.saturating_sub(10).max(1);
                }
                KeyCode::Char(']') if self.tab == Tab::Chart => {
                    if self.chart_zoom_v < 200 {
                        self.chart_zoom_v += 1;
                    }
                }
                KeyCode::Char('[') if self.tab == Tab::Chart => {
                    if self.chart_zoom_v > 1 {
                        self.chart_zoom_v -= 1;
                    }
                }
                KeyCode::Char('k') | KeyCode::Up if self.tab == Tab::Chart => {
                    self.chart_offset_v += 10;
                }
                KeyCode::Char('j') | KeyCode::Down if self.tab == Tab::Chart => {
                    self.chart_offset_v -= 10;
                }
                KeyCode::Char('0') if self.tab == Tab::Chart => {
                    self.chart_offset = 0;
                    self.chart_zoom = 2;
                    self.chart_zoom_v = 1;
                    self.chart_offset_v = 0;
                }
                KeyCode::Char('1') if self.tab == Tab::Chart => {
                    self.price_tracking = !self.price_tracking;
                }
                _ => {}
            },
            InputMode::Command => {
                let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
                let ctrl_or_alt = ctrl || key.modifiers.contains(KeyModifiers::ALT);

                match key.code {
                    KeyCode::Char('c') if ctrl => {
                        if !self.input.is_empty() {
                            self.command_history.push(self.input.clone());
                            self.save_history();
                        }
                        self.input_mode = InputMode::Normal;
                        self.input.clear();
                        self.input_cursor = 0;
                        self.history_index = None;
                    }
                    KeyCode::Esc => {
                        self.input_mode = InputMode::Normal;
                        self.input.clear();
                        self.input_cursor = 0;
                        self.history_index = None;
                    }
                    KeyCode::Enter => {
                        let cmd = self.input.clone();
                        self.input.clear();
                        self.input_cursor = 0;
                        self.input_mode = InputMode::Normal;
                        self.history_index = None;
                        if !cmd.is_empty() {
                            self.command_history.push(cmd.clone());
                            self.save_history();
                        }
                        self.execute_command(&cmd).await?;
                    }
                    KeyCode::Left => {
                        if ctrl_or_alt {
                            self.input_cursor = Self::find_word_start_left(&self.input, self.input_cursor);
                        } else if self.input_cursor > 0 {
                            self.input_cursor -= 1;
                        }
                    }
                    KeyCode::Right => {
                        if ctrl_or_alt {
                            self.input_cursor = Self::find_word_end_right(&self.input, self.input_cursor);
                        } else if self.input_cursor < self.input.len() {
                            self.input_cursor += 1;
                        }
                    }
                    KeyCode::Up => {
                        if !self.command_history.is_empty() {
                            let new_index = match self.history_index {
                                None => self.command_history.len() - 1,
                                Some(0) => 0,
                                Some(i) => i - 1,
                            };
                            self.history_index = Some(new_index);
                            self.input = self.command_history[new_index].clone();
                            self.input_cursor = self.input.len();
                        }
                    }
                    KeyCode::Down => {
                        if let Some(i) = self.history_index {
                            if i + 1 < self.command_history.len() {
                                self.history_index = Some(i + 1);
                                self.input = self.command_history[i + 1].clone();
                                self.input_cursor = self.input.len();
                            } else {
                                self.history_index = None;
                                self.input.clear();
                                self.input_cursor = 0;
                            }
                        }
                    }
                    KeyCode::Tab => {
                        self.tab = match self.tab {
                            Tab::Trade => Tab::Chart,
                            Tab::Chart => Tab::Orders,
                            Tab::Orders => Tab::Positions,
                            Tab::Positions => Tab::Autostops,
                            Tab::Autostops => Tab::Trade,
                        };
                        if self.tab == Tab::Chart && self.candles.is_empty() {
                            self.refresh_candles().await?;
                        }
                    }
                    KeyCode::BackTab => {
                        self.tab = match self.tab {
                            Tab::Trade => Tab::Autostops,
                            Tab::Chart => Tab::Trade,
                            Tab::Orders => Tab::Chart,
                            Tab::Positions => Tab::Orders,
                            Tab::Autostops => Tab::Positions,
                        };
                        if self.tab == Tab::Chart && self.candles.is_empty() {
                            self.refresh_candles().await?;
                        }
                    }
                    KeyCode::Char(c) => {
                        self.input.insert(self.input_cursor, c);
                        self.input_cursor += 1;
                    }
                    KeyCode::Backspace => {
                        if ctrl_or_alt {
                            let word_start = Self::find_word_start_left(&self.input, self.input_cursor);
                            self.input.drain(word_start..self.input_cursor);
                            self.input_cursor = word_start;
                        } else if self.input_cursor > 0 {
                            self.input_cursor -= 1;
                            self.input.remove(self.input_cursor);
                        }
                    }
                    KeyCode::Delete => {
                        if ctrl_or_alt {
                            let word_end = Self::find_word_end_right(&self.input, self.input_cursor);
                            self.input.drain(self.input_cursor..word_end);
                        } else if self.input_cursor < self.input.len() {
                            self.input.remove(self.input_cursor);
                        }
                    }
                    KeyCode::Home => {
                        self.input_cursor = 0;
                    }
                    KeyCode::End => {
                        self.input_cursor = self.input.len();
                    }
                    _ => {}
                }
            },
            InputMode::Copy => match key.code {
                KeyCode::Esc | KeyCode::Char('y') => {
                    self.input_mode = InputMode::Normal;
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    if self.copy_index + 1 < self.messages.len() {
                        self.copy_index += 1;
                    }
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    if self.copy_index > 0 {
                        self.copy_index -= 1;
                    }
                }
                KeyCode::Enter => {
                    let msg_idx = self.messages.len().saturating_sub(1 + self.copy_index);
                    if let Some(msg) = self.messages.get(msg_idx) {
                        if let Some(ref mut cb) = self.clipboard {
                            if cb.set_text(msg.clone()).is_ok() {
                                self.input_mode = InputMode::Normal;
                            }
                        }
                    }
                }
                _ => {}
            },
        }
        Ok(())
    }

    pub async fn handle_mouse(&mut self, mouse: MouseEvent) -> Result<()> {
        if self.tab != Tab::Chart {
            return Ok(());
        }

        match mouse.kind {
            MouseEventKind::Moved | MouseEventKind::Drag(_) => {
                self.mouse_position = Some((mouse.column, mouse.row));
            }
            MouseEventKind::Down(_) => {
                if let Some(chart_area) = self.chart_area {
                    if mouse.column >= chart_area.x
                        && mouse.column < chart_area.x + chart_area.width
                        && mouse.row >= chart_area.y
                        && mouse.row < chart_area.y + chart_area.height
                    {
                        if let Some(price) = self.get_price_at_mouse(mouse.column, mouse.row) {
                            let price_rounded = price.round_dp(10);
                            let price_str = price_rounded.to_string();

                            if self.input_mode == InputMode::Command {
                                self.input.insert_str(self.input_cursor, &price_str);
                                self.input_cursor += price_str.len();
                                self.messages.push(format!("Price inserted: {}", price_str));
                            } else {
                                if let Some(ref mut cb) = self.clipboard {
                                    if cb.set_text(price_str.clone()).is_ok() {
                                        self.messages.push(format!("Copied: {}", price_str));
                                    }
                                }
                            }
                        }
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn get_price_at_mouse(&self, col: u16, row: u16) -> Option<Decimal> {
        let chart_area = self.chart_area?;

        if col < chart_area.x || col >= chart_area.x + chart_area.width
            || row < chart_area.y || row >= chart_area.y + chart_area.height {
            return None;
        }

        if self.candles.is_empty() {
            return None;
        }

        let candle_width = self.chart_zoom as usize;
        let visible_count = (chart_area.width as usize).saturating_sub(2) / (candle_width + 1);
        let start_idx = self.candles.len().saturating_sub(visible_count + self.chart_offset);
        let end_idx = self.candles.len().saturating_sub(self.chart_offset);
        let visible_candles = &self.candles[start_idx..end_idx];

        if visible_candles.is_empty() {
            return None;
        }

        let (min_price, max_price) = visible_candles.iter().fold(
            (Decimal::MAX, Decimal::MIN),
            |(min, max), c| (min.min(c.low), max.max(c.high)),
        );

        let base_padding = (max_price - min_price) * Decimal::from_str_exact("0.05").unwrap_or(Decimal::ZERO);
        let zoom_factor = Decimal::from(self.chart_zoom_v);
        let range = max_price - min_price;
        let center = min_price + range / Decimal::from(2);
        let half_range = (range + base_padding * Decimal::from(2)) / zoom_factor / Decimal::from(2);

        let tick_size = if self.chart_zoom_v >= 150 {
            if max_price >= Decimal::from(1000) {
                Decimal::from_str_exact("0.01").unwrap()
            } else if max_price >= Decimal::from(100) {
                Decimal::from_str_exact("0.001").unwrap()
            } else if max_price >= Decimal::from(10) {
                Decimal::from_str_exact("0.0001").unwrap()
            } else if max_price >= Decimal::from(1) {
                Decimal::from_str_exact("0.00001").unwrap()
            } else {
                Decimal::from_str_exact("0.000001").unwrap()
            }
        } else if self.chart_zoom_v >= 100 {
            if max_price >= Decimal::from(1000) {
                Decimal::from_str_exact("0.05").unwrap()
            } else if max_price >= Decimal::from(100) {
                Decimal::from_str_exact("0.005").unwrap()
            } else if max_price >= Decimal::from(10) {
                Decimal::from_str_exact("0.0005").unwrap()
            } else {
                Decimal::from_str_exact("0.00005").unwrap()
            }
        } else if self.chart_zoom_v >= 50 {
            if max_price >= Decimal::from(1000) {
                Decimal::from_str_exact("0.1").unwrap()
            } else if max_price >= Decimal::from(100) {
                Decimal::from_str_exact("0.01").unwrap()
            } else if max_price >= Decimal::from(10) {
                Decimal::from_str_exact("0.001").unwrap()
            } else {
                Decimal::from_str_exact("0.0001").unwrap()
            }
        } else if self.chart_zoom_v >= 20 {
            if max_price >= Decimal::from(1000) {
                Decimal::from_str_exact("0.5").unwrap()
            } else if max_price >= Decimal::from(100) {
                Decimal::from_str_exact("0.05").unwrap()
            } else if max_price >= Decimal::from(10) {
                Decimal::from_str_exact("0.005").unwrap()
            } else {
                Decimal::from_str_exact("0.0005").unwrap()
            }
        } else {
            range / Decimal::from(100)
        };

        let v_offset = tick_size * Decimal::from(self.chart_offset_v);
        let adjusted_center = center + v_offset;

        let y_min = adjusted_center - half_range;
        let y_max = adjusted_center + half_range;

        let inner_height = chart_area.height.saturating_sub(2);
        let y_pos_in_chart = row.saturating_sub(chart_area.y + 1);

        if y_pos_in_chart >= inner_height {
            return None;
        }

        let y_ratio = Decimal::from(y_pos_in_chart) / Decimal::from(inner_height.saturating_sub(1).max(1));
        let price = y_max - (y_max - y_min) * y_ratio;

        Some(price)
    }

    async fn execute_command(&mut self, cmd: &str) -> Result<()> {
        if cmd.contains(';') {
            for sub_cmd in cmd.split(';') {
                let sub_cmd = sub_cmd.trim();
                if !sub_cmd.is_empty() {
                    self.execute_single_command(sub_cmd).await?;
                }
            }
            return Ok(());
        }

        if cmd.contains('|') {
            let parts: Vec<&str> = cmd.split('|').map(|s| s.trim()).collect();
            if parts.len() >= 2 {
                let first_cmd = parts[0];
                let first_parts: Vec<&str> = first_cmd.split_whitespace().collect();
                let side = if !first_parts.is_empty() {
                    match match_command(first_parts[0]) {
                        Some("buyrisk") | Some("buy") => Side::Buy,
                        Some("sellrisk") | Some("sell") => Side::Sell,
                        _ => Side::Buy,
                    }
                } else {
                    Side::Buy
                };
                self.pending_action = Some(PendingAction {
                    symbol: self.symbol.clone(),
                    side,
                    actions: parts[1..].iter().map(|s| s.to_string()).collect(),
                });
                self.execute_single_command(first_cmd).await?;
                return Ok(());
            }
        }

        self.execute_single_command(cmd).await
    }

    async fn execute_pending_action(&mut self, pending: &PendingAction) -> Result<()> {
        if pending.actions.is_empty() {
            return Ok(());
        }

        let first_action = &pending.actions[0];
        let parts: Vec<&str> = first_action.split_whitespace().collect();
        if parts.is_empty() {
            return Ok(());
        }

        if pending.actions.len() > 1 {
            self.pending_action = Some(PendingAction {
                symbol: pending.symbol.clone(),
                side: pending.side,
                actions: pending.actions[1..].to_vec(),
            });
        }

        match match_command(parts[0]) {
            Some("ts") => {
                if parts.len() >= 3 {
                    self.set_trailing_stop(&parts[1..], Some(pending.side)).await?;
                } else {
                    self.messages.push("Usage: ts <trigger> <callback>".to_string());
                }
            }
            Some("activityStop") => {
                if parts.len() >= 2 {
                    self.set_activity_stop(&parts[1..], Some(pending.side)).await?;
                } else {
                    self.messages.push("Usage: as <seconds> [trigger_price]".to_string());
                }
            }
            _ => {
                self.execute_single_command(first_action).await?;
            }
        }

        Ok(())
    }

    async fn create_pending_autostop_from_action(
        &mut self,
        order_id: &str,
        symbol: &Symbol,
        side: Side,
        pending: &PendingAction,
    ) -> Result<()> {
        for action in &pending.actions {
            let parts: Vec<&str> = action.split_whitespace().collect();
            if parts.is_empty() {
                continue;
            }

            match match_command(parts[0]) {
                Some("ts") => {
                    if parts.len() >= 3 {
                        let trigger = Decimal::from_str(parts[1])?;
                        let callback = Decimal::from_str(parts[2])?;

                        let config = PendingAutostopConfig {
                            symbol: symbol.clone(),
                            side,
                            autostop_type: AutostopType::Trailing,
                            params: AutostopParams {
                                trailing_stop_target: Some(TrailingStopTarget::Absolute(callback)),
                                active_price: Some(trigger),
                                timeout_secs: None,
                                trigger_price: None,
                            },
                        };

                        let msg = ClientMessage::new(ClientPayload::CreatePendingAutostop {
                            limit_order_id: order_id.to_string(),
                            config,
                        });
                        self.conn_tx.send(msg).await?;
                        self.messages.push(format!("Pending trailing stop created for order {}", order_id));
                    }
                }
                Some("activityStop") => {
                    if parts.len() >= 2 {
                        let timeout_secs = parts[1].parse::<u32>()?;
                        let trigger_price = if parts.len() >= 3 {
                            Some(Decimal::from_str(parts[2])?)
                        } else {
                            None
                        };

                        let config = PendingAutostopConfig {
                            symbol: symbol.clone(),
                            side,
                            autostop_type: AutostopType::Activity,
                            params: AutostopParams {
                                trailing_stop_target: None,
                                active_price: None,
                                timeout_secs: Some(timeout_secs),
                                trigger_price,
                            },
                        };

                        let msg = ClientMessage::new(ClientPayload::CreatePendingAutostop {
                            limit_order_id: order_id.to_string(),
                            config,
                        });
                        self.conn_tx.send(msg).await?;
                        self.messages.push(format!("Pending activity stop created for order {}", order_id));
                    }
                }
                _ => {}
            }
        }

        Ok(())
    }

    async fn execute_single_command(&mut self, cmd: &str) -> Result<()> {
        let parts: Vec<&str> = cmd.split_whitespace().collect();
        if parts.is_empty() {
            return Ok(());
        }

        let command = match_command(parts[0]);

        match command {
            Some("buy") => {
                if parts.len() >= 2 {
                    self.place_order(Side::Buy, &parts[1..]).await?;
                }
            }
            Some("sell") => {
                if parts.len() >= 2 {
                    self.place_order(Side::Sell, &parts[1..]).await?;
                }
            }
            Some("cancel") => {
                if parts.len() >= 2 {
                    self.cancel_order(parts[1]).await?;
                }
            }
            Some("cancelall") => {
                self.cancel_all_orders().await?;
            }
            Some("close") => {
                if parts.len() >= 2 {
                    self.close_position(Some(parts[1])).await?;
                } else {
                    self.close_position(None).await?;
                }
            }
            Some("symbol") => {
                if parts.len() >= 2 {
                    self.unsubscribe_chart().await?;
                    self.symbol = parts[1].to_uppercase();
                    self.last_price = None;
                    self.candles.clear();
                    self.trades.clear();
                    self.chart_offset = 0;
                    self.chart_offset_v = 0;
                    self.messages.push(format!("Symbol: {}", self.symbol));
                    self.subscribe_chart().await?;
                }
            }
            Some("positions") => {
                self.tab = Tab::Positions;
                self.refresh_positions().await?;
            }
            Some("orders") => {
                self.tab = Tab::Orders;
                self.refresh_orders().await?;
            }
            Some("buyrisk") => {
                if parts.len() >= 3 {
                    self.place_risk_order(Side::Buy, &parts[1..]).await?;
                } else {
                    self.messages.push("Usage: buyrisk <risk_usdt> <sl_price> [limit_price]".to_string());
                }
            }
            Some("sellrisk") => {
                if parts.len() >= 3 {
                    self.place_risk_order(Side::Sell, &parts[1..]).await?;
                } else {
                    self.messages.push("Usage: sellrisk <risk_usdt> <sl_price> [limit_price]".to_string());
                }
            }
            Some("help") => {
                self.show_help();
            }
            Some("ts") => {
                if parts.len() >= 3 {
                    self.set_trailing_stop(&parts[1..], None).await?;
                } else {
                    self.messages.push("Usage: ts <trigger> <callback> (use % for percent)".to_string());
                }
            }
            Some("chart") => {
                self.tab = Tab::Chart;
                self.messages.push(format!("Chart: {} candles loaded", self.candles.len()));
                if self.candles.is_empty() {
                    self.messages.push("Requesting candles...".to_string());
                    self.refresh_candles().await?;
                }
                self.subscribe_chart().await?;
            }
            Some("tf") => {
                if parts.len() >= 2 {
                    let interval = parts[1];
                    if ["1", "3", "5", "15", "30", "60", "120", "240", "D", "W"].contains(&interval) {
                        self.chart_interval = interval.to_string();
                        self.chart_offset = 0;
                        self.refresh_candles().await?;
                        self.subscribe_chart().await?;
                        self.messages.push(format!("Timeframe: {}", interval));
                    } else {
                        self.messages.push("Valid: 1, 3, 5, 15, 30, 60, 120, 240, D, W".to_string());
                    }
                } else {
                    self.messages.push(format!("Current tf: {}. Usage: tf <1|5|15|30|60|240>", self.chart_interval));
                }
            }
            Some("level") => {
                if parts.len() >= 2 {
                    if let Ok(price) = Decimal::from_str(parts[1]) {
                        self.chart_levels.push(price);
                        self.messages.push(format!("Level added: {}", price));
                    } else {
                        self.messages.push("Invalid price".to_string());
                    }
                } else {
                    self.messages.push(format!("Levels: {:?}", self.chart_levels));
                }
            }
            Some("clevel") => {
                if parts.len() >= 2 {
                    if let Ok(price) = Decimal::from_str(parts[1]) {
                        self.chart_levels.retain(|&l| l != price);
                        self.messages.push(format!("Level removed: {}", price));
                    }
                } else {
                    self.chart_levels.clear();
                    self.messages.push("All levels cleared".to_string());
                }
            }
            Some("sound") => {
                self.sound_enabled = !self.sound_enabled;
                self.messages.push(format!("Sound: {}", if self.sound_enabled { "ON" } else { "OFF" }));
            }
            Some("activityStop") => {
                if parts.len() >= 2 {
                    self.set_activity_stop(&parts[1..], None).await?;
                } else {
                    self.messages.push("Usage: as <seconds> [trigger_price]".to_string());
                }
            }
            _ => {
                self.messages.push(format!("Unknown command: {}", parts[0]));
            }
        }

        Ok(())
    }

    async fn place_order(&mut self, side: Side, args: &[&str]) -> Result<()> {
        let quantity = Decimal::from_str(args[0]).unwrap_or(Decimal::ZERO);
        let price = args.get(1).and_then(|p| Decimal::from_str(p).ok());

        let order_type = if price.is_some() {
            OrderType::Limit
        } else {
            OrderType::Market
        };

        let req = OrderRequest {
            symbol: Symbol::new(&self.symbol),
            side,
            order_type,
            quantity,
            price,
            time_in_force: TimeInForce::Gtc,
            reduce_only: false,
            take_profit: None,
            stop_loss: None,
            position_idx: None,
        };

        let msg = ClientMessage::new(ClientPayload::PlaceOrder(req));
        self.conn_tx.send(msg).await?;
        self.messages.push(format!(
            "Placing {} {} {} @ {:?}",
            if side == Side::Buy { "BUY" } else { "SELL" },
            quantity,
            self.symbol,
            price
        ));

        Ok(())
    }

    async fn place_risk_order(&mut self, side: Side, args: &[&str]) -> Result<()> {
        let risk_usdt = match Decimal::from_str(args[0]) {
            Ok(v) => v,
            Err(_) => {
                self.messages.push("Invalid risk amount".to_string());
                return Ok(());
            }
        };

        let sl_arg = args[1];
        let sl_is_percent = sl_arg.ends_with('%');

        let limit_price = args.get(2).and_then(|p| {
            if p == &"-" { None } else { Decimal::from_str(p).ok() }
        });

        let tp_value = args.get(3).and_then(|p| parse_value(p));

        if sl_is_percent {
            let sl_percent = match Decimal::from_str(sl_arg.trim_end_matches('%')) {
                Ok(v) => v,
                Err(_) => {
                    self.messages.push("Invalid SL percent".to_string());
                    return Ok(());
                }
            };

            if let Some(entry_price) = limit_price {
                let sl_price = match side {
                    Side::Buy => entry_price * (Decimal::ONE - sl_percent / Decimal::from(100)),
                    Side::Sell => entry_price * (Decimal::ONE + sl_percent / Decimal::from(100)),
                };
                self.execute_risk_order(side, risk_usdt, sl_price, Some(entry_price), tp_value).await?;
            } else {
                self.pending_risk_order = Some(PendingRiskOrder {
                    side,
                    risk_usdt,
                    sl_price: Decimal::ZERO,
                    sl_percent: Some(sl_percent),
                    symbol: self.symbol.clone(),
                    tp_value,
                });
                self.messages.push("Fetching price...".to_string());
                self.refresh_ticker().await?;
            }
        } else {
            let sl_price = match Decimal::from_str(sl_arg) {
                Ok(v) => v,
                Err(_) => {
                    self.messages.push("Invalid SL price".to_string());
                    return Ok(());
                }
            };

            if let Some(entry_price) = limit_price {
                self.execute_risk_order(side, risk_usdt, sl_price, Some(entry_price), tp_value).await?;
            } else {
                self.pending_risk_order = Some(PendingRiskOrder {
                    side,
                    risk_usdt,
                    sl_price,
                    sl_percent: None,
                    symbol: self.symbol.clone(),
                    tp_value,
                });
                self.messages.push("Fetching price...".to_string());
                self.refresh_ticker().await?;
            }
        }

        Ok(())
    }

    async fn execute_risk_order(
        &mut self,
        side: Side,
        risk_usdt: Decimal,
        sl_price: Decimal,
        limit_price: Option<Decimal>,
        tp_value: Option<Value>,
    ) -> Result<()> {
        let is_limit = limit_price.is_some();
        let entry_price = limit_price.unwrap_or_else(|| self.last_price.unwrap_or_default());

        if entry_price.is_zero() {
            self.messages.push("No price available".to_string());
            return Ok(());
        }

        let calc = match calculate_position_size(risk_usdt, entry_price, sl_price, side, is_limit) {
            Ok(c) => c,
            Err(e) => {
                let msg = match e {
                    RiskError::InvalidRisk => "Invalid risk amount",
                    RiskError::InvalidSlPrice => "Invalid SL price",
                    RiskError::InvalidEntryPrice => "Invalid entry price",
                    RiskError::SlInvalidForSide => match side {
                        Side::Buy => "SL must be below entry price for long",
                        Side::Sell => "SL must be above entry price for short",
                    },
                    RiskError::RiskPerUnitTooSmall => "Risk per unit too small",
                };
                self.messages.push(msg.to_string());
                return Ok(());
            }
        };

        let quantity = round_quantity(calc.quantity, 0);
        let order_type = if is_limit { OrderType::Limit } else { OrderType::Market };

        let take_profit = tp_value.map(|val| {
            match val {
                Value::Percent(pct) => {
                    let multiplier = pct / Decimal::from(100);
                    match side {
                        Side::Buy => entry_price * (Decimal::ONE + multiplier),
                        Side::Sell => entry_price * (Decimal::ONE - multiplier),
                    }
                }
                Value::Absolute(price) => price,
                Value::Ratio(numerator, denominator) => {
                    let sl_distance = (entry_price - sl_price).abs();
                    let multiplier = Decimal::from(numerator) / Decimal::from(denominator);
                    let tp_distance = sl_distance * multiplier;
                    match side {
                        Side::Buy => entry_price + tp_distance,
                        Side::Sell => entry_price - tp_distance,
                    }
                }
            }
        });

        let req = OrderRequest {
            symbol: Symbol::new(&self.symbol),
            side,
            order_type,
            quantity,
            price: limit_price,
            time_in_force: if is_limit { TimeInForce::Gtc } else { TimeInForce::Ioc },
            reduce_only: false,
            take_profit,
            stop_loss: Some(sl_price),
            position_idx: None,
        };

        let msg = ClientMessage::new(ClientPayload::PlaceOrder(req));
        self.conn_tx.send(msg).await?;

        let tp_str = take_profit.map(|tp| format!(", TP: {:.2}", tp)).unwrap_or_default();
        self.messages.push(format!(
            "Risk order: {} {} {} @ {} (SL: {}{}, risk: ${}, qty: {})",
            if side == Side::Buy { "LONG" } else { "SHORT" },
            self.symbol,
            if is_limit { "LIMIT" } else { "MARKET" },
            if is_limit { entry_price.to_string() } else { format!("~{}", entry_price) },
            sl_price,
            tp_str,
            risk_usdt,
            quantity
        ));

        Ok(())
    }

    async fn cancel_order(&mut self, order_id: &str) -> Result<()> {
        let msg = ClientMessage::new(ClientPayload::CancelOrder {
            order_id: order_id.to_string(),
        });
        self.conn_tx.send(msg).await?;
        self.messages.push(format!("Cancelling order: {}", order_id));
        Ok(())
    }

    async fn cancel_all_orders(&mut self) -> Result<()> {
        let msg = ClientMessage::new(ClientPayload::CancelAllOrders {
            symbol: Some(Symbol::new(&self.symbol)),
        });
        self.conn_tx.send(msg).await?;
        self.messages.push("Cancelling all orders".to_string());
        Ok(())
    }

    async fn close_position(&mut self, side_str: Option<&str>) -> Result<()> {
        let side = if let Some(s) = side_str {
            match s.to_lowercase().as_str() {
                "l" | "long" | "buy" => Side::Buy,
                "s" | "short" | "sell" => Side::Sell,
                _ => {
                    self.messages.push("Invalid side. Use 'l' or 's'".to_string());
                    return Ok(());
                }
            }
        } else {
            let current_positions: Vec<_> = self.positions.iter()
                .filter(|p| p.symbol.0 == self.symbol)
                .collect();

            match current_positions.len() {
                0 => {
                    self.messages.push("No positions to close".to_string());
                    return Ok(());
                }
                1 => current_positions[0].side,
                _ => {
                    self.messages.push("Multiple positions open. Specify side: cl <l|s>".to_string());
                    return Ok(());
                }
            }
        };

        let msg = ClientMessage::new(ClientPayload::ClosePosition(ClosePositionRequest {
            symbol: Symbol::new(&self.symbol),
            side,
        }));
        self.conn_tx.send(msg).await?;
        self.messages.push(format!(
            "Closing {} position for {}",
            if side == Side::Buy { "LONG" } else { "SHORT" },
            self.symbol
        ));
        Ok(())
    }

    async fn set_trailing_stop(&mut self, args: &[&str], side: Option<Side>) -> Result<()> {
        let trigger = match parse_value(args[0]) {
            Some(v) => v,
            None => {
                self.messages.push("Invalid trigger value".to_string());
                return Ok(());
            }
        };

        let callback = match parse_value(args[1]) {
            Some(v) => v,
            None => {
                self.messages.push("Invalid callback value".to_string());
                return Ok(());
            }
        };

        let entry_price = match self.last_price {
            Some(p) => p,
            None => {
                self.messages.push("No price available".to_string());
                return Ok(());
            }
        };

        let position_side = match side {
            Some(s) => s,
            None => {
                let pos = self.positions.iter().find(|p| p.symbol.0 == self.symbol);
                match pos {
                    Some(p) => p.side,
                    None => {
                        self.messages.push("No position found for TS".to_string());
                        return Ok(());
                    }
                }
            }
        };

        let active_price = match trigger {
            Value::Percent(pct) => match position_side {
                Side::Buy => entry_price * (Decimal::ONE + pct / Decimal::from(100)),
                Side::Sell => entry_price * (Decimal::ONE - pct / Decimal::from(100)),
            },
            Value::Absolute(price) => price,
            Value::Ratio(numerator, denominator) => {
                let pos = self.positions.iter().find(|p| p.symbol.0 == self.symbol && p.side == position_side);
                match pos {
                    Some(p) => {
                        if let Some(sl) = p.stop_loss {
                            let sl_distance = (sl - p.entry_price).abs();
                            let multiplier = Decimal::from(denominator) / Decimal::from(numerator);
                            let trigger_distance = sl_distance * multiplier;
                            match position_side {
                                Side::Buy => p.entry_price + trigger_distance,
                                Side::Sell => p.entry_price - trigger_distance,
                            }
                        } else {
                            self.messages.push("No SL set, cannot calculate ratio trigger".to_string());
                            return Ok(());
                        }
                    }
                    None => {
                        self.messages.push("Position not found for ratio trigger".to_string());
                        return Ok(());
                    }
                }
            }
        };

        let target = match callback {
            Value::Percent(pct) => TrailingStopTarget::Percentage(pct),
            Value::Absolute(val) => TrailingStopTarget::Absolute(val),
            Value::Ratio(_, _) => {
                self.messages.push("Ratio not supported for callback, use % or absolute".to_string());
                return Ok(());
            }
        };

        let req = TrailingStopRequest {
            symbol: Symbol::new(&self.symbol),
            side: position_side,
            target,
            active_price: Some(active_price),
        };

        let msg = ClientMessage::new(ClientPayload::SetTrailingStop(req));
        self.conn_tx.send(msg).await?;

        let target_display = match target {
            TrailingStopTarget::Absolute(val) => format!("{}", val),
            TrailingStopTarget::Percentage(pct) => format!("{}%", pct),
            TrailingStopTarget::Ratio { numerator, denominator } => format!("{}/{}", numerator, denominator),
        };

        self.messages.push(format!(
            "Setting TS: {} trigger@{}, callback {}",
            if position_side == Side::Buy { "LONG" } else { "SHORT" },
            active_price, target_display
        ));

        Ok(())
    }

    async fn set_activity_stop(&mut self, args: &[&str], side: Option<Side>) -> Result<()> {
        let timeout_secs: u32 = match args[0].parse() {
            Ok(v) => v,
            Err(_) => {
                self.messages.push("Invalid timeout value".to_string());
                return Ok(());
            }
        };

        let trigger_price = if args.len() >= 2 {
            match Decimal::from_str(args[1]) {
                Ok(v) => Some(v),
                Err(_) => {
                    self.messages.push("Invalid trigger price".to_string());
                    return Ok(());
                }
            }
        } else {
            None
        };

        let position_side = match side {
            Some(s) => s,
            None => {
                let pos = self.positions.iter().find(|p| p.symbol.0 == self.symbol);
                match pos {
                    Some(p) => p.side,
                    None => {
                        self.messages.push("No position found for activity stop".to_string());
                        return Ok(());
                    }
                }
            }
        };

        let req = ActivityStopRequest {
            symbol: Symbol::new(&self.symbol),
            side: position_side,
            timeout_secs,
            trigger_price,
        };

        let msg = ClientMessage::new(ClientPayload::ActivityStop(req));
        self.conn_tx.send(msg).await?;

        let trigger_str = trigger_price
            .map(|p| format!(" trigger@{}", p))
            .unwrap_or_default();
        self.messages.push(format!(
            "Setting activity stop: {} {}s{}",
            if position_side == Side::Buy { "LONG" } else { "SHORT" },
            timeout_secs,
            trigger_str
        ));

        Ok(())
    }

    async fn refresh(&mut self) -> Result<()> {
        self.refresh_orders().await?;
        self.refresh_positions().await?;
        Ok(())
    }

    pub async fn auto_refresh(&mut self) -> Result<()> {
        if self.connected {
            self.refresh_orders().await?;
            self.refresh_positions().await?;
        }
        Ok(())
    }

    async fn refresh_ticker(&mut self) -> Result<()> {
        let msg = ClientMessage::new(ClientPayload::GetTicker {
            symbol: Symbol::new(&self.symbol),
        });
        self.conn_tx.send(msg).await?;
        Ok(())
    }

    async fn refresh_orders(&mut self) -> Result<()> {
        let msg = ClientMessage::new(ClientPayload::GetOrders { symbol: None });
        self.conn_tx.send(msg).await?;
        Ok(())
    }

    async fn refresh_positions(&mut self) -> Result<()> {
        let msg = ClientMessage::new(ClientPayload::GetPositions);
        self.conn_tx.send(msg).await?;
        Ok(())
    }

    async fn refresh_candles(&mut self) -> Result<()> {
        let msg = ClientMessage::new(ClientPayload::GetCandles {
            symbol: Symbol::new(&self.symbol),
            interval: self.chart_interval.clone(),
            limit: 200,
        });
        self.conn_tx.send(msg).await?;
        Ok(())
    }

    async fn subscribe_chart(&mut self) -> Result<()> {
        let msg = ClientMessage::new(ClientPayload::SubscribeChart {
            symbol: Symbol::new(&self.symbol),
            interval: self.chart_interval.clone(),
        });
        self.conn_tx.send(msg).await?;
        Ok(())
    }

    async fn unsubscribe_chart(&mut self) -> Result<()> {
        let msg = ClientMessage::new(ClientPayload::UnsubscribeChart);
        self.conn_tx.send(msg).await?;
        Ok(())
    }

    fn show_help(&mut self) {
        self.messages.push("Commands:".to_string());
        self.messages.push("  buy <qty> [price]                    - Place buy order (market/limit)".to_string());
        self.messages.push("  sell <qty> [price]                   - Place sell order (market/limit)".to_string());
        self.messages.push("  buyrisk <risk$> <sl|sl%> [lim] [tp]  - Long with risk calc".to_string());
        self.messages.push("  sellrisk <risk$> <sl|sl%> [lim] [tp] - Short with risk calc".to_string());
        self.messages.push("    tp can be: 2% (percent), 1/3 (ratio), or absolute price".to_string());
        self.messages.push("  cancel <id>                    - Cancel order".to_string());
        self.messages.push("  cancelall                      - Cancel all orders".to_string());
        self.messages.push("  close [l|s]                    - Close position (auto if one)".to_string());
        self.messages.push("  symbol <sym>                   - Set symbol".to_string());
        self.messages.push("  ts <trigger> <callback>        - Set trailing stop (% or abs)".to_string());
        self.messages.push("  as <secs> [trigger]            - Activity stop (auto-close after N sec)".to_string());
        self.messages.push("  chart                          - Open chart view".to_string());
        self.messages.push("  tf <1|5|15|30|60|240|D|W>      - Set timeframe".to_string());
        self.messages.push("  level <price>                  - Add price level".to_string());
        self.messages.push("  clevel [price]                 - Clear level(s)".to_string());
        self.messages.push("  sound                          - Toggle trade sounds".to_string());
        self.messages.push("".to_string());
        self.messages.push("Pipe operator (smart autostops):".to_string());
        self.messages.push("  Market orders: br 1 0.3% | as 5         - Autostop activates on position open".to_string());
        self.messages.push("  Limit orders:  sr 1 95000 96000 | ts 0.15 0.005 - Creates pending autostop".to_string());
        self.messages.push("                                            (activates when order fills)".to_string());
        self.messages.push("  Chain commands: cmd1 ; cmd2        - Run sequentially".to_string());
        self.messages.push("".to_string());
        self.messages.push("Tabs: Orders | Positions | Trade | Chart | Autostops (navigate with Tab/Shift+Tab)".to_string());
        self.messages.push("  Autostops tab shows pending autostops (waiting for limit orders to fill)".to_string());
        self.messages.push("".to_string());
        self.messages.push("Chart keys: h/l/←/→=scroll, k/j/↑/↓=vertical, +/-=zoom, [/]=vzoom, 0=reset".to_string());
        self.messages.push("Keys: Tab=autocomplete, r=refresh, :=command, q=quit".to_string());
    }

    fn autocomplete(&mut self) {
        let input = self.input.trim();
        if input.is_empty() {
            return;
        }

        let all_names = all_command_names();
        let matches: Vec<&str> = all_names
            .iter()
            .filter(|cmd| cmd.starts_with(input))
            .copied()
            .collect();

        if matches.len() == 1 {
            self.input = format!("{} ", matches[0]);
            self.input_cursor = self.input.len();
        } else if matches.len() > 1 {
            let common = Self::common_prefix(&matches);
            if common.len() > input.len() {
                self.input = common;
                self.input_cursor = self.input.len();
            }
        }
    }

    fn common_prefix(strings: &[&str]) -> String {
        if strings.is_empty() {
            return String::new();
        }
        let first = strings[0];
        let mut prefix_len = first.len();
        for s in &strings[1..] {
            prefix_len = first
                .chars()
                .zip(s.chars())
                .take_while(|(a, b)| a == b)
                .count()
                .min(prefix_len);
        }
        first[..prefix_len].to_string()
    }

    pub async fn process_messages(&mut self) -> Result<bool> {
        let mut had_messages = false;
        while let Ok(msg) = self.server_rx.try_recv() {
            had_messages = true;
            match msg.payload {
                ServerPayload::Connected => {
                    self.connected = true;
                    self.messages.push("Connected to server".to_string());
                    self.refresh().await?;
                    self.subscribe_chart().await?;
                }
                ServerPayload::Disconnected => {
                    self.connected = false;
                    self.messages.push("Disconnected from server".to_string());
                }
                ServerPayload::OrderPlaced(order) => {
                    self.messages.push(format!("Order placed: {}", order.id));
                    self.last_placed_order_id = Some(order.id.clone());

                    if order.order_type == OrderType::Limit {
                        if let Some(pending) = self.pending_action.take() {
                            if pending.symbol == self.symbol {
                                self.messages.push(format!("Limit order placed, processing autostop: {}", pending.actions.join(" | ")));
                                if let Err(e) = self.create_pending_autostop_from_action(&order.id, &order.symbol, order.side, &pending).await {
                                    self.messages.push(format!("Error creating pending autostop: {}", e));
                                }
                            }
                        }
                    }

                    self.refresh_orders().await?;
                }
                ServerPayload::OrderCancelled { order_id } => {
                    self.messages.push(format!("Order cancelled: {}", order_id));
                    self.refresh_orders().await?;
                }
                ServerPayload::OrderUpdate(order) => {
                    self.messages.push(format!("Order update: {} - {:?}", order.id, order.status));
                }
                ServerPayload::Orders(orders) => {
                    self.orders = orders;
                }
                ServerPayload::Positions(positions) => {
                    let had_position = self.positions.iter().any(|p| p.symbol.0 == self.symbol);
                    let has_position = positions.iter().any(|p| p.symbol.0 == self.symbol);
                    self.positions = positions;

                    if !had_position && has_position {
                        if let Some(pending) = self.pending_action.take() {
                            if pending.symbol == self.symbol {
                                self.messages.push(format!("Position opened, executing: {}", pending.actions.join(" | ")));
                                if let Err(e) = self.execute_pending_action(&pending).await {
                                    self.messages.push(format!("Pending action error: {}", e));
                                }
                            }
                        }
                    } else if had_position && !has_position
                        && (self.activity_stop_active || self.activity_stop_remaining_ms.is_some()) {
                            let msg = ClientMessage::new(ClientPayload::CancelActivityStop {
                                symbol: Symbol::new(&self.symbol),
                            });
                            let _ = self.conn_tx.send(msg).await;
                            self.activity_stop_active = false;
                            self.activity_stop_remaining_ms = None;
                        }
                }
                ServerPayload::TrailingStopSet { symbol } => {
                    self.messages.push(format!("Trailing stop set for {}", symbol.0));

                    if let Some(pending) = self.pending_action.take() {
                        if pending.symbol == symbol.0 {
                            self.messages.push(format!("Executing next action: {}", pending.actions.join(" | ")));
                            if let Err(e) = self.execute_pending_action(&pending).await {
                                self.messages.push(format!("Pending action error: {}", e));
                            }
                        }
                    }
                }
                ServerPayload::ActivityStopActivated { symbol, timeout_secs, trigger_price } => {
                    self.messages.push(format!("Activity stop activated for {} ({}s)", symbol.0, timeout_secs));
                    self.activity_stop_active = true;
                    self.activity_stop_remaining_ms = Some((timeout_secs as u64) * 1000);
                    self.activity_stop_trigger_price = trigger_price;

                    if let Some(pending) = self.pending_action.take() {
                        if pending.symbol == symbol.0 {
                            self.messages.push(format!("Executing next action: {}", pending.actions.join(" | ")));
                            if let Err(e) = self.execute_pending_action(&pending).await {
                                self.messages.push(format!("Pending action error: {}", e));
                            }
                        }
                    }
                }
                ServerPayload::ActivityStopStatus { symbol: _, remaining_ms, active, trigger_price } => {
                    self.activity_stop_active = active;
                    self.activity_stop_remaining_ms = if active { Some(remaining_ms) } else { None };
                    self.activity_stop_trigger_price = if active { trigger_price } else { None };
                }
                ServerPayload::ActivityStopTriggered { symbol } => {
                    self.messages.push(format!("Activity stop triggered for {} - market close executed", symbol.0));
                    self.activity_stop_active = false;
                    self.activity_stop_remaining_ms = None;
                    self.activity_stop_trigger_price = None;
                }
                ServerPayload::OrderError { message } => {
                    self.messages.push(format!("Order error: {}", message));
                }
                ServerPayload::PositionModeChanged { from_mode, to_mode } => {
                    self.messages.push(format!("Position mode switched: {} → {}", from_mode, to_mode));
                }
                ServerPayload::Error { code, message } => {
                    self.messages.push(format!("Error {}: {}", code, message));
                }
                ServerPayload::TickerUpdate(ticker) => {
                    if ticker.symbol.0 == self.symbol {
                        self.last_price = Some(ticker.last_price);
                    }
                    if let Some(pending) = self.pending_risk_order.take() {
                        if ticker.symbol.0 == pending.symbol {
                            self.last_price = Some(ticker.last_price);
                            let sl_price = if let Some(sl_pct) = pending.sl_percent {
                                match pending.side {
                                    Side::Buy => ticker.last_price * (Decimal::ONE - sl_pct / Decimal::from(100)),
                                    Side::Sell => ticker.last_price * (Decimal::ONE + sl_pct / Decimal::from(100)),
                                }
                            } else {
                                pending.sl_price
                            };
                            if let Err(e) = self.execute_risk_order(
                                pending.side,
                                pending.risk_usdt,
                                sl_price,
                                None,
                                pending.tp_value,
                            ).await {
                                self.messages.push(format!("Order error: {}", e));
                            }
                        } else {
                            self.pending_risk_order = Some(pending);
                        }
                    }
                }
                ServerPayload::Candles(candles) => {
                    self.messages.push(format!("Received {} candles", candles.len()));
                    self.candles = candles;
                    self.chart_offset = 0;
                }
                ServerPayload::CandleUpdate(candle) => {
                    if let Some(last) = self.candles.last_mut() {
                        if last.timestamp == candle.timestamp {
                            *last = candle;
                        } else {
                            self.candles.push(candle);
                            if self.candles.len() > 500 {
                                self.candles.remove(0);
                            }
                        }
                    }
                }
                ServerPayload::TradeUpdate(trade) => {
                    let is_buy = trade.side == Side::Buy;
                    self.last_price = Some(trade.price);
                    self.trades.push_front(trade);
                    if self.trades.len() > 100 {
                        self.trades.pop_back();
                    }

                    if self.sound_enabled && self.tab == Tab::Chart {
                        if let Some(ref player) = self.audio_player {
                            player.tick(is_buy);
                        }
                    }
                }
                ServerPayload::Pong => {}
                ServerPayload::ChartSubscribed => {}
                ServerPayload::AccountInfo(_) => {}
                ServerPayload::PendingAutostopCreated { limit_order_id } => {
                    self.messages.push(format!("Pending autostop created for order {}", limit_order_id));
                }
                ServerPayload::PendingAutostopUpdated { limit_order_id } => {
                    self.messages.push(format!("Pending autostop updated for order {}", limit_order_id));
                }
                ServerPayload::PendingAutostopCancelled { limit_order_id } => {
                    self.messages.push(format!("Pending autostop cancelled for order {}", limit_order_id));
                }
                ServerPayload::PendingAutostopActivated { limit_order_id, symbol } => {
                    self.messages.push(format!("Pending autostop activated for order {} ({})", limit_order_id, symbol.0));
                }
                ServerPayload::PendingAutostops(pending_autostops) => {
                    self.pending_autostops = pending_autostops;
                }
            }
        }

        if self.messages.len() > 100 {
            self.messages.drain(0..50);
        }

        Ok(had_messages)
    }
}
