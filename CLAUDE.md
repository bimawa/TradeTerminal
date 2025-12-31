# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

TradeTerminal — торговый терминал для Bybit с архитектурой клиент-сервер. Сервер размещается на VPS рядом с биржей для минимальной задержки, клиент — CLI на ratatui.

## Build & Run Commands

```bash
just server          # Запуск сервера локально
just client          # Запуск клиента
just build           # cargo build --release
just test            # cargo test
just lint            # cargo clippy
just fmt             # cargo fmt
just docker-up       # docker-compose up -d
```

Переменные окружения (из .env):
- `BYBIT_API_KEY`, `BYBIT_API_SECRET`, `BYBIT_TESTNET`
- `SERVER_URL` — WebSocket URL сервера для клиента

## Architecture

```
shared/     → Типы: Order, Position, Ticker, Symbol, Side
            → Протокол: ClientPayload, ServerPayload
            → Расчёт риска: calculate_position_size()

server/     → bybit/client.rs — REST API (ордера, позиции, trailing stop)
            → client_handler.rs — обработка WebSocket сообщений от клиента
            → server.rs — WebSocket сервер на порту 9000

client/     → app.rs — команды, pipe оператор, история команд
            → ui.rs — рендеринг ratatui (Orders/Positions/Trade tabs)
            → connection.rs — WebSocket к серверу
```

## Key Implementation Details

**Команды клиента** определены в `client/src/app.rs`:
- `COMMANDS` — массив с именами и алиасами
- `match_command()` — маппинг алиасов на имена
- Pipe `|` сохраняет `PendingAction`, выполняется при появлении позиции
- Цепочка `;` выполняет команды последовательно

**Bybit API** в `server/src/bybit/client.rs`:
- HMAC-SHA256 подпись для приватных эндпоинтов
- Hedge mode: `positionIdx` (1=Long, 2=Short)
- `settleCoin=USDT` обязателен для get_orders

**Trailing Stop**:
- Устанавливается только на открытую позицию
- `active_price` — цена активации (trigger)
- `trailing_stop` — callback distance
- Направление active_price зависит от side (+ для Long, - для Short)

**Расчёт позиции**: `qty = risk / (|entry - sl| + entry * fee * 2)`

## Code Style

- Не оставлять комментарии в коде
- `rust_decimal::Decimal` для всех цен и объёмов
- Округление qty: `round_quantity(qty, 0)` для большинства символов
- Делать не "как проще", а как правильно — не переиспользовать payload/типы для разных целей, создавать отдельные варианты enum для разных событий
- Каждый ServerPayload/ClientPayload должен иметь чёткое семантическое значение
