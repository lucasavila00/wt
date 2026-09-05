# WT API contract

`api.ts` is the public TypeScript definition for `wt api`. The command also prints
this definition in its help output. WT generates its Rust request and response
types from it with `npm run generate:api`; `make check-api` detects drift.

Consumers clone a pinned WT commit and generate their own clients and JSON Schemas.
WT does not ship a language-specific client or a JSON Schema distribution.

`exec_world` accepts an absolute executable, argv, and UTF-8 stdin, executed as the
world's guest user. It returns stdout, stderr, and exit status. It is the generic
hook for installing and starting controller-owned runtimes: invoke a guest
supervisor for long-running work, then poll it through subsequent calls.

The execution deadline is 60 seconds, input and arguments together are limited to
1 MiB and 256 arguments, and each output stream is limited to 16 MiB. A transport
failure has an unknown execution outcome. WT never replays command requests.
