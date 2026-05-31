# Smoke Tests

Hurl files for every public endpoint. Run against production:

```bash
hurl --variables-file tests/smoke/vars.env tests/smoke/*.hurl
```

Or against a local server:

```bash
hurl --variable base_url=http://localhost:3001 tests/smoke/*.hurl
```

The `vars.env` file defaults `base_url` to `https://go.pointe.dev`.
