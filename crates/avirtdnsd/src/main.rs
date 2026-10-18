//! The DNS resolver for `<name>.<ns>.svc.host.avirt.internal` and `<name>.<ns>.workload.host.avirt.internal`, which answers with the local front-door Envoy or, for peered names, their host's front-door Envoy.

fn main() -> std::process::ExitCode {
    eprintln!("avirtdnsd: not implemented");
    std::process::ExitCode::FAILURE
}
