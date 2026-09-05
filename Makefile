.PHONY: $(MAKECMDGOALS)

KVM_INSTALL_CONFIG ?= examples/server-config/wt-server.kvm-e2e-install.toml

bootstrap-server-user:
	scripts/bootstrap-server-user

check-crate-readmes:
	@rg --files crates -g 'Cargo.toml' | { failed=; while IFS= read -r manifest; do \
		crate=$${manifest%/Cargo.toml}; \
		if [ ! -f "$$crate/README.md" ]; then \
			printf '%s has no README.md\n' "$$crate"; \
			failed=1; \
		fi; \
	done; \
	test -z "$$failed"; }

check-file-lines:
	@rg --files -g '*.rs' | { failed=; while IFS= read -r file; do \
		lines=$$(wc -l < "$$file"); \
		if [ "$$lines" -gt 700 ]; then \
			printf '%s has %s lines (maximum 700)\n' "$$file" "$$lines"; \
			failed=1; \
		fi; \
	done; \
	test -z "$$failed"; }

check-api:
	npm run generate:api
	git diff --exit-code -- api/generated crates/products/wt/client/src/api/generated.rs

check-install-checkout:
	scripts/test-require-clean-checkout

check-snapshot-lines:
	cargo run --quiet -p wt-repository-checks -- snapshot-lines

clear:
	scripts/clear

nuke:
	scripts/nuke

e2e-tests:
	scripts/check-kvm-e2e-host
	cargo run --quiet -p wt-server-installer --bin wts -- prepare-e2e --config "$(KVM_INSTALL_CONFIG)"
	cargo run --quiet -p wt-server-installer --bin wts -- validate-e2e --config "$(KVM_INSTALL_CONFIG)"
	cargo test -p wt-end-to-end-tests --test install_server_bootstrap -- --ignored --nocapture
	cargo run --quiet -p wt-server-installer --bin wts -- validate --config "$(KVM_INSTALL_CONFIG)"
	scripts/clear --codex-sessions /home/wt/.config/wt/kvm-test/codex/sessions
	cargo run --release -p wt-server-installer --bin wts -- install --config "$(KVM_INSTALL_CONFIG)"
	cargo test -p wt-end-to-end-tests --test kvm_e2e -- --ignored --nocapture
	@printf '\nWT E2E test server remains installed on this host.\n'

install-client:
	scripts/install-client

install-git-server:
	@test -n "$(CONFIG)" || { echo "usage: make install-git-server CONFIG=PATH" >&2; exit 2; }
	scripts/install-git-server --config "$(CONFIG)"

install-server:
	@test -n "$(CONFIG)" || { echo "usage: make install-server CONFIG=PATH" >&2; exit 2; }
	scripts/install-server --config "$(CONFIG)"

prepare-image:
	@test -n "$(CONFIG)" || { echo "usage: make prepare-image CONFIG=PATH" >&2; exit 2; }
	scripts/prepare-image --config "$(CONFIG)"

shell:
	cargo run -p wt-client -- shell

check-typescript:
	npm run check:typescript

ci: static check-api
	cargo test --workspace --locked

static: check-crate-readmes check-file-lines check-install-checkout check-snapshot-lines check-typescript
	@set -e; rg --files assets/world -g '*.sh' | sort | while IFS= read -r file; do \
		bash -n "$$file"; \
		shellcheck --shell=sh --severity=warning "$$file"; \
	done
	cargo fmt --all --check
	cargo check --workspace --all-targets --locked
	cargo clippy --workspace --all-targets --locked -- -D warnings
