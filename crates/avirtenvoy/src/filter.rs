//! The filter resolves once and checks the entire answer. It pins the DFP authority to a checked IP.
use std::cell::RefCell;
use std::io::Write;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Duration, Instant};

use envoy_proxy_dynamic_modules_rust_sdk as sdk;
use sdk::abi::envoy_dynamic_module_type_attribute_id as Attribute;
use sdk::abi::envoy_dynamic_module_type_on_http_filter_request_headers_status as Headers;
use sdk::{EnvoyHttpFilter, EnvoyHttpFilterConfig, EnvoyHttpFilterScheduler, HttpFilter, HttpFilterConfig};

use crate::stream::Stream;
use avirt_connect::{self as policy, Target};

// With the SDK handling the FFI boundary, this code doesn't need handwritten unsafe code.
sdk::declare_init_functions!(init, config);

fn init() -> bool {
    LazyLock::force(&RESOLVER).is_ok()
}

fn config<EC: EnvoyHttpFilterConfig, EHF: EnvoyHttpFilter>(
    _envoy: &mut EC,
    name: &str,
    _config: &[u8],
) -> Option<Box<dyn HttpFilterConfig<EHF>>> {
    (name == "avirt-connect").then(|| Box::new(Config) as Box<dyn HttpFilterConfig<EHF>>)
}

// DNS work stays off Envoy's workers. The blocking resolver pool keeps its size limit even when
// the system resolver takes longer than the per-request deadline to return.
static RESOLVER: LazyLock<std::io::Result<tokio::runtime::Runtime>> = LazyLock::new(|| {
    tokio::runtime::Builder::new_multi_thread().worker_threads(1).max_blocking_threads(128).enable_all().build()
});

struct Config;
impl<EHF: EnvoyHttpFilter> HttpFilterConfig<EHF> for Config {
    fn new_http_filter(&self, _envoy: &mut EHF) -> Box<dyn HttpFilter<EHF>> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        Box::new(Filter {
            id: NEXT.fetch_add(1, Ordering::Relaxed),
            stream: RefCell::new(None),
            resolved: Arc::default(),
        })
    }
}

type Resolution = Result<SocketAddr, &'static str>;
struct Filter {
    id: u64,
    stream: RefCell<Option<Stream>>,
    resolved: Arc<Mutex<Option<Resolution>>>,
}

impl Filter {
    fn emit(lines: Vec<String>) {
        let mut out = std::io::stdout().lock();
        for line in lines {
            let _ = writeln!(out, "{line}");
        }
    }

    fn refuse<EHF: EnvoyHttpFilter>(&self, envoy: &mut EHF, status: u32, rule: &str, error: Option<&str>) {
        if let Some(stream) = self.stream.borrow_mut().as_mut() {
            Self::emit(stream.on_refused(rule, error, Instant::now()));
        }
        envoy.send_response(status, &[], Some(rule.as_bytes()), Some(rule));
    }
}

fn header<EHF: EnvoyHttpFilter>(envoy: &EHF, key: &str) -> String {
    envoy.get_request_header_value(key).map(|v| String::from_utf8_lossy(v.as_slice()).into_owned()).unwrap_or_default()
}

impl<EHF: EnvoyHttpFilter> HttpFilter<EHF> for Filter {
    fn on_request_headers(&self, envoy: &mut EHF, _end: bool) -> Headers {
        let method = header(envoy, ":method");
        let authority = header(envoy, ":authority");
        let target = Target::parse(&authority);
        let (stream, lines) =
            Stream::on_headers(self.id, &method, &authority, &header(envoy, ":scheme"), Instant::now());
        Self::emit(lines);
        let refusal = stream.refusal();
        *self.stream.borrow_mut() = Some(stream);
        if let Some((status, rule)) = refusal {
            envoy.send_response(status, &[], Some(rule.as_bytes()), Some(rule));
            return Headers::StopIteration;
        }
        let target = target.expect("validated authority");
        let result = Arc::clone(&self.resolved);
        let scheduler = envoy.new_scheduler();
        let Ok(runtime) = &*RESOLVER else {
            self.refuse(envoy, 502, "allow-all", Some("dns"));
            return Headers::StopIteration;
        };
        runtime.spawn(async move {
            let answer = resolve(&target).await;
            *result.lock().expect("resolution lock") = Some(answer);
            scheduler.commit(0);
        });
        Headers::StopAllIterationAndWatermark
    }

    fn on_scheduled(&self, envoy: &mut EHF, _id: u64) {
        let answer = self.resolved.lock().expect("resolution lock").take();
        match answer {
            Some(Ok(address)) => {
                // Don't let the DFP perform a second hostname lookup (DNS rebinding).
                if envoy.set_request_header(":authority", address.to_string().as_bytes()) {
                    if let Some(stream) = self.stream.borrow_mut().as_mut() {
                        Self::emit(stream.on_allowed(Instant::now()));
                    }
                    envoy.continue_decoding();
                } else {
                    self.refuse(envoy, 502, "bad-request", Some("authority rewrite"));
                }
            }
            Some(Err("host addresses")) => self.refuse(envoy, 403, "private-dest", Some("host addresses")),
            Some(Err("private-dest")) => self.refuse(envoy, 403, "private-dest", None),
            _ => self.refuse(envoy, 502, "allow-all", Some("dns")),
        }
    }

    fn on_request_body(
        &self,
        envoy: &mut EHF,
        _end: bool,
    ) -> sdk::abi::envoy_dynamic_module_type_on_http_filter_request_body_status {
        if let Some(stream) = self.stream.borrow_mut().as_mut() {
            Self::emit(stream.on_bytes(envoy.get_received_request_body_size() as u64, 0, Instant::now()));
        }
        sdk::abi::envoy_dynamic_module_type_on_http_filter_request_body_status::Continue
    }

    fn on_response_body(
        &self,
        envoy: &mut EHF,
        _end: bool,
    ) -> sdk::abi::envoy_dynamic_module_type_on_http_filter_response_body_status {
        if let Some(stream) = self.stream.borrow_mut().as_mut() {
            Self::emit(stream.on_bytes(0, envoy.get_received_response_body_size() as u64, Instant::now()));
        }
        sdk::abi::envoy_dynamic_module_type_on_http_filter_response_body_status::Continue
    }

    fn on_stream_complete(&self, envoy: &mut EHF) {
        if let Some(stream) = self.stream.borrow_mut().as_mut() {
            Self::emit(stream.on_complete(envoy.get_attribute_int(Attribute::ResponseCode), Instant::now()));
        }
    }
}

async fn resolve(target: &Target) -> Resolution {
    let addresses: Vec<_> =
        tokio::time::timeout(Duration::from_secs(10), tokio::net::lookup_host((target.host.as_str(), target.port)))
            .await
            .map_err(|_| "dns")?
            .map_err(|_| "dns")?
            .collect();
    let address = addresses.first().copied().ok_or("dns")?;
    let own = policy::host_addresses();
    if !policy::destination_allowed(&addresses.iter().map(SocketAddr::ip).collect::<Vec<_>>(), &own) {
        return Err(if own.is_err() { "host addresses" } else { "private-dest" });
    }
    Ok(address)
}
