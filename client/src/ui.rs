use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, List, ListItem, Paragraph, Row, Table, Tabs},
    Frame,
};
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
    let titles = vec!["Orders", "Positions", "Trade"];
    let selected = match app.tab {
        Tab::Orders => 0,
        Tab::Positions => 1,
        Tab::Trade => 2,
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
        Line::from("  :cancel <id>                      :cancelall"),
        Line::from("  :symbol <sym>"),
        Line::from(""),
        Line::from(Span::styled(" Shortcuts", Style::default().add_modifier(Modifier::BOLD))),
        Line::from("  :b = buy   :s = sell   :br = buyrisk   :sr = sellrisk"),
        Line::from("  :c = cancel            :ca = cancelall"),
        Line::from(""),
        Line::from(Span::styled(" Risk Order Examples", Style::default().add_modifier(Modifier::BOLD))),
        Line::from("  :br 10 95000           market long, $10 risk, SL@95000"),
        Line::from("  :br 10 95000 96500     limit long @96500, SL@95000"),
        Line::from("  :br 10 95000 - 2       market long, $10 risk, SL@95000, limin skip -, TP +2%"),
        Line::from("  :br 10 95000 96500 2   limit @96500, SL@95000, TP +2%"),
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
        f.set_cursor_position((area.x + 2 + app.input.len() as u16, area.y + 1));
    }
}
