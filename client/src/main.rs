mod app;
mod audio;
mod connection;
mod tls;
mod ui;

use anyhow::Result;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
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
    terminal.clear()?;
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
    let mut last_render = std::time::Instant::now();
    let refresh_interval = std::time::Duration::from_secs(1);
    let render_interval = std::time::Duration::from_millis(33);
    let mut needs_render = true;

    loop {
        if needs_render && last_render.elapsed() >= render_interval {
            terminal.draw(|f| ui::draw(f, &mut app))?;
            last_render = std::time::Instant::now();
            needs_render = false;
        }

        if event::poll(std::time::Duration::from_millis(8))? {
            match event::read()? {
                Event::Key(key) => {
                    if key.code == KeyCode::Esc && !app.is_input_mode() {
                        return Ok(());
                    }
                    if key.code == KeyCode::Char('q') && !app.is_input_mode() {
                        return Ok(());
                    }
                    app.handle_key(key).await?;
                    needs_render = true;
                }
                Event::Mouse(mouse) => {
                    app.handle_mouse(mouse).await?;
                    needs_render = true;
                }
                _ => {}
            }
        }

        if app.process_messages().await? {
            needs_render = true;
        }

        if last_refresh.elapsed() >= refresh_interval {
            app.auto_refresh().await?;
            last_refresh = std::time::Instant::now();
            needs_render = true;
        }
    }
}
