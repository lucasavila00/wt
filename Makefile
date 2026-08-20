.PHONY: bootstrap-server-user check-file-lines clear e2e-tests e2e-tests-fast e2e-tests-full install-client install-git-server install-server nuke prepare-image

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

e2e-tests: e2e-tests-full

e2e-tests-fast:
	cargo test -p wt-integration-tests --test kvm_e2e shared_folders::fast_shared_folder_lifecycle -- --ignored --exact --nocapture

e2e-tests-full:
	cargo test -p wt-integration-tests --test kvm_e2e guest_lifecycle::agent_git_transport_works_without_provider_credentials -- --ignored --exact --nocapture

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
