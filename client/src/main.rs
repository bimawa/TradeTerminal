mod app;
mod connection;
mod ui;

use anyhow::Result;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::prelude::*;
use std::io;
use tokio::sync::mpsc;

use app::App;
use connection::Connection;

#[tokio::main]
async fn main() -> Result<()> {
    let server_url = std::env::var("SERVER_URL").unwrap_or_else(|_| "ws://127.0.0.1:9000".into());

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let (tx, rx) = mpsc::channel(100);
    let connection = Connection::new(&server_url, tx.clone());
    let conn_tx = connection.sender();

    tokio::spawn(async move {
        if let Err(e) = connection.run().await {
            tracing::error!("Connection error: {}", e);
        }
    });

    let app = App::new(conn_tx, rx);
    let result = run_app(&mut terminal, app).await;

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    if let Err(e) = result {
        eprintln!("Error: {}", e);
    }

    Ok(())
}

async fn run_app<B: Backend>(terminal: &mut Terminal<B>, mut app: App) -> Result<()> {
    let mut last_refresh = std::time::Instant::now();
    let refresh_interval = std::time::Duration::from_secs(1);

    loop {
        terminal.draw(|f| ui::draw(f, &app))?;

        if event::poll(std::time::Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
                    return Ok(());
                }
                if key.code == KeyCode::Char('q') && !app.is_input_mode() {
                    return Ok(());
                }
                app.handle_key(key).await?;
            }
        }

        app.process_messages().await?;

        if last_refresh.elapsed() >= refresh_interval {
            app.auto_refresh().await?;
            last_refresh = std::time::Instant::now();
        }
    }
}
