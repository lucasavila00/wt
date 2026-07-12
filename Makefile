.PHONY: bootstrap-server-user clear e2e-tests install-client install-server prepare-image

bootstrap-server-user:
	scripts/bootstrap-server-user

clear:
	scripts/clear

e2e-tests:
	cargo test -p wt-integration-tests --test kvm_e2e -- --ignored

install-client:
	scripts/install-client

install-server:
	@test -n "$(CONFIG)" || { echo "usage: make install-server CONFIG=PATH" >&2; exit 2; }
	scripts/install-server --config "$(CONFIG)"

prepare-image:
	@test -n "$(CONFIG)" || { echo "usage: make prepare-image CONFIG=PATH" >&2; exit 2; }
	scripts/prepare-image --config "$(CONFIG)"
