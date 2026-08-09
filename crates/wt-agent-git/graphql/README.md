# Provider schemas

- `github/schema.graphql` comes from GitHub's public GraphQL schema.
- `gitlab/schema.json` comes from GitLab.com's production introspection schema
  without deprecated fields.

These files make query validation deterministic and credential-free. Updating
them is a deliberate source change; normal builds and tests never download or
introspect a provider schema.
