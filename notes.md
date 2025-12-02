Debug command:

```bash
cargo run --release -- --debug -c (readlink -f assets/test_config.toml ) test_a.js test_b.js
```

Added some debug print statements to figure out what node ID each node actually is
