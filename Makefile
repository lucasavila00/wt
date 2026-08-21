.PHONY: bootstrap-server-user check-file-lines clear e2e-tests install-client install-git-server install-server nuke prepare-image static

KVM_INSTALL_CONFIG ?= examples/server-config/wt-server.kvm-e2e-install.toml

bootstrap-server-user:
	scripts/bootstrap-server-user

check-file-lines:
	@rg --files -g '*.rs' | { failed=; while IFS= read -r file; do \
		lines=$$(wc -l < "$$file"); \
		if [ "$$lines" -gt 700 ]; then \
			printf '%s has %s lines (maximum 700)\n' "$$file" "$$lines"; \
			failed=1; \
		fi; \
	done; \
	test -z "$$failed"; }

clear:
	scripts/clear

nuke:
	scripts/nuke

e2e-tests:
	@cargo run --release -p wt-server-setup -- image verify --config "$(KVM_INSTALL_CONFIG)" || { \
		printf '\nImage verification failed. Rebuild the E2E images with:\n  make prepare-image CONFIG=%s\n' "$(KVM_INSTALL_CONFIG)" >&2; \
		exit 1; \
	}
	cargo test -p wt-integration-tests --test kvm_e2e -- --ignored

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

static: check-file-lines
	@set -e; rg --files assets/world -g '*.sh' | sort | while IFS= read -r file; do \
		bash -n "$$file"; \
		shellcheck --shell=sh --severity=warning "$$file"; \
	done
	cargo fmt --all --check
	cargo check --workspace --all-targets --locked
	cargo clippy --workspace --all-targets --locked -- -D warnings
