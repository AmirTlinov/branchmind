# Troubleshooting

## `cargo fmt --check` fails

```bash
make fmt
```

## `clippy` fails with warnings

- Fix warnings; CI treats warnings as errors.

## SQLite store issues

- If you see lock errors, ensure only one daemon owns the socket or stop the daemon and retry.
