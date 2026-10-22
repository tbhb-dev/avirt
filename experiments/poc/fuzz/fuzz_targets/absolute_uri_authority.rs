//! Fuzz target for the absolute-form request target reader: it must return without panicking.

#![no_main]

use avirt_connect::absolute_uri_authority;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(target) = core::str::from_utf8(data) {
        let _authority = absolute_uri_authority(target);
    }
});
