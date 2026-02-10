# Testing Binance Integration

## Известные проблемы

### WebSocket Connection Issues
При запуске сервера могут появляться ошибки:
```
ERROR trade_server::server: Client handler error payload=GetPositions error=Failed to send response to client
```

Это происходит когда:
1. WebSocket соединение разрывается при отправке сообщения
2. Клиент отключается до получения ответа

**Улучшения добавлены:**
- Детальное логирование ошибок WebSocket
- Логирование завершения write task
- Лучшая обработка ошибок сериализации

## Тестирование с Binance

### 1. Получение API ключей

**Testnet (рекомендуется для тестирования):**
1. Зарегистрируйтесь на https://testnet.binancefuture.com/
2. Создайте API ключи в разделе API Management
3. Убедитесь, что включены разрешения для торговли

**Mainnet (только для реальной торговли):**
1. Зарегистрируйтесь на https://www.binance.com/
2. Включите Futures trading
3. Создайте API ключи с разрешениями для Futures trading

### 2. Настройка переменных окружения

```bash
# Для Testnet
export EXCHANGE=binance
export BINANCE_TESTNET=true
export BINANCE_API_KEY=<your_testnet_api_key>
export BINANCE_API_SECRET=<your_testnet_api_secret>
export RUST_LOG=debug  # Для подробного логирования

# Опционально
export LISTEN_ADDR=127.0.0.1:9000
```

### 3. Запуск сервера

```bash
just server
# или
cargo run --bin trade-server
```

**Ожидаемый вывод:**
```
INFO trade_server: Starting trade server on 0.0.0.0:9000 with exchange: Binance
INFO trade_server::server: Server listening on 0.0.0.0:9000 (TLS: false)
INFO trade_server::binance::ws: Binance private WebSocket connected
```

### 4. Запуск клиента

```bash
just client
# или
cargo run --bin trade-client
```

### 5. Тестовые операции

**Проверка подключения:**
1. Клиент должен подключиться и показать Connected
2. Должны загрузиться позиции (если есть)
3. Должны загрузиться активные ордера (если есть)

**Тестирование ордеров:**
1. Создайте limit ордер на тестовом аккаунте
2. Проверьте, что он появляется в списке активных ордеров
3. Отмените ордер
4. Проверьте, что он исчез из списка

**Тестирование позиций:**
1. Откройте позицию (market order)
2. Проверьте, что позиция отображается
3. Закройте позицию
4. Проверьте, что позиция закрылась

### 6. Отладка проблем

**Если клиент не подключается:**
- Проверьте, что сервер запущен (`netstat -an | grep 9000`)
- Проверьте логи сервера на наличие ошибок
- Убедитесь, что LISTEN_ADDR правильный

**Если WebSocket disconnects:**
- Увеличьте RUST_LOG=trace для максимального логирования
- Проверьте сетевое соединение
- Убедитесь, что API ключи валидны

**Если получаете API ошибки:**
- Проверьте, что API ключи правильные
- Убедитесь, что включен testnet (если используете testnet ключи)
- Проверьте права доступа API ключей
- Для Binance: убедитесь, что ключи созданы для Futures (USDⓈ-M)

### 7. Проверка функционала

**REST API:**
```bash
# В логах сервера вы должны видеть:
# - GET запросы к Binance API
# - Подписи запросов
# - Ответы от биржи
```

**WebSocket:**
```bash
# В логах сервера вы должны видеть:
# - Binance private WebSocket connected
# - Binance chart WebSocket connected
# - Обновления ордеров (при изменениях)
# - Обновления позиций (при изменениях)
```

### 8. Сравнение с Bybit

Чтобы убедиться, что Binance работает идентично Bybit:

1. Запустите с Bybit:
```bash
export EXCHANGE=bybit
export BYBIT_TESTNET=true
export BYBIT_API_KEY=<key>
export BYBIT_API_SECRET=<secret>
just server
```

2. Выполните те же операции
3. Проверьте, что поведение идентично

### 9. Логи для анализа

Полезные логи для отправки при обнаружении проблем:

```bash
RUST_LOG=debug cargo run --bin trade-server 2>&1 | tee server.log
```

Сохраните весь вывод в файл для анализа.

## Известные ограничения

1. **Binance Futures only**: Реализация работает только с Futures (USDⓈ-M), не со Spot
2. **Hedge mode**: Автоматически переключается в hedge mode при необходимости
3. **Символы**: Используйте формат "BTCUSDT", "ETHUSDT" и т.д.
4. **Интервалы свечей**: Используйте "1m", "5m", "15m", "1h", "4h", "1d"

## Troubleshooting

**Error: "BINANCE_API_KEY and BINANCE_API_SECRET required when EXCHANGE=binance"**
- Убедитесь, что переменные окружения установлены
- Проверьте export команды

**Error: "Binance error -2014: API-key format invalid"**
- API ключ неправильный формат
- Пересоздайте ключи

**Error: "Binance error -2015: Invalid API-key, IP, or permissions"**
- Проверьте права доступа ключа
- Убедитесь, что IP разрешен (если установлены IP ограничения)
- Для testnet используйте testnet ключи

**Error: "Binance error -1021: Timestamp for this request is outside of the recvWindow"**
- Проверьте системное время
- Синхронизируйте время: `sudo ntpdate -s time.nist.gov`
