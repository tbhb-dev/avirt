import Foundation
import NIOCore
import NIOPosix
import Security
import Synchronization
import Testing

@testable import AvirtVMD

/// A directory short enough to hold a Unix socket path.
struct ScratchDirectory {
  let path: String

  init() throws {
    path = NSTemporaryDirectory() + "avirtvmd-" + String(UInt32.random(in: 0...UInt32.max), radix: 16)
    try FileManager.default.createDirectory(
      atPath: path, withIntermediateDirectories: false, attributes: [.posixPermissions: 0o700])
  }

  func remove() {
    try? FileManager.default.removeItem(atPath: path)
  }
}

final class Counter: Sendable {
  private let count = Mutex(0)

  func increment() {
    count.withLock { $0 += 1 }
  }

  var value: Int {
    count.withLock { $0 }
  }
}

let testIdentity = Identity(
  workload: "1a2b3c4d",
  incarnation: Incarnation(bytes: Array(0...15))
)

/// A launcher for tests that never start a VM.
let refusingLauncher: Launcher = { _ in throw StartFailure(description: "no VM in this test") }

/// Connects a client that frames messages the way the helper does.
func connect(to socket: String, group: MultiThreadedEventLoopGroup) async throws -> ControlChannel {
  try await ClientBootstrap(group: group).connect(unixDomainSocketPath: socket) { channel in
    channel.eventLoop.makeCompletedFuture {
      try channel.pipeline.syncOperations.addHandlers(
        ByteToMessageHandler(FrameDecoder()),
        MessageToByteHandler(FrameEncoder())
      )
      return try ControlChannel(wrappingChannelSynchronously: channel)
    }
  }
}

/// Runs `body` against a control server bound in a scratch directory.
func withServer(
  userID: UInt32 = Sys.currentUID(),
  launcher: @escaping Launcher = refusingLauncher,
  _ body: (_ socket: String, _ group: MultiThreadedEventLoopGroup, _ stops: Counter) async throws -> Void
) async throws {
  let scratch = try ScratchDirectory()
  defer { scratch.remove() }
  let group = MultiThreadedEventLoopGroup(numberOfThreads: 2)
  let socket = scratch.path + "/v.sock"
  let listener = try await ControlServer.bind(path: socket, group: group)
  let stops = Counter()
  let host = WorkloadHost(workload: testIdentity.workload, launcher: launcher)
  let server = ControlServer(
    identity: testIdentity, host: host, userID: userID, requestStop: { stops.increment() })
  let serving = Task { try await server.serve(on: listener) }
  do {
    try await body(socket, group, stops)
  } catch {
    serving.cancel()
    try? await group.shutdownGracefully()
    throw error
  }
  serving.cancel()
  _ = try? await serving.value
  try await group.shutdownGracefully()
}

extension NIOAsyncChannelInboundStream.AsyncIterator where Element == ByteBuffer {
  /// The next frame as a message from the helper, or nil once the connection is closed.
  mutating func nextHelperMessage() async -> HelperMessage? {
    guard let frame = (try? await next()) ?? nil else { return nil }
    return try? HelperMessage(frameBody: frame)
  }
}

func hostMessage(_ build: (inout HostMessage) -> Void) throws -> ByteBuffer {
  var message = HostMessage()
  build(&message)
  return try message.frameBody()
}

/// Waits up to `seconds` for `condition` to hold.
func eventually(seconds: Double = 5, _ condition: () -> Bool) async -> Bool {
  let deadline = Date().addingTimeInterval(seconds)
  while Date() < deadline {
    if condition() { return true }
    try? await Task.sleep(for: .milliseconds(20))
  }
  return condition()
}

/// A requirement that only the test process's own code satisfies: its code directory hash.
func ownCdhashRequirementText() throws -> String {
  let information = try Sys.signingInformation(of: try Sys.selfCode())
  guard let unique = information[kSecCodeInfoUnique as String] as? Data else {
    throw SignatureError.security("read the test process's code directory hash", errSecParam)
  }
  return "cdhash H\"" + unique.map { ($0 < 16 ? "0" : "") + String($0, radix: 16) }.joined() + "\""
}

/// The audit token that the helper reads for a connection from the test process.
func ownAuditToken() async throws -> audit_token_t {
  let scratch = try ScratchDirectory()
  defer { scratch.remove() }
  let group = MultiThreadedEventLoopGroup(numberOfThreads: 1)
  let socket = scratch.path + "/v.sock"
  let listener = try await ControlServer.bind(path: socket, group: group)
  do {
    let token = try await listener.executeThenClose { connections in
      let client = try await connect(to: socket, group: group)
      return try await client.executeThenClose { _, _ in
        var accepted = connections.makeAsyncIterator()
        let connection = try #require(try await accepted.next())
        return try await connection.executeThenClose { _, _ in
          try await peerAuditToken(of: connection.channel)
        }
      }
    }
    try await group.shutdownGracefully()
    return token
  } catch {
    try? await group.shutdownGracefully()
    throw error
  }
}
