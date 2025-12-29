# TradeTerminal

Торговый терминал для Bybit. Rust workspace с двумя проектами: server и client.

## Архитектура

- **server** — VPS-сервер, подключается к Bybit API с минимальной задержкой
- **client** — CLI терминал на ratatui, подключается к серверу по WebSocket
- **shared** — общие типы и протокол сообщений

## Технологии

- Rust 2021 edition
- tokio (async runtime)
- tokio-tungstenite (WebSocket)
- ratatui + crossterm (TUI)
- rust_decimal (точные вычисления)
- serde/serde_json (сериализация)

## Структура проекта

```
TradeTerminal/
├── Cargo.toml              # Workspace
├── shared/                 # Общие типы (Order, Position, Ticker, Messages)
├── server/                 # Сервер для VPS
│   └── src/
│       ├── bybit/          # Bybit REST + WebSocket клиент
│       ├── client_handler.rs
│       └── server.rs
└── client/                 # CLI терминал
    └── src/
        ├── app.rs          # Логика, команды
        ├── connection.rs   # WebSocket к серверу
        └── ui.rs           # Рендеринг UI
```

## Команды клиента

| Команда | Сокращение | Описание |
|---------|------------|----------|
| `buy <qty> [price]` | `b` | Ордер на покупку |
| `sell <qty> [price]` | `s` | Ордер на продажу |
| `buyrisk <risk$> <sl> [limit]` | `br` | Long с расчётом от риска |
| `sellrisk <risk$> <sl> [limit]` | `sr` | Short с расчётом от риска |
| `cancel <id>` | `c` | Отменить ордер |
| `cancelall` | `ca` | Отменить все ордера |
| `symbol <sym>` | `sym` | Сменить символ |

## Расчёт риска

Формула: `qty = risk / (|entry - sl| + entry * fee * 2)`

- taker fee: 0.055%
- maker fee: 0.02%
- Комиссия учитывается дважды (вход + выход по SL)

## Правила разработки

- Не оставлять комментарии в коде
- Использовать rust_decimal для всех цен и объёмов
- Сообщения клиент-сервер через shared types
- Валидация на клиенте перед отправкой
