# middlewarejson (Rust)

Middleware между VPN-клиентами (HAPP и др.) и панелью **3x-ui**. Проксирует JSON-подписки, сохраняет заголовки upstream и применяет трансформации: балансировщики по группам клиентов.

Drop-in замена Python-версии: совместим с `data/middleware.db` и `.env`.

## Требования

- **Linux VPS** (systemd, nginx) — единственная целевая платформа
- Rust 1.75+ (stable), устанавливается на сервере через [rustup](https://rustup.rs)
- Панель **3x-ui** с JSON-подписками и Panel API token (на том же или другом Linux-хосте)

Проект не предназначен для запуска на Windows или macOS. 3x-ui на desktop-Windows не используется.

## Где собирать

Release-сборку выполняйте **на VPS**, рядом с 3x-ui:

```bash
cd /opt/middlewarejson   # каталог клонирования — см. «Быстрый старт»
cargo build --release
```

Бинарники (Linux, без `.exe`):

- `target/release/middlewarejson` — HTTP-агент
- `target/release/middlewarejson-cli` — CLI

## Быстрый старт

```bash
sudo mkdir -p /opt
sudo git clone https://github.com/Sp0nge-bob/middlewarejson-rust.git /opt/middlewarejson
cd /opt/middlewarejson

rustup default stable
cp .env.example .env
# Отредактируйте .env — см. docs/CONFIGURATION.md
chmod 600 .env

cargo build --release
```

```bash
./target/release/middlewarejson-cli          # интерактивное меню
curl -s http://127.0.0.1:8080/health         # {"status":"ok"}
```

## Меню CLI

| # | Раздел | Действие |
|---|--------|----------|
| 1 | Настройки | Показать / изменить настройки панели |
| 2 | Настройки | Показать настройки скрипта (.env) |
| 3 | Настройки | Проверить подключение к Panel API |
| 4 | Настройки | Состояние systemd-службы |
| 5 | Настройки | Установить службу systemd |
| 6–7 | Данные панели | Список инбаундов / групп |
| 8 | Настройка JSON | Балансировщики |
| 9 | Синхронизация | Каталог + группы клиентов |
| 10 | Отладка | Запуск агента вручную |

Команды без меню:

```bash
middlewarejson-cli settings show
middlewarejson-cli catalog sync
middlewarejson-cli group sync
middlewarejson-cli service status
middlewarejson-cli service install
middlewarejson-cli balancer create --name "Pool" --members 1,7
```

## Деплой

- [docs/DEPLOY.md](docs/DEPLOY.md)

```bash
chmod 600 .env
middlewarejson-cli          # п. 5 — установить systemd
# nginx: deploy/nginx.conf.example
```

## Документация

| Файл | Содержание |
|------|------------|
| [docs/CONFIGURATION.md](docs/CONFIGURATION.md) | Переменные `.env` |
| [docs/DEPLOY.md](docs/DEPLOY.md) | VPS, nginx, systemd |
| [docs/TZ.md](docs/TZ.md) | Техническое задание |
| [SECURITY.md](SECURITY.md) | Секреты и уязвимости |

## Разработка

Offline-тесты core (`cargo test`) можно гонять на любой ОС. Интеграция с systemd, nginx и 3x-ui — только на Linux VPS.

```bash
cargo test
cargo clippy --all-targets
```

## Лицензия

MIT — см. [LICENSE](LICENSE).