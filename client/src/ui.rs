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
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;
use trade_shared::Side;

fn dec_to_f64(d: Decimal) -> f64 {
    d.to_f64().unwrap_or(0.0)
}

use crate::app::{App, InputMode, Tab};

pub fn draw(f: &mut Frame, app: &mut App) {
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

fn draw_main(f: &mut Frame, app: &mut App, area: Rect) {
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
        .constraints([Constraint::Percentage(40), Constraint::Percentage(30), Constraint::Percentage(30)])
        .split(area);

    let help_text = vec![
        Line::from(vec![
            Span::styled(" Symbol: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(&app.symbol),
        ]),
        Line::from(""),
        Line::from(Span::styled(" Commands", Style::default().add_modifier(Modifier::BOLD))),
        Line::from("  :buy <qty> [price]"),
        Line::from("  :sell <qty> [price]"),
        Line::from("  :buyrisk <risk$> <sl> [lim] [tp%]"),
        Line::from("  :sellrisk <risk$> <sl> [lim] [tp%]"),
        Line::from("  :close [l|s]"),
        Line::from("  :ts <trigger> <callback>"),
        Line::from("  :ps <secs> [trigger]"),
        Line::from("  :cancel <id>   :cancelall"),
        Line::from(""),
        Line::from(Span::styled(" Shortcuts", Style::default().add_modifier(Modifier::BOLD))),
        Line::from("  :b :s :br :sr :cl :c :ca :ps"),
        Line::from("  |=pipe  ;=chain"),
    ];

    let help = Paragraph::new(help_text)
        .block(Block::default().borders(Borders::ALL).title(" Trade "));

    let chart_text = vec![
        Line::from(Span::styled(" Chart Commands", Style::default().add_modifier(Modifier::BOLD))),
        Line::from(""),
        Line::from("  :chart        Open chart"),
        Line::from("  :tf <int>     Timeframe"),
        Line::from("  :level <p>    Add level"),
        Line::from("  :clevel [p]   Clear level"),
        Line::from("  :sound        Toggle sound"),
        Line::from(""),
        Line::from(Span::styled(" Chart Keys", Style::default().add_modifier(Modifier::BOLD))),
        Line::from(""),
        Line::from("  h/l     Scroll horizontal"),
        Line::from("  j/k     Scroll vertical"),
        Line::from("  +/-     Zoom horizontal"),
        Line::from("  [/]     Zoom vertical"),
        Line::from("  0       Reset view"),
        Line::from(""),
        Line::from(Span::styled(" Chart Lines", Style::default().add_modifier(Modifier::BOLD))),
        Line::from(""),
        Line::from(vec![
            Span::styled("  ━", Style::default().fg(Color::Blue)),
            Span::raw(" Entry"),
        ]),
        Line::from(vec![
            Span::styled("  ━", Style::default().fg(Color::Green)),
            Span::raw(" Take Profit"),
        ]),
        Line::from(vec![
            Span::styled("  ━", Style::default().fg(Color::Red)),
            Span::raw(" Stop Loss"),
        ]),
        Line::from(vec![
            Span::styled("  ━", Style::default().fg(Color::Yellow)),
            Span::raw(" Trailing Stop"),
        ]),
        Line::from(vec![
            Span::styled("  ━", Style::default().fg(Color::Cyan)),
            Span::raw(" Buy Order"),
        ]),
        Line::from(vec![
            Span::styled("  ━", Style::default().fg(Color::Magenta)),
            Span::raw(" Sell Order"),
        ]),
    ];

    let chart_help = Paragraph::new(chart_text)
        .block(Block::default().borders(Borders::ALL).title(" Chart "));

    let keys_text = vec![
        Line::from(Span::styled(" Keyboard", Style::default().add_modifier(Modifier::BOLD))),
        Line::from(""),
        Line::from("  Tab/S-Tab Switch tabs"),
        Line::from("  r         Refresh"),
        Line::from("  :         Command mode"),
        Line::from("  Esc       Exit mode"),
        Line::from("  Enter     Execute"),
        Line::from("  q         Quit"),
    ];

    let keys = Paragraph::new(keys_text)
        .block(Block::default().borders(Borders::ALL).title(" Keys "));

    f.render_widget(help, chunks[0]);
    f.render_widget(chart_help, chunks[1]);
    f.render_widget(keys, chunks[2]);
}

fn draw_chart(f: &mut Frame, app: &mut App, area: Rect) {
    let price_col_width = if let Some(last_price) = app.last_price {
        let int_digits = last_price.trunc().to_string().len();
        (int_digits + 1 + 10 + 2).max(23) as u16
    } else {
        23
    };

    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Min(40),
            Constraint::Length(price_col_width),
            Constraint::Length(22),
        ])
        .split(area);

    let chart_width = chunks[0].width;
    app.chart_area = Some(chunks[0]);
    draw_candlesticks(f, app, chunks[0]);
    draw_price_scale(f, app, chunks[1], chart_width);
    draw_trades_tape(f, app, chunks[2]);
}

fn draw_candlesticks(f: &mut Frame, app: &mut App, area: Rect) {
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
    let visible_candles = &app.candles[start_idx..end_idx];

    if visible_candles.is_empty() {
        return;
    }

    let (min_price, max_price) = visible_candles.iter().fold(
        (Decimal::MAX, Decimal::MIN),
        |(min, max), c| {
            (min.min(c.low), max.max(c.high))
        },
    );

    let base_padding = (max_price - min_price) * Decimal::from_str_exact("0.05").unwrap_or(Decimal::ZERO);
    let zoom_factor = Decimal::from(app.chart_zoom_v);
    let range = max_price - min_price;
    let center = min_price + range / Decimal::from(2);
    let half_range = (range + base_padding * Decimal::from(2)) / zoom_factor / Decimal::from(2);

    let tick_size = if app.chart_zoom_v >= 150 {
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
    } else if app.chart_zoom_v >= 100 {
        if max_price >= Decimal::from(1000) {
            Decimal::from_str_exact("0.05").unwrap()
        } else if max_price >= Decimal::from(100) {
            Decimal::from_str_exact("0.005").unwrap()
        } else if max_price >= Decimal::from(10) {
            Decimal::from_str_exact("0.0005").unwrap()
        } else {
            Decimal::from_str_exact("0.00005").unwrap()
        }
    } else if app.chart_zoom_v >= 50 {
        if max_price >= Decimal::from(1000) {
            Decimal::from_str_exact("0.1").unwrap()
        } else if max_price >= Decimal::from(100) {
            Decimal::from_str_exact("0.01").unwrap()
        } else if max_price >= Decimal::from(10) {
            Decimal::from_str_exact("0.001").unwrap()
        } else {
            Decimal::from_str_exact("0.0001").unwrap()
        }
    } else if app.chart_zoom_v >= 20 {
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

    let v_offset = if app.price_tracking {
        if let Some(current_price) = app.last_price {
            (current_price - center) / tick_size
        } else {
            Decimal::from(app.chart_offset_v)
        }
    } else {
        Decimal::from(app.chart_offset_v)
    };
    let adjusted_center = center + tick_size * v_offset;
    
    let y_min = dec_to_f64(adjusted_center - half_range);
    let y_max = dec_to_f64(adjusted_center + half_range);

    let title = format!(
        " {} tf:{} h/l +/- [/] j/k 0=reset 1=track ",
        app.symbol,
        app.chart_interval,
    );

    let tracking_status = if app.price_tracking { "track:ON" } else { "track:OFF" };
    let right_title = format!(" {} ", tracking_status);

    let canvas = Canvas::default()
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(title)
                .title_top(Line::from(right_title).right_aligned())
        )
        .x_bounds([0.0, (visible_count * (candle_width + 1)) as f64])
        .y_bounds([y_min, y_max])
        .paint(|ctx| {
            for level in &app.chart_levels {
                let y = dec_to_f64(*level);
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
                    let x_end = (visible_count * (candle_width + 1)) as f64;

                    let entry_y = dec_to_f64(pos.entry_price);
                    if entry_y >= y_min && entry_y <= y_max {
                        ctx.draw(&CanvasLine {
                            x1: 0.0,
                            y1: entry_y,
                            x2: x_end,
                            y2: entry_y,
                            color: Color::Blue,
                        });
                    }

                    if let Some(tp) = pos.take_profit {
                        let tp_y = dec_to_f64(tp);
                        if tp_y >= y_min && tp_y <= y_max {
                            ctx.draw(&CanvasLine {
                                x1: 0.0,
                                y1: tp_y,
                                x2: x_end,
                                y2: tp_y,
                                color: Color::Green,
                            });
                        }
                    }

                    if let Some(sl) = pos.stop_loss {
                        let sl_y = dec_to_f64(sl);
                        if sl_y >= y_min && sl_y <= y_max {
                            ctx.draw(&CanvasLine {
                                x1: 0.0,
                                y1: sl_y,
                                x2: x_end,
                                y2: sl_y,
                                color: Color::Red,
                            });
                        }
                    }

                    if let Some(ts) = pos.trailing_stop {
                        let ts_y = dec_to_f64(ts);
                        if ts_y >= y_min && ts_y <= y_max {
                            ctx.draw(&CanvasLine {
                                x1: 0.0,
                                y1: ts_y,
                                x2: x_end,
                                y2: ts_y,
                                color: Color::Yellow,
                            });
                        }
                    }
                }
            }

            for order in &app.orders {
                if order.symbol.0 == app.symbol {
                    if let Some(price) = order.price {
                        let order_y = dec_to_f64(price);
                        if order_y >= y_min && order_y <= y_max {
                            let color = if order.side == Side::Buy {
                                Color::Cyan
                            } else {
                                Color::Magenta
                            };
                            ctx.draw(&CanvasLine {
                                x1: 0.0,
                                y1: order_y,
                                x2: (visible_count * (candle_width + 1)) as f64,
                                y2: order_y,
                                color,
                            });
                        }
                    }
                }
            }

            for (i, candle) in visible_candles.iter().enumerate() {
                let x = (i * (candle_width + 1)) as f64 + (candle_width as f64 / 2.0);
                let open = dec_to_f64(candle.open);
                let close = dec_to_f64(candle.close);
                let high = dec_to_f64(candle.high);
                let low = dec_to_f64(candle.low);

                if high < y_min || low > y_max {
                    continue;
                }

                let is_bullish = close >= open;
                let color = if is_bullish { Color::Green } else { Color::Red };

                let body_top = open.max(close);
                let body_bottom = open.min(close);

                let wick_top = high;
                let wick_bottom = low;

                if wick_top > y_min && wick_top <= y_max && body_top >= y_min && body_top <= y_max {
                    ctx.draw(&CanvasLine {
                        x1: x,
                        y1: body_top,
                        x2: x,
                        y2: wick_top,
                        color,
                    });
                }

                if wick_bottom >= y_min && wick_bottom < y_max && body_bottom >= y_min && body_bottom <= y_max {
                    ctx.draw(&CanvasLine {
                        x1: x,
                        y1: wick_bottom,
                        x2: x,
                        y2: body_bottom,
                        color,
                    });
                }

                let visible_body_top = body_top.min(y_max);
                let visible_body_bottom = body_bottom.max(y_min);
                let body_height = visible_body_top - visible_body_bottom;

                if body_height > 0.0 {
                    ctx.draw(&Rectangle {
                        x: x - (candle_width as f64 / 2.0) + 0.5,
                        y: visible_body_bottom,
                        width: candle_width as f64 - 1.0,
                        height: body_height,
                        color,
                    });
                }
            }

            if let Some((mouse_x, mouse_y)) = app.mouse_position {
                if mouse_x >= area.x && mouse_x < area.x + area.width
                    && mouse_y >= area.y && mouse_y < area.y + area.height
                {
                    let x_in_canvas = ((mouse_x - area.x).saturating_sub(1) as f64).min((visible_count * (candle_width + 1)) as f64);
                    let x_adjusted = (x_in_canvas - 1.0).max(0.0);

                    ctx.draw(&CanvasLine {
                        x1: x_adjusted,
                        y1: y_min,
                        x2: x_adjusted,
                        y2: y_max,
                        color: Color::Gray,
                    });

                    let inner_height = area.height.saturating_sub(2);
                    let y_pos_in_chart = mouse_y.saturating_sub(area.y + 1);

                    if y_pos_in_chart < inner_height {
                        let y_ratio_f64 = y_pos_in_chart as f64 / inner_height.saturating_sub(1).max(1) as f64;
                        let price_at_mouse = y_max - (y_max - y_min) * y_ratio_f64;

                        ctx.draw(&CanvasLine {
                            x1: 0.0,
                            y1: price_at_mouse,
                            x2: (visible_count * (candle_width + 1) + 1) as f64,
                            y2: price_at_mouse,
                            color: Color::Gray,
                        });
                    }
                }
            }
        });

    f.render_widget(canvas, area);
}

fn draw_price_scale(f: &mut Frame, app: &App, area: Rect, chart_width: u16) {
    if app.candles.is_empty() {
        let block = Block::default().borders(Borders::ALL).title(" Price ");
        f.render_widget(block, area);
        return;
    }

    let candle_width = app.chart_zoom as usize;
    let visible_count = (chart_width as usize).saturating_sub(2) / (candle_width + 1);
    let start_idx = app.candles.len().saturating_sub(visible_count + app.chart_offset);
    let end_idx = app.candles.len().saturating_sub(app.chart_offset);
    let visible_candles = &app.candles[start_idx..end_idx];

    if visible_candles.is_empty() {
        let block = Block::default().borders(Borders::ALL).title(" Price ");
        f.render_widget(block, area);
        return;
    }

    let (min_price, max_price) = visible_candles.iter().fold(
        (Decimal::MAX, Decimal::MIN),
        |(min, max), c| (min.min(c.low), max.max(c.high)),
    );

    let base_padding = (max_price - min_price) * Decimal::from_str_exact("0.05").unwrap_or(Decimal::ZERO);
    let zoom_factor = Decimal::from(app.chart_zoom_v);
    let price_range = max_price - min_price;
    let center = min_price + price_range / Decimal::from(2);
    let half_range = (price_range + base_padding * Decimal::from(2)) / zoom_factor / Decimal::from(2);

    let tick_size = if app.chart_zoom_v >= 150 {
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
    } else if app.chart_zoom_v >= 100 {
        if max_price >= Decimal::from(1000) {
            Decimal::from_str_exact("0.05").unwrap()
        } else if max_price >= Decimal::from(100) {
            Decimal::from_str_exact("0.005").unwrap()
        } else if max_price >= Decimal::from(10) {
            Decimal::from_str_exact("0.0005").unwrap()
        } else {
            Decimal::from_str_exact("0.00005").unwrap()
        }
    } else if app.chart_zoom_v >= 50 {
        if max_price >= Decimal::from(1000) {
            Decimal::from_str_exact("0.1").unwrap()
        } else if max_price >= Decimal::from(100) {
            Decimal::from_str_exact("0.01").unwrap()
        } else if max_price >= Decimal::from(10) {
            Decimal::from_str_exact("0.001").unwrap()
        } else {
            Decimal::from_str_exact("0.0001").unwrap()
        }
    } else if app.chart_zoom_v >= 20 {
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
        price_range / Decimal::from(100)
    };

    let v_offset = if app.price_tracking {
        if let Some(current_price) = app.last_price {
            (current_price - center) / tick_size
        } else {
            Decimal::from(app.chart_offset_v)
        }
    } else {
        Decimal::from(app.chart_offset_v)
    };
    let adjusted_center = center + tick_size * v_offset;

    let y_min = adjusted_center - half_range;
    let y_max = adjusted_center + half_range;
    let range = y_max - y_min;

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

    let mouse_price = if let Some((mouse_x, mouse_y)) = app.mouse_position {
        if let Some(chart_area) = app.chart_area {
            if mouse_x >= chart_area.x && mouse_x < chart_area.x + chart_area.width
                && mouse_y >= chart_area.y && mouse_y < chart_area.y + chart_area.height
            {
                let inner_height = chart_area.height.saturating_sub(2);
                let y_pos_in_chart = mouse_y.saturating_sub(chart_area.y + 1);
                if y_pos_in_chart < inner_height {
                    let y_ratio = Decimal::from(y_pos_in_chart) / Decimal::from(inner_height.saturating_sub(1).max(1));
                    Some(y_max - (y_max - y_min) * y_ratio)
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        }
    } else {
        None
    };

    let price_width = if let Some(last_price) = app.last_price {
        let int_digits = last_price.trunc().to_string().len();
        (int_digits + 1 + 10).max(14)
    } else {
        14
    };

    let align_width = area.width.saturating_sub(2) as usize;

    for i in 0..available_lines {
        let raw_price = y_max - range * Decimal::from(i) / Decimal::from(available_lines.saturating_sub(1).max(1));

        let price = if app.chart_zoom_v >= 20 && !tick_size.is_zero() {
            (raw_price / tick_size).round() * tick_size
        } else {
            raw_price
        };

        let display_price = raw_price;

        let price_str = {
            let price_rounded = display_price.round_dp(10);
            let price_string = price_rounded.to_string();

            if let Some(dot_idx) = price_string.find('.') {
                let int_part = &price_string[..dot_idx];
                let frac_part = &price_string[dot_idx + 1..];
                format!("{}.{:0<10}", int_part, frac_part)
            } else {
                format!("{}.{:0<10}", price_string, "")
            }
        };

        // Check if this price is in the PnL range for coloring
        let pnl_highlight_color = if let (Some(color), Some(pos), Some(current)) = (pnl_color, current_pos, app.last_price) {
            let entry = pos.entry_price;
            let min_p = entry.min(current);
            let max_p = entry.max(current);
            // Add tolerance of half a row to ensure at least one row is highlighted
            let row_tolerance = range / Decimal::from(available_lines.max(1) * 2);
            if price >= min_p - row_tolerance && price <= max_p + row_tolerance {
                Some(color)
            } else {
                None
            }
        } else {
            None
        };

        let diff_percent = if let Some(current_price) = app.last_price {
            if !current_price.is_zero() {
                ((price - current_price) / current_price) * Decimal::from(100)
            } else {
                Decimal::ZERO
            }
        } else {
            Decimal::ZERO
        };
        let sign = if diff_percent >= Decimal::ZERO { "+" } else { "" };

        let is_mouse_line = if let Some(mp) = mouse_price {
            (price - mp).abs() <= range / Decimal::from(available_lines * 2)
        } else {
            false
        };

        let mut spans = if is_mouse_line {
            let percent_str = format!("{}{:.2}%", sign, diff_percent);
            let price_aligned = format!("{:>width$}", price_str, width = price_width);
            vec![Span::styled(
                format!("{} {}", percent_str, price_aligned),
                Style::default().fg(Color::White).bg(Color::DarkGray).add_modifier(Modifier::BOLD),
            )]
        } else if let Some(color) = pnl_highlight_color {
            vec![
                Span::styled(format!("{:>width$}", price_str, width = align_width), Style::default().fg(color)),
                Span::styled(" █", Style::default().fg(color)),
            ]
        } else {
            vec![Span::raw(format!("{:>width$}", price_str, width = align_width))]
        };

        lines.push(Line::from(spans));
    }

    let paragraph = Paragraph::new(lines)
        .block(Block::default().borders(Borders::ALL).title(" Price "));

    f.render_widget(paragraph, area);
}

fn draw_trades_tape(f: &mut Frame, app: &App, area: Rect) {
    let has_panic_stop = app.panic_stop_remaining_ms.is_some();
    let available_lines = (area.height as usize).saturating_sub(2);
    let trades_lines = if has_panic_stop {
        available_lines.saturating_sub(1)
    } else {
        available_lines
    };

    let mut items: Vec<ListItem> = Vec::new();

    if let Some(remaining_ms) = app.panic_stop_remaining_ms {
        let remaining_secs = remaining_ms as f64 / 1000.0;
        let timer_style = if app.panic_stop_active {
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        let status_str = if app.panic_stop_active {
            if let Some(trigger) = app.panic_stop_trigger_price {
                format!("PS: {:.1}s @{:>9}", remaining_secs, trigger)
            } else {
                format!("PS: {:.1}s", remaining_secs)
            }
        } else {
            "PS: ---".to_string()
        };
        items.push(ListItem::new(Line::from(vec![
            Span::styled(status_str, timer_style),
        ])));
    }

    let trade_items: Vec<ListItem> = app
        .trades
        .iter()
        .take(trades_lines)
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
                Span::raw(format!(" {:>9}", t.price)),
            ]))
        })
        .collect();

    items.extend(trade_items);

    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(" Trades "));

    f.render_widget(list, area);
}

fn draw_messages(f: &mut Frame, app: &App, area: Rect) {
    let visible_count = (area.height as usize).saturating_sub(2).min(8);
    let start = app.messages.len().saturating_sub(visible_count);
    
    let messages: Vec<ListItem> = app
        .messages
        .iter()
        .skip(start)
        .enumerate()
        .map(|(i, m)| {
            let actual_idx = start + i;
            let reverse_idx = app.messages.len().saturating_sub(1 + actual_idx);
            
            if app.input_mode == InputMode::Copy && reverse_idx == app.copy_index {
                ListItem::new(Line::from(m.as_str()))
                    .style(Style::default().bg(Color::DarkGray).add_modifier(Modifier::BOLD))
            } else {
                ListItem::new(Line::from(m.as_str()))
            }
        })
        .collect();

    let title = if app.input_mode == InputMode::Copy {
        " Messages [COPY: j/k=nav, Enter=copy, Esc=exit] "
    } else {
        " Messages (y=copy mode) "
    };

    let list = List::new(messages)
        .block(Block::default().borders(Borders::ALL).title(title));

    f.render_widget(list, area);
}

fn draw_input(f: &mut Frame, app: &App, area: Rect) {
    let (title, style) = match app.input_mode {
        InputMode::Normal => (" Press ':' for commands ", Style::default()),
        InputMode::Command => (" Command ", Style::default().fg(Color::Yellow)),
        InputMode::Copy => (" Copy Mode ", Style::default().fg(Color::Cyan)),
    };

    let input = Paragraph::new(format!(":{}", app.input))
        .style(style)
        .block(Block::default().borders(Borders::ALL).title(title));

    f.render_widget(input, area);

    if app.input_mode == InputMode::Command {
        f.set_cursor_position((area.x + 2 + app.input_cursor as u16, area.y + 1));
    }
}
