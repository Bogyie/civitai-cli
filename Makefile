.PHONY: build run run-debug lint fmt fetch-log tail-fetch-log clear-fetch-log

UNAME_S := $(shell uname -s)
ifeq ($(UNAME_S),Darwin)
  CONFIG_DIR := $(HOME)/Library/Application\ Support/com.civitai/civitai-cli
else
  CONFIG_DIR := $(HOME)/.config/com.civitai/civitai-cli
endif

FETCH_LOG_PATH := $(CONFIG_DIR)/fetch_debug.log

build:
	cargo build

run:
	cargo run

run-debug:
	RUST_LOG=debug cargo run

lint:
	cargo clippy -- -D warnings

fmt:
	cargo fmt --all

fetch-log:
	@if [ -f "$(FETCH_LOG_PATH)" ]; then \
		cat "$(FETCH_LOG_PATH)"; \
	else \
		echo "No fetch debug log found at $(FETCH_LOG_PATH)"; \
	fi

tail-fetch-log:
	tail -f "$(FETCH_LOG_PATH)"

clear-fetch-log:
	rm -f "$(FETCH_LOG_PATH)"
