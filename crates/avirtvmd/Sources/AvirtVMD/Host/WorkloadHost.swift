import Foundation

/// What a launcher needs to start the workload's VM.
struct LaunchSpec: Sendable {
  let workload: String
  let kernelPath: String
  let initfsPath: String
  let rootfsPath: String
  let imageConfigPath: String
  let environment: [String]
  let shares: [(host: String, guest: String)]
  let sockets: [(host: String, guest: String, mode: UInt32)]
  let stdio: StdioLog
}

/// A running workload container.
protocol VirtualWorkload: Sendable {
  /// Resolves with the container's exit code when it exits.
  func wait() async throws -> Int32
  /// Asks the container to stop.
  func terminate() async throws
  /// Stops the VM.
  func stop() async throws
}

/// Starts the VM for a workload.
typealias Launcher = @Sendable (LaunchSpec) async throws -> any VirtualWorkload

/// How a workload ended.
struct WorkloadOutcome: Sendable, Equatable {
  let code: Int32?
  let error: String
}

/// Why `start` didn't start a workload.
struct StartFailure: Error, CustomStringConvertible {
  let description: String
}

/// The workload's lifecycle: idle, starting, running, exited, or stopping.
actor WorkloadHost {
  enum Phase: Equatable {
    case idle
    case starting
    case running
    case exited(WorkloadOutcome)
    case stopping
  }

  private let workload: String
  private let launcher: Launcher
  private var phase = Phase.idle
  private var digest: String?
  private var virtual: (any VirtualWorkload)?
  private var stdio: StdioLog?
  private var startWaiters: [CheckedContinuation<Void, Never>] = []
  private var exitListeners: [UUID: AsyncStream<WorkloadOutcome>.Continuation] = [:]

  init(workload: String, launcher: @escaping Launcher) {
    self.workload = workload
    self.launcher = launcher
  }

  var current: Phase { phase }

  /// Starts the workload, or waits for the start that is under way.
  ///
  /// The same `digest` while starting or running answers once the workload runs. Another digest
  /// fails. Any failure leaves the host idle.
  func start(digest requested: String, _ prepare: @Sendable () throws -> LaunchSpec) async throws {
    switch phase {
    case .starting, .running:
      guard digest == requested else {
        throw StartFailure(description: "the workload already started with another spec")
      }
      if phase == .starting { await withCheckedContinuation { startWaiters.append($0) } }
      guard phase == .running else { throw StartFailure(description: "the workload didn't start") }
      return
    case .stopping, .exited:
      throw StartFailure(description: "the workload already ran")
    case .idle:
      break
    }
    phase = .starting
    digest = requested
    do {
      let spec = try prepare()
      stdio = spec.stdio
      let launched = try await launcher(spec)
      virtual = launched
      phase = .running
      watch(launched)
      resumeStartWaiters()
    } catch {
      phase = .idle
      digest = nil
      stdio?.close()
      stdio = nil
      resumeStartWaiters()
      throw error
    }
  }

  /// Stops the workload.
  ///
  /// During a start it waits for the start to settle, then stops the workload that launched. It sends
  /// SIGTERM, then forces a stop after the grace period and waits for the workload's exit, which closes the stdio log.
  /// Calling it again does nothing more.
  func stop(graceSeconds: UInt32) async {
    switch phase {
    case .idle, .exited:
      return
    case .starting:
      await withCheckedContinuation { startWaiters.append($0) }
      await stop(graceSeconds: graceSeconds)
      return
    case .stopping, .running:
      break
    }
    guard let virtual else { return }
    phase = .stopping
    try? await virtual.terminate()
    let exited = await withTaskGroup(of: Bool.self) { group in
      group.addTask {
        await self.waitForExit() != nil
      }
      group.addTask {
        try? await Task.sleep(for: .seconds(Int(graceSeconds)))
        return false
      }
      let first = await group.next() ?? false
      group.cancelAll()
      return first
    }
    if !exited {
      try? await virtual.stop()
      _ = await waitForExit()
    }
  }

  /// The outcome once the workload has exited, or nil if the caller's task is cancelled first.
  func waitForExit() async -> WorkloadOutcome? {
    for await outcome in exitStream() {
      return outcome
    }
    return nil
  }

  /// A stream that yields the outcome once the workload has exited, then finishes.
  func exitStream() -> AsyncStream<WorkloadOutcome> {
    let (stream, continuation) = AsyncStream.makeStream(of: WorkloadOutcome.self)
    if case .exited(let outcome) = phase {
      continuation.yield(outcome)
      continuation.finish()
      return stream
    }
    let id = UUID()
    exitListeners[id] = continuation
    continuation.onTermination = { _ in
      Task { await self.removeListener(id) }
    }
    return stream
  }

  private func removeListener(_ id: UUID) {
    exitListeners[id] = nil
  }

  private func watch(_ launched: any VirtualWorkload) {
    Task {
      var outcome = WorkloadOutcome(code: nil, error: "")
      do {
        outcome = WorkloadOutcome(code: try await launched.wait(), error: "")
      } catch {
        outcome = WorkloadOutcome(code: nil, error: "\(error)")
      }
      try? await launched.stop()
      finish(outcome)
    }
  }

  private func finish(_ outcome: WorkloadOutcome) {
    phase = .exited(outcome)
    stdio?.close()
    stdio = nil
    virtual = nil
    let listeners = exitListeners
    exitListeners = [:]
    for listener in listeners.values {
      listener.yield(outcome)
      listener.finish()
    }
  }

  private func resumeStartWaiters() {
    let waiters = startWaiters
    startWaiters = []
    for waiter in waiters { waiter.resume() }
  }
}

extension WorkloadOutcome {
  /// The outcome as the protocol reports it.
  var proto: Avirt_Vmd_V1_ExitStatus {
    var status = Avirt_Vmd_V1_ExitStatus()
    if let code { status.code = code }
    status.error = error
    return status
  }
}

extension WorkloadHost {
  /// The answer to a `Status` request.
  func reply() -> Avirt_Vmd_V1_StatusReply {
    var reply = Avirt_Vmd_V1_StatusReply()
    switch phase {
    case .idle: reply.state = .idle
    case .starting: reply.state = .starting
    case .running: reply.state = .running
    case .stopping: reply.state = .stopping
    case .exited(let outcome):
      reply.state = .exited
      reply.exit = outcome.proto
    }
    return reply
  }
}
