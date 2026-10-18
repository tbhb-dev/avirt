//! The edge proxy supervises the host's edge Envoy with libavirtenvoy and serves it local xDS and SDS. It's the only component that opens connections off the host, and the policy enforcement point for traffic between workloads and between hosts.

fn main() -> std::process::ExitCode {
    eprintln!("avirtedgeproxyd: not implemented");
    std::process::ExitCode::FAILURE
}
