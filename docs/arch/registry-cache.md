# Registry cache

Parent: [architecture](./README.md).

Each KVM world has its own Docker daemon. Worlds do not share
`/var/lib/docker`.

The KVM host runs one registry proxy. Docker pulls go through it. The proxy
caches image blobs on the host. Worlds still have separate containers, volumes,
networks, and writable layers.

```text
world Docker ──► host registry cache ──► public registry
```

## Config

The cache is part of the strict server config:

```toml
[registry_cache]
state_dir = "/var/lib/wt/registry-cache"
port = 3128
max_size_gib = 64
registries = ["docker.io", "mcr.microsoft.com"]
```

- `state_dir`: cached blobs and proxy CA.
- `port`: proxy port on the libvirt bridge.
- `max_size_gib`: hard cache size limit.
- `registries`: public registries to cache.

Other registries pass through without caching.

## Setup

`wt-server-setup`:

1. Starts the pinned registry-proxy container on the libvirt bridge.
2. Installs the proxy CA on the host.
3. Configures host Docker to use the proxy.

Manifest caching is off. Tags are checked upstream on every pull. Immutable
blobs come from the local cache when present. Pushes bypass the cache.

## Worlds

Cloud-init installs the proxy CA and configures Docker to use the proxy. World
creation waits for this to finish before running the repository recipe. If the
proxy is down or the CA setup fails, world creation fails and removes the VM.

WT runs:

```text
devcontainer up --log-level debug --log-format text --workspace-folder /workspace
```

The command output is streamed live. After it finishes, WT prints cache hits,
misses, and byte counts seen during that command.

## Limits

- Public images only.
- Configured registry hosts only.
- The proxy CA is trusted by every world.
- The cache does not share containers, volumes, local images, or Docker build
  cache.
- Private registry credentials and private-image isolation are out of scope.
