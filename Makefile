# Portfolio Tracker — backend (Rust/axum) + frontend (Vite/React) + WhatsApp gateway (Node/Baileys)
#
# `backend` and `frontend` are also directory names; they MUST stay in .PHONY,
# otherwise `make backend` sees the existing directory and prints
# "Nothing to be done for 'backend'". Recipe lines are TAB-indented.

BACKEND_DIR  := backend
FRONTEND_DIR := frontend
GATEWAY_DIR  := whatsapp-gateway

# Override: `make backend BIND_ADDR=127.0.0.1:8089` (8080 may be taken locally).
BIND_ADDR ?= 127.0.0.1:8080

SHELL := /bin/bash
.DEFAULT_GOAL := help
.PHONY: help install env dev backend frontend gateway test test-backend test-frontend \
        build build-backend build-frontend fmt lint clean

# Source a service's local .env (KEY=VALUE lines) into the recipe shell if present.
# Run from inside the service directory. The Makefile's BIND_ADDR still wins for the
# backend because it is assigned explicitly on the cargo line.
LOAD_ENV = set -a; [ -f .env ] && . ./.env; set +a;

help: ## Show this help
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) \
		| awk 'BEGIN {FS = ":.*?## "}; {printf "  \033[36m%-16s\033[0m %s\n", $$1, $$2}'

install: ## Install frontend + gateway deps (backend deps fetched by cargo)
	cd $(FRONTEND_DIR) && npm install
	cd $(GATEWAY_DIR) && npm install

env: ## Bootstrap local .env files from the *.env.example samples (won't overwrite)
	@for dir in $(BACKEND_DIR) $(GATEWAY_DIR); do \
		if [ -f $$dir/.env ]; then echo "skip  $$dir/.env (exists)"; \
		else cp $$dir/.env.example $$dir/.env && echo "wrote $$dir/.env"; fi; \
	done
	@if [ -f $(FRONTEND_DIR)/.env.local ]; then echo "skip  $(FRONTEND_DIR)/.env.local (exists)"; \
	else cp $(FRONTEND_DIR)/.env.example $(FRONTEND_DIR)/.env.local && echo "wrote $(FRONTEND_DIR)/.env.local"; fi

backend: ## Run backend API (override port: make backend BIND_ADDR=127.0.0.1:8089)
	cd $(BACKEND_DIR) && $(LOAD_ENV) BIND_ADDR=$(BIND_ADDR) cargo run

frontend: ## Run frontend dev server (http://localhost:5173, proxies /api -> :8080)
	cd $(FRONTEND_DIR) && npm run dev

gateway: ## Run WhatsApp Baileys gateway (backend must be up; scan the QR). Needs GATEWAY_TOKEN.
	cd $(GATEWAY_DIR) && $(LOAD_ENV) npm start

dev: ## Run backend + frontend together (Ctrl-C stops both)
	@echo "Starting backend ($(BIND_ADDR)) + frontend (:5173)..."
	@trap 'kill 0' EXIT INT TERM; \
		( cd $(BACKEND_DIR) && $(LOAD_ENV) BIND_ADDR=$(BIND_ADDR) cargo run ) & \
		( cd $(FRONTEND_DIR) && npm run dev ) & \
		wait

test: test-backend test-frontend ## Run all tests

test-backend: ## Backend tests (cargo)
	cd $(BACKEND_DIR) && cargo test

test-frontend: ## Frontend tests (vitest)
	cd $(FRONTEND_DIR) && npm test

build: build-backend build-frontend ## Build everything (release backend + frontend bundle)

build-backend: ## Build backend (release)
	cd $(BACKEND_DIR) && cargo build --release

build-frontend: ## Build frontend (static bundle in frontend/dist)
	cd $(FRONTEND_DIR) && npm run build

fmt: ## Format Rust code
	cd $(BACKEND_DIR) && cargo fmt

lint: ## Lint Rust (clippy)
	cd $(BACKEND_DIR) && cargo clippy

clean: ## Remove build artifacts
	cd $(BACKEND_DIR) && cargo clean
	rm -rf $(FRONTEND_DIR)/dist
