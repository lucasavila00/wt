# wt-world

Shared domain identity for WT worlds.

`WorldId` is the immutable UUID for a world. It names the registry row, the
world disk, and cross-component world references. A world name is deliberately
not defined here because it is mutable and validated by the control protocol.
