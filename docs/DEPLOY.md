# Развёртывание (production)

Руководство для **Linux VPS** с nginx и systemd. Подходит для машин от 1 CPU / 1 GB RAM.

Проект не предназначен для Windows или macOS: 3x-ui, systemd и production-сборка рассчитаны на Linux.

Перед установкой убедитесь, что на VPS доступны:

```bash
uname -a                  # Linux
systemctl --version       # systemd
nginx -t                  # nginx (если используете reverse proxy)
rustc --version           # после установки rustup
```

## 1. Установка

```bash
sudo mkdir -p /opt
sudo git clone https://github.com/Sp0nge-bob/middlewarejson-rust.git /opt/middlewarejson
cd /opt/middlewarejson

# Установите Rust: https://rustup.rs
rustup default stable

cp .env.example .env
nano .env   # см. docs/CONFIGURATION.md
chmod 600 .env

mkdir -p data
cargo build --release
```

Установите `TRANSFORM_MODE=rules` в `.env`, если нужны балансировщики.

**Миграция с Python:** скопируйте существующие `.env` и `data/middleware.db` — Rust читает ту же схему SQLite.

## 2. Первичная настройка через CLI

```bash
./target/release/middlewarejson-cli
```

Рекомендуемый порядок:

1. **П. 1** — настройки панели (URL, web base path, token), если не всё в `.env`
2. **П. 3** — проверка Panel API
3. **П. 9** — синхронизация каталога и групп
4. **П. 8** — балансировщики (при `TRANSFORM_MODE=rules`)
5. **П. 10** — ручной запуск для проверки (`curl http://127.0.0.1:8080/health`)
6. **П. 5** — установка systemd

Или из командной строки:

```bash
./target/release/middlewarejson-cli service install
```

### User vs system unit

- Запуск **без root** → user unit (`~/.config/systemd/user/middlewarejson.service`)
- Запуск **от root** → system unit (`/etc/systemd/system/middlewarejson.service`)

Для user unit без активной сессии:

```bash
loginctl enable-linger $USER
```

CLI подскажет эту команду после установки.

## 3. Nginx

Фрагмент для существующего `server { listen 443 ssl; ... }`:

```nginx
location <AGENT_JSON_PATH>/ {
    proxy_pass http://127.0.0.1:8080;   # порт = AGENT_PORT из .env
    proxy_http_version 1.1;
    proxy_set_header Host $host;
    proxy_set_header X-Real-IP $remote_addr;
    proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
    proxy_set_header X-Forwarded-Proto $scheme;
    proxy_read_timeout 30s;
    proxy_connect_timeout 10s;
}
```

Полный пример: [deploy/nginx.conf.example](../deploy/nginx.conf.example).

## 4. Обновление

```bash
./deploy/update.sh
# или: export APP_DIR=/path/to/project && ./deploy/update.sh
```

Скрипт выполняет `git pull`, `cargo build --release` и перезапуск systemd.

## 5. Проверка

```bash
curl -s http://127.0.0.1:8080/health
systemctl status middlewarejson   # или systemctl --user status middlewarejson
./target/release/middlewarejson-cli settings script-show
```

## Чеклист

- [ ] Linux VPS, `cargo build --release` выполнен на сервере
- [ ] `.env` заполнен, `chmod 600 .env`
- [ ] `TRANSFORM_MODE=rules` (если нужны балансировщики)
- [ ] Синхронизация каталога и групп (п. 9)
- [ ] Балансировщики созданы (п. 8)
- [ ] systemd установлен и active
- [ ] nginx проксирует `<AGENT_JSON_PATH>/` на `127.0.0.1:AGENT_PORT`
- [ ] Подписка отдаёт JSON с балансировщиками