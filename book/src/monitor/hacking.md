# Hacking on the monitor locally

Easy but slow way:

```
$ nix develop -c sudo [RUST_BACKTRACE=1] monitor
```

Harder but faster way:

```
$ export RUSTFLAGS=-Clink-arg=-fuse-ld=mold
$ cargo build
$ sudo [RUST_BACKTRACE=1] IMAGE_DEPS_DIR=$(nix build --print-out-paths .\#image-deps) LIB_MONITOR_DIR=. target/debug/monitor
```
