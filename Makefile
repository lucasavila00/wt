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

check-snapshot-lines:
	cargo run --quiet -p wt-repository-checks -- snapshot-lines

clear:
	scripts/clear

nuke:
	scripts/nuke

e2e-tests:
	@test -f /home/wt/.codex/.wt-auth/auth.json \
		&& systemctl is-active --quiet wt-codex-integration-auth.path \
		&& test -f /var/lib/wt/registry-cache/ca/ca.crt \
		&& test "$$(docker inspect --format '{{.State.Running}}' wt-workload-registry-cache 2>/dev/null)" = true || { \
		printf '\nKVM E2E host prerequisites are missing. Install them with:\n  make install-server CONFIG=%s\n' "$(KVM_INSTALL_CONFIG)" >&2; \
		exit 1; \
	}
	@cargo run --release -p wt-server-installer -- image verify --config "$(KVM_INSTALL_CONFIG)" || { \
		printf '\nImage verification failed. Rebuild the E2E images with:\n  make prepare-image CONFIG=%s\n' "$(KVM_INSTALL_CONFIG)" >&2; \
		exit 1; \
	}
	cargo test -p wt-end-to-end-tests --test kvm_e2e -- --ignored

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

static: check-crate-readmes check-file-lines check-snapshot-lines
	@set -e; rg --files assets/world -g '*.sh' | sort | while IFS= read -r file; do \
		bash -n "$$file"; \
		shellcheck --shell=sh --severity=warning "$$file"; \
	done
	cargo fmt --all --check
	cargo check --workspace --all-targets --locked
	cargo clippy --workspace --all-targets --locked -- -D warnings
