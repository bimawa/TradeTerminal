# TradeTerminal

Торговый терминал для Bybit с CLI интерфейсом. Архитектура клиент-сервер для минимальной задержки при работе с биржей.

## Возможности

- Размещение market/limit ордеров
- Расчёт размера позиции от риска в USDT
- Просмотр открытых ордеров и позиций
- Отмена ордеров
- Работа с несколькими символами

## Требования

- Rust 1.75+
- Docker (для сервера)
- Bybit API ключи

## Быстрый старт

### 1. Клонирование и настройка

```bash
git clone <repo>
cd TradeTerminal
cp .env.example .env
```

Заполните `.env`:
```
BYBIT_API_KEY=your_api_key
BYBIT_API_SECRET=your_api_secret
BYBIT_TESTNET=true
```

### 2. Запуск сервера

**Docker (рекомендуется для VPS):**
```bash
docker-compose up -d
```

**Локально:**
```bash
source .env
cargo run --bin trade-server
```

### 3. Запуск клиента

```bash
export SERVER_URL=ws://127.0.0.1:9000  # или ws://your-vps-ip:9000
cargo run --bin trade-client
```

## Использование

### Навигация

| Клавиша | Действие |
|---------|----------|
| `:` | Режим ввода команды |
| `Tab` | Переключение вкладок |
| `r` | Обновить данные |
| `Esc` | Выход из режима ввода |
| `q` | Выход |
| `Ctrl+C` | Принудительный выход |

### Команды

**Базовые ордера:**
```
:buy 0.01              # Market buy 0.01 BTC
:buy 0.01 95000        # Limit buy @ 95000
:sell 0.01             # Market sell
:sell 0.01 96000       # Limit sell @ 96000
```

**Ордера с расчётом риска:**
```
:buyrisk 10 94000           # Long market, риск $10, SL @ 94000
:buyrisk 10 94000 95000     # Long limit @ 95000
:sellrisk 10 96000          # Short market, риск $10, SL @ 96000
:sellrisk 10 96000 95000    # Short limit @ 95000
```

**Управление:**
```
:cancel abc123         # Отменить ордер
:cancelall             # Отменить все ордера
:symbol ETHUSDT        # Сменить символ
:help                  # Справка
```

**Сокращения:**
- `:b` = `:buy`
- `:s` = `:sell`
- `:br` = `:buyrisk`
- `:sr` = `:sellrisk`
- `:c` = `:cancel`
- `:ca` = `:cancelall`
- `:sym` = `:symbol`

## Расчёт размера позиции

Формула учитывает комиссию биржи:

```
quantity = risk_usdt / (|entry_price - sl_price| + entry_price * fee * 2)
```

- Taker fee: 0.055%
- Maker fee: 0.02%
- Комиссия умножается на 2 (вход + выход по SL)

**Пример:**
- Риск: $10
- Entry: 95000, SL: 94000
- Разница: $1000
- Fee cost: 95000 * 0.00055 * 2 = $104.5
- Total risk per unit: $1104.5
- Quantity: 10 / 1104.5 = 0.00905 BTC

## Деплой на VPS

1. Установите Docker на VPS
2. Скопируйте проект или соберите образ
3. Создайте `.env` с API ключами
4. Запустите: `docker-compose up -d`
5. Подключите клиент: `SERVER_URL=ws://vps-ip:9000`

Рекомендуется размещать VPS в том же регионе, где находятся серверы Bybit (Singapore, Tokyo).

## Лицензия

MIT
