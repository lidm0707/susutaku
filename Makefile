# susutaku local orchestration.
#
# Daily driver (Metal on host, backend NOT in docker):
#   make up          # postgres + web (nginx UI, http://localhost:3334)
#   make backend     # backend on host, real MLX models from models/
#   make mock-model  # instead of `make backend`: fake model server :8992
#
# Fully-containerized stacks:
#   make e2e         # postgres + mock-model + backend + web + playwright
#   make client-test # hub + backend + sandbox client integration test
#
# Everything: make down

COMPOSE := docker compose
MAIN    := -f docker/compose/base.yml
DEMO    := -f docker/compose/demo.yml
DEPLOY  := -f docker/compose/deploy.yml
E2E     := -f docker/compose/playwright-backend.yml
CLIENT  := -f docker/compose/client-test.yml

WEB_PORT     := 3334
BACKEND_PORT := 8991
MODEL_PORT   := 8992

MOCK_IMAGE := susutaku-mock-model

.PHONY: help run up down web db logs backend mock-model e2e e2e-down client-test client-test-down clean cover cover-html

help:
	@grep -E '^# |^[a-z-]+:' Makefile

# --- daily stack ---------------------------------------------------------

## real-demo: one command — pg + mock-model + backend + web all in docker, open UI
real-demo:
	$(COMPOSE) $(DEMO) up --build -d
	@until curl -sf http://localhost:$(WEB_PORT) >/dev/null; do sleep 1; done
	@open http://localhost:$(WEB_PORT)
	@echo "demo stack up (mock local model). logs: $(COMPOSE) $(DEMO) logs -f"

## deploy: one compose — pg + backend + web(nginx), private, loopback UI only
## host model server must run separately (Metal): make mock-model, or your MLX server on :8992
deploy:
	$(COMPOSE) $(DEPLOY) up --build -d
	@until curl -sf http://localhost:$(WEB_PORT) >/dev/null; do sleep 1; done
	@echo "up: http://localhost:$(WEB_PORT) (login: owner/owner first run)"

## undeploy: stop the deploy stack (keeps data)
undeploy:
	$(COMPOSE) $(DEPLOY) down

## run: one command — db + web in docker, backend on host (blocking, Ctrl-C to stop)
run:
	$(COMPOSE) $(MAIN) up -d --build postgres web
	@echo "web ui: http://localhost:$(WEB_PORT)  (db + web up, backend starting)"
	cargo run -p backend

## up: start postgres + web ui (http://localhost:3334)
up:
	$(COMPOSE) $(MAIN) up -d --build postgres web
	@echo "web ui: http://localhost:$(WEB_PORT)  (backend: run 'make backend' or 'make mock-model')"

## db: start only postgres (host port 5434)
db:
	$(COMPOSE) $(MAIN) up -d postgres

## web: start only web ui
web:
	$(COMPOSE) $(MAIN) up -d --build web

## adminer: db browser at http://localhost:8081 (server: postgres, user susutaku/susutaku)
adminer:
	$(COMPOSE) $(MAIN) --profile adminer up -d adminer
	@echo "adminer: http://localhost:8081 (system: PostgreSQL, server: postgres, user: susutaku)"

## logs: follow compose logs (daily stack)
logs:
	$(COMPOSE) $(MAIN) logs -f

## backend: run backend on host (real MLX, needs Metal)
backend:
	cargo run -p backend

## mock-model: fake model server on :8992 (use INSTEAD of real backend)
mock-model:
	docker build -f docker/mock/Dockerfile.mock-model -t $(MOCK_IMAGE) .
	docker rm -f susutaku-mock-model 2>/dev/null || true
	docker run -d --name susutaku-mock-model -p $(MODEL_PORT):8992 $(MOCK_IMAGE)
	@echo "mock model: http://localhost:$(MODEL_PORT)"

## sandbox-image: build the default podman sandbox image used by agent jails
sandbox-image:
	podman build -f docker/sandbox/Containerfile -t localhost/susutaku-sandbox:latest .

# --- containerized stacks ------------------------------------------------

## e2e: full stack in docker (pg + mock-model + backend + web + playwright)
e2e:
	$(COMPOSE) $(E2E) up --build -d
	$(COMPOSE) $(E2E) logs -f playwright

## client-test: hub + backend + sandbox client, then show RESULT lines
client-test:
	$(COMPOSE) $(CLIENT) up --build -d
	$(COMPOSE) $(CLIENT) logs hub | grep RESULT || true

## down: stop all stacks (keeps volumes)
down:
	$(COMPOSE) $(DEPLOY) down || true
	$(COMPOSE) $(DEMO) down || true
	$(COMPOSE) $(MAIN) down || true
	$(COMPOSE) $(E2E) down || true
	$(COMPOSE) $(CLIENT) down || true
	$(COMPOSE) $(MAIN) --profile adminer down || true
	-docker rm -f susutaku-mock-model

## cover: test-coverage summary for the whole workspace (needs cargo-llvm-cov)
cover:
	cargo llvm-cov --workspace --summary-only --hide-instantiations

## cover-html: coverage report at target/llvm-cov/html/index.html
cover-html:
	cargo llvm-cov --workspace --html --hide-instantiations

## clean: also delete postgres volumes (destroys data)
clean: down
	$(COMPOSE) $(DEPLOY) down -v || true
	$(COMPOSE) $(DEMO) down -v || true
	$(COMPOSE) $(MAIN) down -v
	$(COMPOSE) $(E2E) down -v || true
	$(COMPOSE) $(CLIENT) down -v || true
