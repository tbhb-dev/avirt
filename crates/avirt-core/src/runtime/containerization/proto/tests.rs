//! Unit tests for the control channel's messages against the golden encodings.

use super::{
    Event, EventKind, ExitStatus, Failure, HelperBody, HelperHello, HelperMessage, HostBody, HostHello, HostMessage,
    Refusal, RefusalCode, Request, RequestOp, Response, ResponseResult, Share, Socket, StartWorkload, Started, State,
    Status, StatusReply, Stop, Stopped, WorkloadExited,
};
use anyhow::{Result, ensure};
use prost::Message as _;
use std::fs;
use std::path::Path;

/// Reads a fixture from the Swift package at build time.
macro_rules! fixture {
    ($name:literal) => {
        include_bytes!(concat!("../../../../../avirtvmd/Tests/AvirtVMDTests/fixtures/", $name, ".bin")).as_slice()
    };
}

/// A message, its name, and the bytes the Swift package's fixtures hold for it.
struct Golden<M> {
    /// The fixture's bytes.
    bytes: &'static [u8],
    /// The message that must encode to the bytes.
    message: M,
    /// The fixture's filename, without the extension.
    name: &'static str,
}

/// A `StartWorkload` with every field filled in.
fn start_workload() -> StartWorkload {
    StartWorkload {
        container_dir: "/state/containers/1a2b3c4d".to_owned(),
        env: vec!["AVIRT_WORKLOAD_ID=1a2b3c4d".to_owned(), "AVIRT_PROMPT=pong".to_owned()],
        image_config_path: "/images/blobs/sha256/config".to_owned(),
        initfs_path: "/cache/initfs/initfs.ext4".to_owned(),
        kernel_path: "/cache/kernel/vmlinux".to_owned(),
        rootfs_base_path: "/cache/rootfs/rootfs.ext4".to_owned(),
        shares: vec![
            Share { guest_path: "/workspace".to_owned(), host_path: "/home/work".to_owned() },
            Share { guest_path: "/codex-home".to_owned(), host_path: "/state/scratch/codex".to_owned() },
        ],
        sockets: vec![
            Socket {
                guest_path: "/run/avirt/proxy.sock".to_owned(),
                host_path: "/state/workloads/1a2b3c4d/proxy.sock".to_owned(),
                mode: 0o600,
            },
            Socket {
                guest_path: "/run/avirt/mediator.sock".to_owned(),
                host_path: "/state/workloads/1a2b3c4d/mediator.sock".to_owned(),
                mode: 0o600,
            },
        ],
        spec_digest: "3a7bd3e2360a3d29eea436fcfb7e44c735d117c42d1c1835420b6b9942dd4f1b".to_owned(),
        stdio_log_path: "/logs/1a2b3c4d.stdio.ndjson".to_owned(),
    }
}

/// The messages avirthostd sends.
fn host_fixtures() -> Vec<Golden<HostMessage>> {
    let refusal = Refusal {
        code: RefusalCode::StaleIncarnation.into(),
        message: "the incarnation is not the one avirthostd started".to_owned(),
    };
    vec![
        Golden {
            bytes: fixture!("host-hello"),
            message: HostMessage::from(HostBody::Hello(HostHello { version: 1 })),
            name: "host-hello",
        },
        Golden {
            bytes: fixture!("host-refusal"),
            message: HostMessage::from(HostBody::Refusal(refusal)),
            name: "host-refusal",
        },
        Golden {
            bytes: fixture!("host-request-start-workload"),
            message: HostMessage::from(HostBody::Request(Box::new(Request::new(
                7,
                RequestOp::StartWorkload(Box::new(start_workload())),
            )))),
            name: "host-request-start-workload",
        },
        Golden {
            bytes: fixture!("host-request-status"),
            message: HostMessage::from(HostBody::Request(Box::new(Request::new(8, RequestOp::Status(Status))))),
            name: "host-request-status",
        },
        Golden {
            bytes: fixture!("host-request-stop"),
            message: HostMessage::from(HostBody::Request(Box::new(Request::new(
                9,
                RequestOp::Stop(Stop { grace_seconds: 5 }),
            )))),
            name: "host-request-stop",
        },
    ]
}

/// The messages avirtvmd sends.
fn helper_fixtures() -> Vec<Golden<HelperMessage>> {
    let hello = HelperHello {
        incarnation: (160u8..=175u8).collect(),
        max_version: 2,
        min_version: 1,
        workload_id: "1a2b3c4d".to_owned(),
    };
    let refusal = Refusal {
        code: RefusalCode::UnsupportedVersion.into(),
        message: "protocol version 3 isn't supported".to_owned(),
    };
    let exit = ExitStatus { code: Some(3i32), error: "the workload exited".to_owned(), signal: Some(15i32) };
    let status = StatusReply { exit: Some(exit), state: State::Exited.into() };
    let failure = Failure { message: "the root filesystem clone failed".to_owned() };
    let killed = ExitStatus { code: Some(0i32), error: "killed".to_owned(), signal: Some(9i32) };
    vec![
        Golden {
            bytes: fixture!("helper-hello"),
            message: HelperMessage::from(HelperBody::Hello(hello)),
            name: "helper-hello",
        },
        Golden {
            bytes: fixture!("helper-refusal"),
            message: HelperMessage::from(HelperBody::Refusal(refusal)),
            name: "helper-refusal",
        },
        Golden {
            bytes: fixture!("helper-response-started"),
            message: HelperMessage::from(HelperBody::Response(Response::new(7, ResponseResult::Started(Started)))),
            name: "helper-response-started",
        },
        Golden {
            bytes: fixture!("helper-response-status"),
            message: HelperMessage::from(HelperBody::Response(Response::new(8, ResponseResult::Status(status)))),
            name: "helper-response-status",
        },
        Golden {
            bytes: fixture!("helper-response-stopped"),
            message: HelperMessage::from(HelperBody::Response(Response::new(9, ResponseResult::Stopped(Stopped)))),
            name: "helper-response-stopped",
        },
        Golden {
            bytes: fixture!("helper-response-failure"),
            message: HelperMessage::from(HelperBody::Response(Response::new(7, ResponseResult::Failure(failure)))),
            name: "helper-response-failure",
        },
        Golden {
            bytes: fixture!("helper-event-workload-exited"),
            message: HelperMessage::from(HelperBody::Event(Event::from(EventKind::WorkloadExited(WorkloadExited {
                exit: Some(killed),
            })))),
            name: "helper-event-workload-exited",
        },
    ]
}

/// The fixture directory and the messages in this file list the same names.
///
/// # Errors
///
/// Fails when the fixture directory is unreadable or the two lists differ.
#[test]
fn every_fixture_has_a_message() -> Result<()> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../avirtvmd/Tests/AvirtVMDTests/fixtures");
    let mut on_disk: Vec<String> = Vec::new();
    for entry in fs::read_dir(dir)? {
        let name = entry?.file_name().to_string_lossy().into_owned();
        on_disk.extend(name.strip_suffix(".bin").map(str::to_owned));
    }
    on_disk.sort();
    let mut covered: Vec<String> = host_fixtures().iter().map(|golden| golden.name.to_owned()).collect();
    covered.extend(helper_fixtures().iter().map(|golden| golden.name.to_owned()));
    covered.sort();
    ensure!(on_disk == covered, "fixtures on disk {on_disk:?} differ from the messages {covered:?}");
    Ok(())
}

/// Each message encodes to its golden bytes and decodes back.
///
/// # Errors
///
/// Fails when an encoding differs from its fixture or a fixture doesn't decode to its message.
#[test]
fn messages_encode_to_the_golden_bytes_and_decode_back() -> Result<()> {
    for golden in host_fixtures() {
        ensure!(golden.message.encode_to_vec() == golden.bytes, "{} encodes differently", golden.name);
        ensure!(HostMessage::decode(golden.bytes)? == golden.message, "{} decodes differently", golden.name);
    }
    for golden in helper_fixtures() {
        ensure!(golden.message.encode_to_vec() == golden.bytes, "{} encodes differently", golden.name);
        ensure!(HelperMessage::decode(golden.bytes)? == golden.message, "{} decodes differently", golden.name);
    }
    Ok(())
}

/// A message converts to its body and back.
///
/// # Errors
///
/// Fails when a body changes in the conversion.
#[test]
fn bodies_convert_to_messages_and_back() -> Result<()> {
    let hello = HostBody::Hello(HostHello { version: 1 });
    ensure!(HostMessage::from(hello.clone()).into_body() == Some(hello), "a host body changed");
    let result = ResponseResult::Stopped(Stopped);
    ensure!(Response::new(4, result.clone()).into_result() == Some(result), "a result changed");
    ensure!(HelperMessage::decode(&[][..])?.into_body().is_none(), "an empty message has a body");
    Ok(())
}
