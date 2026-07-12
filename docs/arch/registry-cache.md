# Registry cache

The KVM host runs one registry proxy for all worlds. It shares public image
blobs, not Docker state.

```text
world Docker ──► host cache ──► public registry
```

```toml
[registry_cache]
state_dir = "/var/lib/wt/registry-cache"
port = 3128
max_size_gib = 64
registries = ["docker.io", "mcr.microsoft.com"]
```

| Setting | Meaning |
|---------|---------|
| `state_dir` | Cached blobs and proxy CA |
| `port` | Proxy port on the libvirt bridge |
| `max_size_gib` | Cache limit |
| `registries` | Registry hosts to cache |

Setup starts and verifies the pinned proxy, then makes its CA available to world
creation. Cloud-init installs the CA and configures Docker.

Tags and manifests are checked upstream. Cached immutable blobs are reused.
Unlisted registries and pushes bypass the cache.

Each world retains separate containers, volumes, networks, writable layers,
local images, and build cache. Private-image caching is unsupported.

Parent: [architecture](./README.md).
