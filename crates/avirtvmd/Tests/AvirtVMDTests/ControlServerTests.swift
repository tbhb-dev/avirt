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
}
