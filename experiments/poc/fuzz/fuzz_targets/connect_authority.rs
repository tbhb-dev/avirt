//! Fuzz target for the CONNECT authority parser: any accepted authority parses again from its rendering.

#![no_main]

use avirt_connect::Target;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(authority) = core::str::from_utf8(data) else { return };
    if let Some(target) = Target::parse(authority) {
        let rendered = if target.host.contains(':') {
            format!("[{}]:{}", target.host, target.port)
        } else {
            format!("{}:{}", target.host, target.port)
        };
        assert_eq!(Target::parse(&rendered), Some(target));
    }
});
