# koto

koto is a fast and minimal TUI todo manager.

Named after the Japanese **事** *(koto)* — meaning "thing" or "action".

## Usage

```bash
cargo run --bin koto            # start with an empty list
cargo run --bin koto -- --demo  # start with demo tasks
```

Key bindings:

- `j` / `k` or `↓` / `↑`: move selection
- `a` or `n`: enter add mode (type then Enter to add)
- `Enter` / `Space`: toggle completion
- `d` / `Delete`: delete selected
- `c`: clear all completed
- `r`: reload
- `q`: quit
