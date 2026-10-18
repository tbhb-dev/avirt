//! The DNS resolver for `<name>.<ns>.svc.host.avirt.internal` and `<name>.<ns>.workload.host.avirt.internal`, which answers with the local avirtedgeproxyd or, for peered names, their host's avirtedgeproxyd.

fn main() -> std::process::ExitCode {
    eprintln!("avirtdnsd: not implemented");
    std::process::ExitCode::FAILURE
}
