.PHONY: bootstrap-server-user clear install-client install-server prepare-image

bootstrap-server-user:
	scripts/bootstrap-server-user

clear:
	scripts/clear

install-client:
	scripts/install-client

install-server:
	@test -n "$(CONFIG)" || { echo "usage: make install-server CONFIG=PATH" >&2; exit 2; }
	scripts/install-server --config "$(CONFIG)"

prepare-image:
	@test -n "$(CONFIG)" || { echo "usage: make prepare-image CONFIG=PATH" >&2; exit 2; }
	scripts/prepare-image --config "$(CONFIG)"
