# susutaku — web chat to inline qwen3.8 (MLX)
#
# Quick start (web chat in Docker, model on host Metal):
#   make run        # starts backend :8991 + web container :3334
#   open http://localhost:3334
#
# Dev mode (no Docker):
#   make backend    # terminal 1 — axum + qwen3.8 on :8991
#   make web        # terminal 2 — vite dev on :5173 (proxies /api)
#
# Build & lint:
#   make build      # release build of backend
#   make check      # cargo check
#   make clippy     # cargo clippy
#   make web-build  # yarn production build of web_ui
#
# Download model:
#   make download-model          # default model (qwen3.8)
#   make download-model MODEL=<key>   # qwen3.8 | gemma4 | gemma4-e4b
#
# Docker lifecycle:
#   make docker-up    # build + start web on :3334 (backend must run on host)
#   make docker-down  # stop the stack

PORT := 8991
WEB_PORT := 3334
COMPOSE := docker compose -f docker/docker-compose.yml

# Load env from the repo root (.env) for every target.
ifneq (,$(wildcard .env))
include .env
export
endif

MODELS_DIR := models
QWEN38_REPO := mlx-community/Qwen3.8-27B-4bit
GEMMA4_REPO := mlx-community/gemma-4-26b-a4b-it-4bit
# NOTE: E4B is ~11 GiB (under the 30 GiB policy floor) — hf_loader will not
# offer it until the loader/ARCH support it; kept for engine bring-up.
GEMMA4_E4B_REPO := google/gemma-4-E4B-it-qat-w4a16-ct
DEFAULT_MODEL := qwen3.8
HF := hf

.PHONY: help init download-model check clippy build backend web web-build docker-up docker-down run

help:
	@echo "make check       - cargo check (workspace)"
	@echo "make clippy      - cargo clippy (workspace)"
	@echo "make build       - cargo build --release -p backend"
	@echo "make backend     - run axum backend on :$(PORT)"
	@echo "make web         - vite dev server (proxies /api to :$(PORT))"
	@echo "make web-build   - yarn build web_ui"
	@echo "make docker-up   - build + start web container on :$(WEB_PORT)"
	@echo "make docker-down - stop compose stack"
	@echo "make run         - backend + docker web together"
	@echo "make init        - setup project (rust deps, web_ui, models dir)"
	@echo "make download-model [MODEL=qwen3.8|gemma4|gemma4-e4b] - download MLX model"

IGNORED_DIRS := models input output .plans

init: $(IGNORED_DIRS)
	cargo fetch
	cargo build
	cd web_ui && yarn install
	@echo "setup done — run 'make download-model' to fetch a model"

$(IGNORED_DIRS):
	mkdir -p $@

download-model: $(MODELS_DIR)
	@case "$(if $(MODEL),$(MODEL),$(DEFAULT_MODEL))" in \
	  qwen3.8) repo=$(QWEN38_REPO) ;; \
	  gemma4) repo=$(GEMMA4_REPO) ;; \
	  gemma4-e4b) repo=$(GEMMA4_E4B_REPO) ;; \
	  *) echo "unknown model: $(MODEL). use qwen3.8, gemma4 or gemma4-e4b"; exit 1 ;; \
	esac; \
	$(HF) download $$repo --local-dir $(MODELS_DIR)

check:
	cargo check

clippy:
	cargo clippy

build:
	cargo build --release -p backend

backend:
	cargo run -p backend --release

web:
	cd web_ui && yarn dev

web-build:
	cd web_ui && yarn install && yarn build

docker-up:
	$(COMPOSE) up web --build -d

docker-down:
	$(COMPOSE) down

run:
	@trap 'kill 0' EXIT; \
	$(MAKE) -s backend & \
	$(MAKE) -s docker-up; \
	wait
