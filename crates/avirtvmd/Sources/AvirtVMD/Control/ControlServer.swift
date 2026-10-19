import Foundation
import NIOCore
import NIOPosix

typealias ControlChannel = NIOAsyncChannel<ByteBuffer, ByteBuffer>

/// How long a peer has to answer the helper's hello.
private let handshakeTimeout = Duration.seconds(5)

/// Who this helper is, as it tells every peer.
struct Identity: Sendable {
  let workload: String
  let incarnation: Incarnation
}

/// Keeps the one connection that completed the handshake most recently.
private actor ConnectionSlot {
  private var closeCurrent: (@Sendable () async -> Void)?

  /// Makes the caller's connection the current one and closes the one it replaces.
  func claim(closing close: @escaping @Sendable () async -> Void) async {
    let previous = closeCurrent
    closeCurrent = close
    await previous?()
  }
}

/// Serves the control channel on `v.sock`: the handshake, then requests and answers.
struct ControlServer: Sendable {
  let identity: Identity
  /// The workload's lifecycle, which the requests drive.
  let host: WorkloadHost
  /// Called after the answer to a `Stop` request is written.
  let requestStop: @Sendable () -> Void
  private let slot = ConnectionSlot()
  private let userID: UInt32

  init(
    identity: Identity, host: WorkloadHost, userID: UInt32 = Sys.currentUID(),
    requestStop: @escaping @Sendable () -> Void
  ) {
    self.identity = identity
    self.host = host
    self.userID = userID
    self.requestStop = requestStop
  }

  /// Binds the Unix socket at `path`.
  ///
  /// Fails if something already exists there.
  static func bind(path: String, group: MultiThreadedEventLoopGroup) async throws -> NIOAsyncChannel<
    ControlChannel, Never
  > {
    try await ServerBootstrap(group: group).bind(unixDomainSocketPath: path) { channel in
      channel.eventLoop.makeCompletedFuture {
        try channel.pipeline.syncOperations.addHandlers(
          ByteToMessageHandler(FrameDecoder()),
          MessageToByteHandler(FrameEncoder())
        )
        return try ControlChannel(wrappingChannelSynchronously: channel)
      }
    }
  }

  /// Accepts connections until the task is cancelled or the listener fails.
  func serve(on listener: NIOAsyncChannel<ControlChannel, Never>) async throws {
    try await listener.executeThenClose { connections in
      try await withThrowingDiscardingTaskGroup { group in
        for try await connection in connections {
          group.addTask { await handle(connection) }
        }
      }
    }
  }

  private func handle(_ connection: ControlChannel) async {
    do {
      try await connection.executeThenClose { inbound, outbound in
        try await converse(inbound: inbound, outbound: outbound, channel: connection.channel)
      }
    } catch {
      log("connection ended: \(error)")
    }
  }

  private func converse(
    inbound: NIOAsyncChannelInboundStream<ByteBuffer>,
    outbound: NIOAsyncChannelOutboundWriter<ByteBuffer>,
    channel: any Channel
  ) async throws {
    guard try await peerUID(of: channel) == userID else {
      log("refused a connection from another user")
      return
    }
    var hello = HelperMessage()
    hello.hello = .with {
      $0.minVersion = minProtocolVersion
      $0.maxVersion = maxProtocolVersion
      $0.workloadID = identity.workload
      $0.incarnation = Data(identity.incarnation.bytes)
    }
    try await outbound.write(hello.frameBody())

    let watchdog = Task {
      try await Task.sleep(for: handshakeTimeout)
      try await channel.close()
    }
    defer { watchdog.cancel() }
    var frames = inbound.makeAsyncIterator()
    guard let first = try await frames.next() else { return }
    watchdog.cancel()
    guard case .hello(let reply)? = try HostMessage(frameBody: first).body else {
      log("the peer didn't answer the hello with a hello")
      return
    }
    guard (minProtocolVersion...maxProtocolVersion).contains(reply.version) else {
      var refusal = HelperMessage()
      refusal.refusal = .with {
        $0.code = .unsupportedVersion
        $0.message = "protocol version \(reply.version) isn't supported; this helper speaks \(maxProtocolVersion)"
      }
      try await outbound.write(refusal.frameBody())
      return
    }
    await slot.claim { try? await channel.close() }

    try await withThrowingTaskGroup(of: Void.self) { group in
      group.addTask { await announceExit(through: outbound) }
      while let frame = try await frames.next() {
        guard case .request(let request)? = try HostMessage(frameBody: frame).body else {
          log("the peer sent something other than a request")
          break
        }
        group.addTask { try await serve(request, through: outbound) }
      }
      group.cancelAll()
    }
  }

  /// Sends an `Event` when the workload exits, or at once if it already has.
  private func announceExit(through outbound: NIOAsyncChannelOutboundWriter<ByteBuffer>) async {
    guard let outcome = await host.waitForExit() else { return }
    var event = HelperMessage()
    event.event = .with {
      $0.workloadExited = .with { $0.exit = outcome.proto }
    }
    try? await outbound.write(event.frameBody())
  }

  /// Answers one request.
  ///
  /// Answering `Stop` also ends the helper.
  private func serve(
    _ request: Request, through outbound: NIOAsyncChannelOutboundWriter<ByteBuffer>
  ) async throws {
    let (response, stopping) = await answer(request)
    var message = HelperMessage()
    message.response = response
    try await outbound.write(message.frameBody())
    if stopping {
      outbound.finish()
      requestStop()
    }
  }

  /// The answer to one request, and whether it ends the helper.
  func answer(_ request: Request) async -> (response: Response, stopping: Bool) {
    var response = Response()
    response.id = request.id
    switch request.op {
    case .startWorkload(let start)?:
      do {
        try await host.start(digest: start.specDigest) {
          try LaunchSpec(request: start, workload: identity.workload)
        }
        response.started = .init()
      } catch {
        response.failure = .with { $0.message = "\(error)" }
      }
    case .status?:
      response.status = await host.reply()
    case .stop(let stop)?:
      await host.stop(graceSeconds: stop.graceSeconds)
      response.stopped = .init()
      return (response, true)
    case nil:
      response.failure = .with { $0.message = "the request names no operation" }
    }
    return (response, false)
  }
}

func log(_ message: String) {
  FileHandle.standardError.write(Data("avirtvmd: \(message)\n".utf8))
}
