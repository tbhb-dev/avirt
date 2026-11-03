# avirt

avirt is a command-line tool written in Go. The repository is at the bootstrap stage: the binary prints only its usage and build metadata.

## Development

Bazel pins and runs every gate. [mise](https://mise.jdx.dev) only installs `bazelisk` and `uv`, pinned in `mise.toml` and locked in `mise.lock`. Run every gate with:

```sh
bazelisk test //...
```

`bazelisk run //:format` formats the tree in place, and `bazelisk run //:bazel_env` builds the shims for the pinned tools in `.bazel_env/bin`. Docs are in [docs/](docs/index.md).

## License

Apache-2.0. See `LICENSE`.
