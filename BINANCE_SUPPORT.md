# Binance Integration

Проект теперь поддерживает работу с биржами Binance и Bybit.

## Переменные окружения

### Для Binance:
```bash
EXCHANGE=binance                  # Указывает использование Binance (по умолчанию: bybit)
BINANCE_API_KEY=<your_api_key>    # API ключ Binance
BINANCE_API_SECRET=<your_secret>  # API секрет Binance
BINANCE_TESTNET=true              # true для testnet, false для mainnet (по умолчанию: false)
```

### Для Bybit (существующие):
```bash
EXCHANGE=bybit                    # Указывает использование Bybit (значение по умолчанию)
BYBIT_API_KEY=<your_api_key>      # API ключ Bybit
BYBIT_API_SECRET=<your_secret>    # API секрет Bybit
BYBIT_TESTNET=true                # true для testnet, false для mainnet (по умолчанию: false)
```

### Общие переменные:
```bash
LISTEN_ADDR=0.0.0.0:9000         # Адрес сервера (по умолчанию: 0.0.0.0:9000)
TLS_ENABLED=false                 # Включить TLS (по умолчанию: false)
TLS_CERT_PATH=/path/to/cert.pem   # Путь к сертификату TLS
TLS_KEY_PATH=/path/to/key.pem     # Путь к ключу TLS
AUTH_SECRET_KEY=your_secret       # Секретный ключ для аутентификации
RUST_LOG=info                     # Уровень логирования
```

## Использование

### Запуск с Binance:
```bash
export EXCHANGE=binance
export BINANCE_API_KEY=your_binance_api_key
export BINANCE_API_SECRET=your_binance_api_secret
cargo run --bin trade-server
```

### Запуск с Bybit:
```bash
export EXCHANGE=bybit
export BYBIT_API_KEY=your_bybit_api_key
export BYBIT_API_SECRET=your_bybit_api_secret
cargo run --bin trade-server
```

### Тестовые сети:
```bash
# Binance Testnet
export EXCHANGE=binance
export BINANCE_TESTNET=true
export BINANCE_API_KEY=testnet_api_key
export BINANCE_API_SECRET=testnet_api_secret

# Bybit Testnet
export EXCHANGE=bybit
export BYBIT_TESTNET=true
export BYBIT_API_KEY=testnet_api_key
export BYBIT_API_SECRET=testnet_api_secret
```

## Реализованный функционал

Для обеих бирж реализован идентичный API:

1. **Управление ордерами:**
   - Размещение ордеров (market, limit)
   - Отмена ордеров
   - Отмена всех ордеров
   - Получение списка активных ордеров

2. **Управление позициями:**
   - Получение открытых позиций
   - Закрытие позиций
   - Трейлинг стоп
   - Режим хеджирования

3. **Рыночные данные:**
   - Тикеры
   - Свечи (klines)
   - WebSocket обновления (ордера, позиции, тикеры, свечи)

## Архитектура

Проект использует унифицированный интерфейс для работы с биржами:

- `server/src/exchange.rs` - обертка над клиентами бирж
- `server/src/binance/` - реализация Binance API
- `server/src/bybit/` - реализация Bybit API
- `server/src/config.rs` - конфигурация с выбором биржи

## Примечания

- Убедитесь, что API ключи имеют необходимые разрешения для торговли и чтения данных
- Для Binance используются Futures endpoints (USDⓈ-M Futures)
- Для Bybit используется V5 API (Linear Perpetual)
- Hedge mode автоматически активируется при необходимости
- WebSocket соединения автоматически переподключаются при обрыве связи
