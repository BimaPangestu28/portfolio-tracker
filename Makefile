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
.PHONY: help install dev backend frontend gateway test test-backend test-frontend \
        build build-backend build-frontend fmt lint clean

help: ## Show this help
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) \
		| awk 'BEGIN {FS = ":.*?## "}; {printf "  \033[36m%-16s\033[0m %s\n", $$1, $$2}'

install: ## Install frontend + gateway deps (backend deps fetched by cargo)
	cd $(FRONTEND_DIR) && npm install
	cd $(GATEWAY_DIR) && npm install

backend: ## Run backend API (override port: make backend BIND_ADDR=127.0.0.1:8089)
	cd $(BACKEND_DIR) && BIND_ADDR=$(BIND_ADDR) cargo run

frontend: ## Run frontend dev server (http://localhost:5173, proxies /api -> :8080)
	cd $(FRONTEND_DIR) && npm run dev

gateway: ## Run WhatsApp Baileys gateway (backend must be up; scan the QR). Needs GATEWAY_TOKEN.
	cd $(GATEWAY_DIR) && npm start

dev: ## Run backend + frontend together (Ctrl-C stops both)
	@echo "Starting backend ($(BIND_ADDR)) + frontend (:5173)..."
	@trap 'kill 0' EXIT INT TERM; \
		( cd $(BACKEND_DIR) && BIND_ADDR=$(BIND_ADDR) cargo run ) & \
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
