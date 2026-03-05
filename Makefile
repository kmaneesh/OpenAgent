# OpenAgent — build targets
#
# Usage:
#   make                    # cross-compile all Go services + Rust sandbox
#   make local              # build all services for your current host platform only
#   make <service>          # cross-compile a single Go service
#   make sandbox            # cross-compile the Rust sandbox service
#   make test-go            # run Go tests for all Go services + sdk-go
#   make test-rust          # run Rust tests for sdk-rust + sandbox
#   make test-py            # run Python tests (openagent/ + app/)
#   make clean              # remove all compiled binaries
#
# Prerequisites:
#   Go services   : go 1.21+
#   Rust sandbox  : rustup
#                   Cross-compilation uses `cross` (cargo install cross --locked)
#                   and requires Docker.  Darwin (arm64 and amd64) builds natively on Mac.

GO_SERVICES := hello filesystem discord telegram slack whatsapp

PLATFORMS := linux/arm64 linux/amd64 darwin/arm64 darwin/amd64

# Detect host platform for `make local`
UNAME_S := $(shell uname -s)
UNAME_M := $(shell uname -m)

ifeq ($(UNAME_S),Darwin)
  HOST_OS := darwin
else
  HOST_OS := linux
endif

ifeq ($(UNAME_M),arm64)
  HOST_ARCH := arm64
else ifeq ($(UNAME_M),aarch64)
  HOST_ARCH := arm64
else
  HOST_ARCH := amd64
endif

HOST_SUFFIX := $(HOST_OS)-$(HOST_ARCH)

.PHONY: all local clean test-go test-rust test-py help sandbox $(GO_SERVICES)

# Default: cross-compile everything
all: $(GO_SERVICES) sandbox

# ---------------------------------------------------------------------------
# Go services — per-service cross-compile rule
# ---------------------------------------------------------------------------

define build_service
$(1):
	@echo "==> services/$(1)"
	@mkdir -p services/$(1)/bin
	cd services/$(1) && GOOS=linux  GOARCH=arm64 go build -ldflags="-s -w" -o bin/$(1)-linux-arm64  .
	cd services/$(1) && GOOS=linux  GOARCH=amd64 go build -ldflags="-s -w" -o bin/$(1)-linux-amd64  .
	cd services/$(1) && GOOS=darwin GOARCH=arm64 go build -ldflags="-s -w" -o bin/$(1)-darwin-arm64 .
	cd services/$(1) && GOOS=darwin GOARCH=amd64 go build -ldflags="-s -w" -o bin/$(1)-darwin-amd64 .
	@echo "   ✓ bin/$(1)-linux-arm64  bin/$(1)-linux-amd64  bin/$(1)-darwin-arm64  bin/$(1)-darwin-amd64"
endef

$(foreach svc,$(GO_SERVICES),$(eval $(call build_service,$(svc))))

# ---------------------------------------------------------------------------
# Rust sandbox — cross-compile via `cross`
# ---------------------------------------------------------------------------
# Darwin arm64 builds natively; Linux targets use musl for static binaries.

sandbox:
	@echo "==> services/sandbox (Rust)"
	@mkdir -p services/sandbox/bin
ifeq ($(HOST_OS),darwin)
	cd services/sandbox && cargo build --release --target aarch64-apple-darwin
	cp services/sandbox/target/aarch64-apple-darwin/release/sandbox \
	   services/sandbox/bin/sandbox-darwin-arm64
	@if rustup target list --installed 2>/dev/null | grep -q x86_64-apple-darwin; then \
	  cd services/sandbox && cargo build --release --target x86_64-apple-darwin && \
	  cp services/sandbox/target/x86_64-apple-darwin/release/sandbox \
	     services/sandbox/bin/sandbox-darwin-amd64; \
	else \
	  echo "   Skipping darwin/amd64 (run: rustup target add x86_64-apple-darwin)"; \
	fi
endif
	@if command -v cross >/dev/null 2>&1; then \
	  cd services/sandbox && cross build --release --target aarch64-unknown-linux-musl && \
	  cp services/sandbox/target/aarch64-unknown-linux-musl/release/sandbox \
	     services/sandbox/bin/sandbox-linux-arm64 && \
	  cd services/sandbox && cross build --release --target x86_64-unknown-linux-musl && \
	  cp services/sandbox/target/x86_64-unknown-linux-musl/release/sandbox \
	     services/sandbox/bin/sandbox-linux-amd64; \
	else \
	  echo "   Skipping linux targets (install: cargo install cross --locked)"; \
	fi
	@echo "   ✓ sandbox binaries in services/sandbox/bin/"

# ---------------------------------------------------------------------------
# Build only for the current host (faster dev loop)
# ---------------------------------------------------------------------------

local:
	@echo "==> Building Go services for $(HOST_OS)/$(HOST_ARCH)"
	@for svc in $(GO_SERVICES); do \
	  mkdir -p services/$$svc/bin; \
	  echo "  → $$svc"; \
	  cd services/$$svc && \
	    GOOS=$(HOST_OS) GOARCH=$(HOST_ARCH) go build -ldflags="-s -w" -o bin/$$svc-$(HOST_SUFFIX) . && \
	    cd ../..; \
	done
	@echo "==> Building sandbox (Rust) for $(HOST_OS)/$(HOST_ARCH)"
	@mkdir -p services/sandbox/bin
ifeq ($(HOST_OS),darwin)
	cd services/sandbox && cargo build --release --target aarch64-apple-darwin
	cp services/sandbox/target/aarch64-apple-darwin/release/sandbox \
	   services/sandbox/bin/sandbox-$(HOST_SUFFIX)
else
	cd services/sandbox && cargo build --release
	cp services/sandbox/target/release/sandbox \
	   services/sandbox/bin/sandbox-$(HOST_SUFFIX)
endif
	@echo "Done — binaries in services/<name>/bin/<name>-$(HOST_SUFFIX)"

# ---------------------------------------------------------------------------
# Tests
# ---------------------------------------------------------------------------

# Go tests for all services + shared SDK
test-go:
	@for pkg in sdk-go $(GO_SERVICES); do \
	  echo "→ testing services/$$pkg ..."; \
	  cd services/$$pkg && go test ./... && cd ../..; \
	done

# Rust tests
test-rust:
	@echo "→ testing services/sdk-rust ..."
	cd services/sdk-rust && cargo test
	@echo "→ testing services/sandbox ..."
	cd services/sandbox && cargo test

# Python tests (skip inspire/ reference implementations)
test-py:
	python -m pytest openagent/ app/ -q

# ---------------------------------------------------------------------------
# Clean
# ---------------------------------------------------------------------------

clean:
	@for svc in $(GO_SERVICES); do \
	  rm -f services/$$svc/bin/$$svc-*; \
	  echo "  cleaned services/$$svc/bin/"; \
	done
	rm -f services/sandbox/bin/sandbox-*
	@echo "  cleaned services/sandbox/bin/"

help:
	@echo ""
	@echo "OpenAgent build targets"
	@echo "  make              Cross-compile all services ($(PLATFORMS))"
	@echo "  make local        Build for current host only ($(HOST_OS)/$(HOST_ARCH))"
	@echo "  make <service>    Cross-compile one Go service: $(GO_SERVICES)"
	@echo "  make sandbox      Cross-compile Rust sandbox"
	@echo "                   darwin/amd64: rustup target add x86_64-apple-darwin"
	@echo "                   linux: cargo install cross --locked"
	@echo "  make test-go      Run Go tests"
	@echo "  make test-rust    Run Rust tests"
	@echo "  make test-py      Run Python tests"
	@echo "  make clean        Remove compiled binaries"
	@echo ""
	@echo "  Rust cross-compile: cargo install cross --locked"
	@echo "  MSB required at runtime: msb server start --dev"
	@echo ""
