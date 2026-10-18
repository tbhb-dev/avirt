import Foundation
import NIOCore
import NIOPosix
import Synchronization

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
  _ body: (_ socket: String, _ group: MultiThreadedEventLoopGroup, _ stops: Counter) async throws -> Void
) async throws {
  let scratch = try ScratchDirectory()
  defer { scratch.remove() }
  let group = MultiThreadedEventLoopGroup(numberOfThreads: 2)
  let socket = scratch.path + "/v.sock"
  let listener = try await ControlServer.bind(path: socket, group: group)
  let stops = Counter()
  let server = ControlServer(identity: testIdentity, userID: userID, requestStop: { stops.increment() })
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
