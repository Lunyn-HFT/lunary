# Lunary

<a href="https://lunyn.com"><img src="https://lunyn.com/lunarybench.png?" alt="Lunyn Banner" width="1280"></a>

Lunary is a high-performance ITCH (NASDAQ TotalView-ITCH) parser built by Lunyn to address limitations in traditional market data infrastructure through optimized parsing for low-latency, high-reliability features. Even on scalar-only hardware without SIMD support, Lunary reliably processes millions of messages across modes like adaptive batching and parallel processing.


## Quickstart

```bash
cargo build --release
./target/release/itch-bench /path/to/itch
```

## License

AGPL - GNU Affero General Public License