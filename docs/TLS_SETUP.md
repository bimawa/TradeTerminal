# Настройка шифрованного соединения (TLS)

## Обзор

TradeTerminal поддерживает защищенное соединение между сервером и клиентом через TLS 1.3 с аутентификацией по предварительно распределенному ключу (Pre-Shared Key).

**Преимущества:**
- 🔒 Шифрование TLS 1.3 (ChaCha20-Poly1305 или AES-GCM)
- 🛡️ Защита от атак "человек посередине" (MITM) через certificate pinning
- 🔑 Простая аутентификация по секретному ключу
- ⚡ Минимальная задержка (<1мс overhead)

## Быстрый старт

### 1. Генерация сертификата на сервере

```bash
cd TradeTerminal

# Узнайте IP адрес вашего VPS
curl ifconfig.me

# Сгенерируйте сертификат (замените на ваш IP)
./scripts/generate_certs.sh 123.45.67.89

# Результат:
# - certs/server.crt (сертификат)
# - certs/server.key (приватный ключ)
# - certs/fingerprint.txt (отпечаток для клиента)
```

**Вывод будет примерно таким:**
```
===================================
TradeTerminal Certificate Generator
===================================

Server IP: 123.45.67.89
Output directory: ./certs

Certificate generated successfully!
  Certificate: ./certs/server.crt
  Private key: ./certs/server.key
  Fingerprint: ./certs/fingerprint.txt

SHA-256 Fingerprint:
  a1b2c3d4e5f6...

Add this to your client .env:
  TLS_CERT_FINGERPRINT=a1b2c3d4e5f6...
```

**Сохраните отпечаток (fingerprint)** - он понадобится для настройки клиента!

### 2. Генерация ключа аутентификации

```bash
# Сгенерируйте случайный ключ
./scripts/generate_auth_key.sh

# Результат:
# ===================================
# Authentication Key Generated
# ===================================
#
# 9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08
#
# Add this to both server and client .env files:
#   AUTH_SECRET_KEY=9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08
```

**Важно:** Этот ключ должен быть одинаковым на сервере и клиенте!

### 3. Настройка сервера

Добавьте в `server/.env`:

```env
# Включить TLS
TLS_ENABLED=true

# Пути к сертификату и ключу
TLS_CERT_PATH=./certs/server.crt
TLS_KEY_PATH=./certs/server.key

# Ключ аутентификации (из шага 2)
AUTH_SECRET_KEY=9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08
```

### 4. Настройка клиента

Добавьте в `client/.env`:

```env
# WebSocket URL с wss:// (замените на IP вашего сервера)
SERVER_URL=wss://123.45.67.89:9000

# Отпечаток сертификата (из шага 1)
TLS_CERT_FINGERPRINT=a1b2c3d4e5f6...

# Ключ аутентификации (тот же, что на сервере)
AUTH_SECRET_KEY=9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08
```

### 5. Запуск

```bash
# На сервере (VPS)
just server

# Вывод:
# Server listening on 0.0.0.0:9000 (TLS: true)

# На клиенте (локальная машина)
just client

# При успешном подключении:
# Sent authentication, waiting for server response...
# Client authenticated successfully
```

## Детальная инструкция

### Генерация сертификата вручную

Если скрипт не работает, используйте бинарный файл напрямую:

```bash
# Сборка бинарника
cargo build --release --bin generate-cert

# Генерация сертификата
./target/release/generate-cert --ip 123.45.67.89 --output-dir ./certs

# Опции:
#   --ip <IP>           IP адрес сервера (по умолчанию: 127.0.0.1)
#   --output-dir <DIR>  Директория для сертификатов (по умолчанию: ./certs)
#   -h, --help          Справка
```

### Генерация ключа аутентификации вручную

Если у вас нет `openssl`:

```bash
# Linux/macOS
head -c 32 /dev/urandom | xxd -p -c 32

# Или используйте онлайн генератор случайных hex строк (64 символа)
```

### Проверка конфигурации

**Сервер:**
```bash
# Проверьте, что файлы существуют
ls -la certs/
# Должны быть: server.crt, server.key, fingerprint.txt

# Проверьте переменные окружения
grep TLS server/.env
grep AUTH_SECRET_KEY server/.env
```

**Клиент:**
```bash
# Проверьте переменные окружения
grep SERVER_URL client/.env
grep TLS_CERT_FINGERPRINT client/.env
grep AUTH_SECRET_KEY client/.env
```

## Безопасность

### Защита приватного ключа

```bash
# Установите правильные права доступа
chmod 600 certs/server.key
chmod 644 certs/server.crt
```

### Ротация ключей

**Ротация ключа аутентификации** (рекомендуется каждые 30-90 дней):

```bash
# 1. Сгенерируйте новый ключ
./scripts/generate_auth_key.sh

# 2. Обновите на сервере
nano server/.env  # Замените AUTH_SECRET_KEY

# 3. Перезапустите сервер
just server

# 4. Обновите на клиентах
nano client/.env  # Замените AUTH_SECRET_KEY

# 5. Перезапустите клиенты
just client
```

**Ротация сертификата** (рекомендуется каждые 12 месяцев):

```bash
# 1. Сгенерируйте новый сертификат
./scripts/generate_certs.sh YOUR_IP

# 2. Обновите fingerprint на клиентах
nano client/.env  # Замените TLS_CERT_FINGERPRINT

# 3. Перезапустите сервер (автоматически загрузит новый сертификат)
just server

# 4. Перезапустите клиенты
just client
```

### Что НЕ нужно хранить в git

Добавьте в `.gitignore`:
```
certs/
*.key
*.crt
.env
```

## Отключение TLS (для тестирования)

Если нужно временно отключить TLS:

**Сервер:**
```env
TLS_ENABLED=false
```

**Клиент:**
```env
SERVER_URL=ws://127.0.0.1:9000
# Можно закомментировать TLS_CERT_FINGERPRINT и AUTH_SECRET_KEY
```

## Диагностика проблем

### Ошибка: "TLS handshake failed"

**Причины:**
- Неправильный IP в сертификате
- Сертификат истек
- Проблемы с сетью

**Решение:**
```bash
# Пересоздайте сертификат с правильным IP
./scripts/generate_certs.sh CORRECT_IP
```

### Ошибка: "Authentication failed: invalid key"

**Причины:**
- Разные ключи на сервере и клиенте
- Опечатка при копировании

**Решение:**
```bash
# Убедитесь, что ключ одинаковый
diff <(grep AUTH_SECRET_KEY server/.env) <(grep AUTH_SECRET_KEY client/.env)
```

### Ошибка: "Certificate fingerprint mismatch"

**Причины:**
- Fingerprint не совпадает с сертификатом на сервере
- Сертификат был изменен

**Решение:**
```bash
# Получите актуальный fingerprint
cat certs/fingerprint.txt

# Обновите в client/.env
nano client/.env
```

### Проверка соединения

```bash
# На сервере, проверьте логи
tail -f /path/to/server.log

# Должны видеть:
# Server listening on 0.0.0.0:9000 (TLS: true)
# New connection from 1.2.3.4
# Client authenticated successfully

# На клиенте
# Должны видеть:
# Sent authentication, waiting for server response...
# (и затем интерфейс терминала)
```

## Docker

Если используете Docker, смонтируйте сертификаты:

```yaml
# docker-compose.yml
services:
  server:
    volumes:
      - ./certs:/app/certs:ro
    environment:
      - TLS_ENABLED=true
      - TLS_CERT_PATH=/app/certs/server.crt
      - TLS_KEY_PATH=/app/certs/server.key
      - AUTH_SECRET_KEY=${AUTH_SECRET_KEY}
```

## Архитектура

```
┌─────────────┐                    ┌─────────────┐
│   Client    │    wss://IP:9000   │   Server    │
│             │◄──────────────────►│   (VPS)     │
│ - Pinning   │   TLS 1.3          │             │
│ - Auth Key  │   + Auth PSK       │ - Cert      │
└─────────────┘                    └─────────────┘
      │                                    │
      │                                    │
      └────────────────────────────────────┘
         Зашифрованный канал
         - ChaCha20-Poly1305 / AES-GCM
         - Forward Secrecy
         - Certificate Pinning
```

## FAQ

**Q: Зачем нужен fingerprint, если есть TLS?**
A: Fingerprint (certificate pinning) защищает от атак с поддельными сертификатами, даже если у атакующего есть доверенный CA-сертификат.

**Q: Можно ли использовать Let's Encrypt?**
A: Да, но это сложнее. Самоподписанный сертификат проще для VPS с только IP адресом.

**Q: Как часто ротировать ключи?**
A: Ключ аутентификации - каждые 30-90 дней, сертификат - каждые 12 месяцев.

**Q: Что делать, если потерял ключ аутентификации?**
A: Сгенерируйте новый, обновите на сервере и всех клиентах.

**Q: Безопасно ли хранить ключи в .env?**
A: Да, если .env не коммитится в git и имеет правильные права доступа (chmod 600).

**Q: Можно ли подключить несколько клиентов?**
A: Да, все клиенты используют одинаковый fingerprint и auth key.

## Дополнительная информация

- [План имплементации](../CLAUDE.md)
- [Архитектура проекта](../README.md)
- [Конфигурация безопасности](./SECURITY.md)
