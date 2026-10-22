import Foundation
import NIOCore
import NIOPosix
import Testing

@testable import AvirtVMD

struct ControlServerTests {
  @Test("A connection from the same user gets the hello and has its requests answered")
  func sameUserIsServed() async throws {
    try await withServer { socket, group, _ in
      let client = try await connect(to: socket, group: group)
      try await client.executeThenClose { inbound, outbound in
        var frames = inbound.makeAsyncIterator()
        let hello = try #require(await frames.nextHelperMessage())
        guard case .hello(let presented)? = hello.body else {
          Issue.record("the first frame wasn't a hello")
          return
        }
        #expect(presented.minVersion == 1)
        #expect(presented.maxVersion == 1)
        #expect(presented.workloadID == "1a2b3c4d")
        #expect([UInt8](presented.incarnation) == testIdentity.incarnation.bytes)

        try await outbound.write(hostMessage { $0.hello = .with { $0.version = 1 } })
        try await outbound.write(
          hostMessage {
            $0.request = .with {
              $0.id = 41
              $0.status = .init()
            }
          })
        let answer = try #require(await frames.nextHelperMessage())
        guard case .response(let response)? = answer.body, case .status(let status)? = response.result else {
          Issue.record("the answer wasn't a status response")
          return
        }
        #expect(response.id == 41)
        #expect(status.state == .idle)
      }
    }
  }

  @Test("A peer that meets the requirement gets the hello")
  func aPeerMeetingTheRequirementGetsTheHello() async throws {
    let policy = PeerPolicy.signed(try PeerRequirement(try ownCdhashRequirementText()))
    try await withServer(policy: policy) { socket, group, _ in
      let client = try await connect(to: socket, group: group)
      try await client.executeThenClose { inbound, _ in
        var frames = inbound.makeAsyncIterator()
        let first = try #require(await frames.nextHelperMessage())
        guard case .hello? = first.body else {
          Issue.record("the first frame wasn't a hello")
          return
        }
      }
    }
  }

  @Test("A peer that fails the requirement is refused before any hello")
  func aPeerFailingTheRequirementIsRefusedBeforeAnyHello() async throws {
    let policy = PeerPolicy.signed(try PeerRequirement(hostRequirementText(team: "ABCDE12345")))
    try await withServer(policy: policy) { socket, group, _ in
      let client = try await connect(to: socket, group: group)
      try await client.executeThenClose { inbound, _ in
        var frames = inbound.makeAsyncIterator()
        let first = try #require(await frames.nextHelperMessage())
        guard case .refusal(let refusal)? = first.body else {
          Issue.record("the first frame wasn't a refusal")
          return
        }
        #expect(refusal.code == .wrongPeer)
        #expect(await frames.nextHelperMessage() == nil)
      }
    }
  }

  @Test("A hello with a version outside the helper's range is refused and the connection closes")
  func unsupportedVersion() async throws {
    try await withServer { socket, group, _ in
      let client = try await connect(to: socket, group: group)
      try await client.executeThenClose { inbound, outbound in
        var frames = inbound.makeAsyncIterator()
        _ = try #require(await frames.nextHelperMessage())
        try await outbound.write(hostMessage { $0.hello = .with { $0.version = 2 } })
        let answer = try #require(await frames.nextHelperMessage())
        guard case .refusal(let refusal)? = answer.body else {
          Issue.record("the answer wasn't a refusal")
          return
        }
        #expect(refusal.code == .unsupportedVersion)
        #expect(await frames.nextHelperMessage() == nil)
      }
    }
  }

  @Test("A connection from another user is closed before any hello")
  func otherUserIsRefused() async throws {
    try await withServer(userID: Sys.currentUID() &+ 1) { socket, group, _ in
      let client = try await connect(to: socket, group: group)
      try await client.executeThenClose { inbound, _ in
        var frames = inbound.makeAsyncIterator()
        #expect(await frames.nextHelperMessage() == nil)
      }
    }
  }

  @Test("A request before the hello is answered is not served")
  func requestWithoutHello() async throws {
    try await withServer { socket, group, _ in
      let client = try await connect(to: socket, group: group)
      try await client.executeThenClose { inbound, outbound in
        var frames = inbound.makeAsyncIterator()
        _ = try #require(await frames.nextHelperMessage())
        try await outbound.write(
          hostMessage {
            $0.request = .with {
              $0.id = 1
              $0.status = .init()
            }
          })
        #expect(await frames.nextHelperMessage() == nil)
      }
    }
  }

  @Test("A connection that completes the handshake replaces the previous one")
  func newConnectionReplacesTheOld() async throws {
    try await withServer { socket, group, _ in
      let first = try await connect(to: socket, group: group)
      try await first.executeThenClose { firstInbound, firstOutbound in
        var firstFrames = firstInbound.makeAsyncIterator()
        _ = try #require(await firstFrames.nextHelperMessage())
        try await firstOutbound.write(hostMessage { $0.hello = .with { $0.version = 1 } })

        let second = try await connect(to: socket, group: group)
        try await second.executeThenClose { secondInbound, secondOutbound in
          var secondFrames = secondInbound.makeAsyncIterator()
          _ = try #require(await secondFrames.nextHelperMessage())
          try await secondOutbound.write(hostMessage { $0.hello = .with { $0.version = 1 } })
          try await secondOutbound.write(
            hostMessage {
              $0.request = .with {
                $0.id = 2
                $0.status = .init()
              }
            })
          #expect(await secondFrames.nextHelperMessage() != nil)
          #expect(await firstFrames.nextHelperMessage() == nil)
        }
      }
    }
  }

  @Test("Stop in the idle state is answered, then the helper is told to stop")
  func stopWhenIdle() async throws {
    try await withServer { socket, group, stops in
      let client = try await connect(to: socket, group: group)
      try await client.executeThenClose { inbound, outbound in
        var frames = inbound.makeAsyncIterator()
        _ = try #require(await frames.nextHelperMessage())
        try await outbound.write(hostMessage { $0.hello = .with { $0.version = 1 } })
        try await outbound.write(
          hostMessage {
            $0.request = .with {
              $0.id = 3
              $0.stop = .with { $0.graceSeconds = 5 }
            }
          })
        let answer = try #require(await frames.nextHelperMessage())
        guard case .response(let response)? = answer.body, case .stopped? = response.result else {
          Issue.record("the answer wasn't a stopped response")
          return
        }
        #expect(response.id == 3)
      }
      #expect(await eventually { stops.value == 1 })
    }
  }

  @Test("A request to start a workload fails while no VM is available")
  func startWorkloadFails() async throws {
    try await withServer { socket, group, _ in
      let client = try await connect(to: socket, group: group)
      try await client.executeThenClose { inbound, outbound in
        var frames = inbound.makeAsyncIterator()
        _ = try #require(await frames.nextHelperMessage())
        try await outbound.write(hostMessage { $0.hello = .with { $0.version = 1 } })
        try await outbound.write(
          hostMessage {
            $0.request = .with {
              $0.id = 4
              $0.startWorkload = .init()
            }
          })
        let answer = try #require(await frames.nextHelperMessage())
        guard case .response(let response)? = answer.body, case .failure? = response.result else {
          Issue.record("the answer wasn't a failure")
          return
        }
        #expect(response.id == 4)
      }
    }
  }

  @Test("A started workload is cloned, announced, and reported with its exit")
  func startThenExit() async throws {
    let scratch = try ScratchDirectory()
    defer { scratch.remove() }
    try Data("base image".utf8).write(to: URL(fileURLWithPath: scratch.path + "/base.ext4"))
    let workload = FakeWorkload(exitOnTerminate: true)
    try await withServer(
      launcher: { _ in workload },
      { socket, group, _ in
        let client = try await connect(to: socket, group: group)
        try await client.executeThenClose { inbound, outbound in
          var frames = inbound.makeAsyncIterator()
          _ = try #require(await frames.nextHelperMessage())
          try await outbound.write(hostMessage { $0.hello = .with { $0.version = 1 } })
          try await outbound.write(
            hostMessage {
              $0.request = .with {
                $0.id = 5
                $0.startWorkload = .with {
                  $0.specDigest = "digest"
                  $0.rootfsBasePath = scratch.path + "/base.ext4"
                  $0.containerDir = scratch.path + "/container"
                  $0.stdioLogPath = scratch.path + "/stdio.ndjson"
                }
              }
            })
          let started = try #require(await frames.nextHelperMessage())
          guard case .response(let response)? = started.body, case .started? = response.result else {
            Issue.record("the answer wasn't started: \(String(describing: started.body))")
            return
          }
          let attributes = try FileManager.default.attributesOfItem(atPath: scratch.path + "/container")
          #expect(attributes[.posixPermissions] as? Int == 0o700)
          #expect(
            try String(contentsOfFile: scratch.path + "/container/rootfs.ext4", encoding: .utf8) == "base image")

          workload.exit(with: 7)
          let event = try #require(await frames.nextHelperMessage())
          guard case .event(let exited)? = event.body else {
            Issue.record("the next frame wasn't an event")
            return
          }
          #expect(exited.workloadExited.exit.code == 7)

          try await outbound.write(
            hostMessage {
              $0.request = .with {
                $0.id = 6
                $0.status = .init()
              }
            })
          let status = try #require(await frames.nextHelperMessage())
          guard case .response(let reply)? = status.body, case .status(let report)? = reply.result else {
            Issue.record("the answer wasn't a status")
            return
          }
          #expect(report.state == .exited)
          #expect(report.exit.code == 7)
        }
      })
  }
}
