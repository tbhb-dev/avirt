//! The DNS resolver for `<name>.<ns>.svc.host.avirt.internal` and `<name>.<ns>.workload.host.avirt.internal`, which answers with the local front-door Envoy and, for peered names, with their host's front-door Envoy.

fn main() -> std::process::ExitCode {
    eprintln!("avirtdnsd: not implemented");
    std::process::ExitCode::FAILURE
}
