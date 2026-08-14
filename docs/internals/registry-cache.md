# Registry cache

The registry cache is a server-side pull-through cache for public container
image blobs used by devcontainer worlds. It is not a world and holds no
repository or provider credentials.

`wt-server-setup` owns its CA, storage, Docker container, and daemon trust
configuration. The devcontainer bootstrap receives the cache URL and CA and
configures the guest Docker daemon for the allowed registries.

Host and GitHub CI worlds receive no cache configuration from this path.

Configuration lives under `[registry_cache]` in the server install input. The
cache is shared infrastructure: deleting one world does not clear it. `make
nuke` removes the standard installed cache state.
