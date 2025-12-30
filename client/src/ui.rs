use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{
        canvas::{Canvas, Line as CanvasLine, Rectangle},
        Block, Borders, Cell, List, ListItem, Paragraph, Row, Table, Tabs,
    },
    Frame,
};
use rust_decimal::Decimal;
use trade_shared::Side;

use crate::app::{App, InputMode, Tab};

pub fn draw(f: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(10),
            Constraint::Length(10),
            Constraint::Length(3),
        ])
        .split(f.area());

    draw_header(f, app, chunks[0]);
    draw_main(f, app, chunks[1]);
    draw_messages(f, app, chunks[2]);
    draw_input(f, app, chunks[3]);
}

fn draw_header(f: &mut Frame, app: &App, area: Rect) {
    let titles = vec!["Orders", "Positions", "Trade", "Chart"];
    let selected = match app.tab {
        Tab::Orders => 0,
        Tab::Positions => 1,
        Tab::Trade => 2,
        Tab::Chart => 3,
    };

    let status = if app.connected { "●" } else { "○" };
    let status_color = if app.connected { Color::Green } else { Color::Red };

    let price_str = app
        .last_price
        .map(|p| format!(" | {} {}", app.symbol, p))
        .unwrap_or_default();

    let header = Tabs::new(titles)
        .select(selected)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(Span::styled(
                    format!(" {} TradeTerminal{} ", status, price_str),
                    Style::default().fg(status_color),
                )),
        )
        .highlight_style(Style::default().add_modifier(Modifier::BOLD));

    f.render_widget(header, area);
}

fn draw_main(f: &mut Frame, app: &App, area: Rect) {
    match app.tab {
        Tab::Orders => draw_orders(f, app, area),
        Tab::Positions => draw_positions(f, app, area),
        Tab::Trade => draw_trade(f, app, area),
        Tab::Chart => draw_chart(f, app, area),
    }
}

fn draw_orders(f: &mut Frame, app: &App, area: Rect) {
    let header = Row::new(vec!["ID", "Symbol", "Side", "Type", "Qty", "Price", "Filled", "Status"])
        .style(Style::default().add_modifier(Modifier::BOLD))
        .height(1);

    let rows: Vec<Row> = app
        .orders
        .iter()
        .map(|o| {
            let side_color = if o.side == Side::Buy {
                Color::Green
            } else {
                Color::Red
            };
            Row::new(vec![
                Cell::from(o.id.chars().take(8).collect::<String>()),
                Cell::from(o.symbol.0.clone()),
                Cell::from(format!("{:?}", o.side)).style(Style::default().fg(side_color)),
                Cell::from(format!("{:?}", o.order_type)),
                Cell::from(o.quantity.to_string()),
                Cell::from(o.price.map(|p| p.to_string()).unwrap_or_default()),
                Cell::from(o.filled_quantity.to_string()),
                Cell::from(format!("{:?}", o.status)),
            ])
        })
        .collect();

    let table = Table::new(
        rows,
        [
            Constraint::Length(10),
            Constraint::Length(12),
            Constraint::Length(6),
            Constraint::Length(8),
            Constraint::Length(12),
            Constraint::Length(12),
            Constraint::Length(12),
            Constraint::Length(12),
        ],
    )
    .header(header)
    .block(Block::default().borders(Borders::ALL).title(" Orders "));

    f.render_widget(table, area);
}

fn draw_positions(f: &mut Frame, app: &App, area: Rect) {
    let header = Row::new(vec!["Symbol", "Side", "Qty", "Entry", "uPnL", "Leverage"])
        .style(Style::default().add_modifier(Modifier::BOLD))
        .height(1);

    let rows: Vec<Row> = app
        .positions
        .iter()
        .map(|p| {
            let side_color = if p.side == Side::Buy {
                Color::Green
            } else {
                Color::Red
            };
            let pnl_color = if p.unrealized_pnl >= rust_decimal::Decimal::ZERO {
                Color::Green
            } else {
                Color::Red
            };
            Row::new(vec![
                Cell::from(p.symbol.0.clone()),
                Cell::from(format!("{:?}", p.side)).style(Style::default().fg(side_color)),
                Cell::from(p.quantity.to_string()),
                Cell::from(p.entry_price.to_string()),
                Cell::from(p.unrealized_pnl.to_string()).style(Style::default().fg(pnl_color)),
                Cell::from(format!("{}x", p.leverage)),
            ])
        })
        .collect();

    let table = Table::new(
        rows,
        [
            Constraint::Length(12),
            Constraint::Length(8),
            Constraint::Length(14),
            Constraint::Length(14),
            Constraint::Length(14),
            Constraint::Length(10),
        ],
    )
    .header(header)
    .block(Block::default().borders(Borders::ALL).title(" Positions "));

    f.render_widget(table, area);
}

fn draw_trade(f: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
        .split(area);

    let help_text = vec![
        Line::from(vec![
            Span::styled(" Symbol: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(&app.symbol),
        ]),
        Line::from(""),
        Line::from(Span::styled(" Commands", Style::default().add_modifier(Modifier::BOLD))),
        Line::from("  :buy <qty> [price]                :sell <qty> [price]"),
        Line::from("  :buyrisk <risk$> <sl> [lim] [tp%] :sellrisk ..."),
        Line::from("  :ts <trigger> <callback>          trailing stop (% or abs)"),
        Line::from("  :cancel <id>   :cancelall         :symbol <sym>"),
        Line::from(""),
        Line::from(Span::styled(" Shortcuts", Style::default().add_modifier(Modifier::BOLD))),
        Line::from("  :b :s :br :sr :c :ca   |=pipe  ;=chain"),
        Line::from(""),
        Line::from(Span::styled(" Examples", Style::default().add_modifier(Modifier::BOLD))),
        Line::from("  :br 10 95000           market, $10 risk, SL@95000"),
        Line::from("  :br 10 95000 - 2       market, SL@95000, TP +2%"),
        Line::from("  :br 10 95000 | ts 2% 0.5%  with trailing stop"),
        Line::from("  :ts 2% 0.5%            trigger +2%, callback 0.5%"),
        Line::from("  :ts 97000 500          trigger @97000, callback $500"),
    ];

    let help = Paragraph::new(help_text)
        .block(Block::default().borders(Borders::ALL).title(" Trade "));

    let keys_text = vec![
        Line::from(""),
        Line::from(Span::styled(" Keyboard", Style::default().add_modifier(Modifier::BOLD))),
        Line::from(""),
        Line::from("  Tab       Switch tabs"),
        Line::from("  r         Refresh data"),
        Line::from("  :         Command mode"),
        Line::from("  Esc       Exit mode"),
        Line::from("  Enter     Execute"),
        Line::from("  q         Quit"),
        Line::from("  Ctrl+C    Force quit"),
    ];

    let keys = Paragraph::new(keys_text)
        .block(Block::default().borders(Borders::ALL).title(" Keys "));

    f.render_widget(help, chunks[0]);
    f.render_widget(keys, chunks[1]);
}

fn draw_chart(f: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Min(40),
            Constraint::Length(12),
            Constraint::Length(22),
        ])
        .split(area);

    draw_candlesticks(f, app, chunks[0]);
    draw_price_scale(f, app, chunks[1]);
    draw_trades_tape(f, app, chunks[2]);
}

fn draw_candlesticks(f: &mut Frame, app: &App, area: Rect) {
    if app.candles.is_empty() {
        let msg = Paragraph::new("Loading candles...")
            .block(Block::default().borders(Borders::ALL).title(format!(
                " {} {} ",
                app.symbol, app.chart_interval
            )));
        f.render_widget(msg, area);
        return;
    }

    let candle_width = app.chart_zoom as usize;
    let visible_count = (area.width as usize).saturating_sub(2) / (candle_width + 1);
    let start_idx = app.candles.len().saturating_sub(visible_count + app.chart_offset);
    let end_idx = app.candles.len().saturating_sub(app.chart_offset);
    let visible_candles: Vec<_> = app.candles[start_idx..end_idx].to_vec();

    if visible_candles.is_empty() {
        return;
    }

    let (min_price, max_price) = visible_candles.iter().fold(
        (Decimal::MAX, Decimal::MIN),
        |(min, max), c| {
            (min.min(c.low), max.max(c.high))
        },
    );

    let padding = (max_price - min_price) * Decimal::from_str_exact("0.05").unwrap_or(Decimal::ZERO);
    let y_min = (min_price - padding).to_string().parse::<f64>().unwrap_or(0.0);
    let y_max = (max_price + padding).to_string().parse::<f64>().unwrap_or(100.0);

    let title = format!(
        " {} tf:{} candles:{} offset:{} zoom:{} ←/→ +/- ",
        app.symbol,
        app.chart_interval,
        visible_candles.len(),
        app.chart_offset,
        app.chart_zoom
    );

    let canvas = Canvas::default()
        .block(Block::default().borders(Borders::ALL).title(title))
        .x_bounds([0.0, (visible_count * (candle_width + 1)) as f64])
        .y_bounds([y_min, y_max])
        .paint(|ctx| {
            for level in &app.chart_levels {
                let y = level.to_string().parse::<f64>().unwrap_or(0.0);
                if y >= y_min && y <= y_max {
                    ctx.draw(&CanvasLine {
                        x1: 0.0,
                        y1: y,
                        x2: (visible_count * (candle_width + 1)) as f64,
                        y2: y,
                        color: Color::Yellow,
                    });
                }
            }

            for pos in &app.positions {
                if pos.symbol.0 == app.symbol {
                    let entry_y = pos.entry_price.to_string().parse::<f64>().unwrap_or(0.0);
                    let color = if pos.side == Side::Buy { Color::Green } else { Color::Red };
                    if entry_y >= y_min && entry_y <= y_max {
                        ctx.draw(&CanvasLine {
                            x1: 0.0,
                            y1: entry_y,
                            x2: (visible_count * (candle_width + 1)) as f64,
                            y2: entry_y,
                            color,
                        });
                    }
                }
            }

            for (i, candle) in visible_candles.iter().enumerate() {
                let x = (i * (candle_width + 1)) as f64 + (candle_width as f64 / 2.0);
                let open = candle.open.to_string().parse::<f64>().unwrap_or(0.0);
                let close = candle.close.to_string().parse::<f64>().unwrap_or(0.0);
                let high = candle.high.to_string().parse::<f64>().unwrap_or(0.0);
                let low = candle.low.to_string().parse::<f64>().unwrap_or(0.0);

                let is_bullish = close >= open;
                let color = if is_bullish { Color::Green } else { Color::Red };

                ctx.draw(&CanvasLine {
                    x1: x,
                    y1: low,
                    x2: x,
                    y2: high,
                    color,
                });

                let body_top = open.max(close);
                let body_bottom = open.min(close);
                let body_height = body_top - body_bottom;

                if body_height > 0.0 {
                    ctx.draw(&Rectangle {
                        x: x - (candle_width as f64 / 2.0) + 0.5,
                        y: body_bottom,
                        width: candle_width as f64 - 1.0,
                        height: body_height,
                        color,
                    });
                }
            }
        });

    f.render_widget(canvas, area);
}

fn draw_price_scale(f: &mut Frame, app: &App, area: Rect) {
    if app.candles.is_empty() {
        let block = Block::default().borders(Borders::ALL).title(" Price ");
        f.render_widget(block, area);
        return;
    }

    let visible_count = 50.min(app.candles.len());
    let start_idx = app.candles.len().saturating_sub(visible_count + app.chart_offset);
    let end_idx = app.candles.len().saturating_sub(app.chart_offset);
    let visible_candles = &app.candles[start_idx..end_idx];

    let (min_price, max_price) = visible_candles.iter().fold(
        (Decimal::MAX, Decimal::MIN),
        |(min, max), c| (min.min(c.low), max.max(c.high)),
    );

    let range = max_price - min_price;
    let step = range / Decimal::from(6);
    let available_lines = (area.height as usize).saturating_sub(2);

    let mut lines: Vec<Line> = Vec::new();

    let current_pos = app.positions.iter().find(|p| p.symbol.0 == app.symbol);
    let pnl_color = current_pos.map(|p| {
        if p.unrealized_pnl >= Decimal::ZERO {
            Color::Green
        } else {
            Color::Red
        }
    });

    for i in 0..available_lines {
        let price = max_price - step * Decimal::from(i) / Decimal::from(available_lines.max(1));
        let price_str = format!("{:.2}", price);

        let mut spans = vec![Span::raw(format!("{:>10}", price_str))];

        if let Some(color) = pnl_color {
            if let Some(pos) = current_pos {
                let entry = pos.entry_price;
                if (price >= entry && price <= max_price && pos.side == Side::Buy)
                    || (price <= entry && price >= min_price && pos.side == Side::Sell)
                {
                    spans.push(Span::styled(" █", Style::default().fg(color)));
                }
            }
        }

        lines.push(Line::from(spans));
    }

    let paragraph = Paragraph::new(lines)
        .block(Block::default().borders(Borders::ALL).title(" Price "));

    f.render_widget(paragraph, area);
}

fn draw_trades_tape(f: &mut Frame, app: &App, area: Rect) {
    let available_lines = (area.height as usize).saturating_sub(2);

    let items: Vec<ListItem> = app
        .trades
        .iter()
        .take(available_lines)
        .map(|t| {
            let color = if t.side == Side::Buy {
                Color::Green
            } else {
                Color::Red
            };
            let time = chrono::DateTime::from_timestamp_millis(t.timestamp)
                .map(|dt| dt.format("%H:%M:%S").to_string())
                .unwrap_or_else(|| "??:??:??".to_string());

            let side_str = if t.side == Side::Buy { "B" } else { "S" };

            ListItem::new(Line::from(vec![
                Span::raw(format!("{} ", time)),
                Span::styled(side_str, Style::default().fg(color)),
                Span::raw(format!(" {:.4}", t.qty)),
            ]))
        })
        .collect();

    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(" Trades "));

    f.render_widget(list, area);
}

fn draw_messages(f: &mut Frame, app: &App, area: Rect) {
    let messages: Vec<ListItem> = app
        .messages
        .iter()
        .rev()
        .take(8)
        .rev()
        .map(|m| ListItem::new(Line::from(m.as_str())))
        .collect();

    let list = List::new(messages)
        .block(Block::default().borders(Borders::ALL).title(" Messages "));

    f.render_widget(list, area);
}

fn draw_input(f: &mut Frame, app: &App, area: Rect) {
    let (title, style) = match app.input_mode {
        InputMode::Normal => (" Press ':' for commands ", Style::default()),
        InputMode::Command => (" Command ", Style::default().fg(Color::Yellow)),
    };

    let input = Paragraph::new(format!(":{}", app.input))
        .style(style)
        .block(Block::default().borders(Borders::ALL).title(title));

    f.render_widget(input, area);

    if app.input_mode == InputMode::Command {
        f.set_cursor_position((area.x + 2 + app.input_cursor as u16, area.y + 1));
    }
}
