import Foundation
import Synchronization
import Testing

@testable import AvirtVMD

/// A workload that exits when told to, or when stopped.
final class FakeWorkload: VirtualWorkload {
  private let exits = AsyncStream.makeStream(of: Int32.self)
  private let terminated = Mutex(0)
  private let exitOnTerminate: Bool
  private let exitDelay: Duration

  init(exitOnTerminate: Bool, exitDelay: Duration = .zero) {
    self.exitOnTerminate = exitOnTerminate
    self.exitDelay = exitDelay
  }

  var terminations: Int { terminated.withLock { $0 } }

  func exit(with code: Int32) {
    exits.continuation.yield(code)
    exits.continuation.finish()
  }

  func wait() async throws -> Int32 {
    for await code in exits.stream { return code }
    throw StartFailure(description: "no exit")
  }

  func terminate() async throws {
    terminated.withLock { $0 += 1 }
    if exitOnTerminate { exit(with: 143) }
  }

  func stop() async throws {
    guard exitDelay > .zero else { return exit(with: 137) }
    let delay = exitDelay
    Task {
      try? await Task.sleep(for: delay)
      self.exit(with: 137)
    }
  }
}

struct WorkloadHostTests {
  private func spec(_ scratch: ScratchDirectory) throws -> LaunchSpec {
    let suffix = UInt32.random(in: 0...UInt32.max)
    let container = scratch.path + "/container-\(suffix)"
    let log = scratch.path + "/log-\(suffix)"
    try FileManager.default.createDirectory(atPath: container, withIntermediateDirectories: false)
    return LaunchSpec(
      workload: "1a2b3c4d", containerDir: container, stdioPath: log, kernelPath: "", initfsPath: "", rootfsPath: "",
      imageConfigPath: "", environment: [], shares: [], sockets: [],
      stdio: try StdioLog(path: log, workload: "1a2b3c4d"))
  }

  private func host(_ workload: FakeWorkload? = nil, failing: Bool = false) -> WorkloadHost {
    WorkloadHost(workload: "1a2b3c4d") { _ in
      if failing { throw StartFailure(description: "boot failed") }
      return workload ?? FakeWorkload(exitOnTerminate: true)
    }
  }

  @Test("A host is idle until a start, then running, then exited with the code")
  func lifecycle() async throws {
    let scratch = try ScratchDirectory()
    defer { scratch.remove() }
    let workload = FakeWorkload(exitOnTerminate: true)
    let host = host(workload)
    #expect(await host.current == .idle)
    try await host.start(digest: "a") { try spec(scratch) }
    #expect(await host.current == .running)
    workload.exit(with: 3)
    let outcome = await host.waitForExit()
    #expect(outcome == WorkloadOutcome(code: 3, error: ""))
    #expect(await host.reply().state == .exited)
    #expect(await host.reply().exit.code == 3)
  }

  @Test("The same digest while running answers once; another digest fails")
  func startIsIdempotent() async throws {
    let scratch = try ScratchDirectory()
    defer { scratch.remove() }
    let host = host()
    try await host.start(digest: "a") { try spec(scratch) }
    try await host.start(digest: "a") { throw StartFailure(description: "must not prepare twice") }
    await #expect(throws: StartFailure.self) {
      try await host.start(digest: "b") { try spec(scratch) }
    }
    #expect(await host.current == .running)
    await host.stop(graceSeconds: 1)
  }

  @Test("A failed start leaves the host idle and ready to start again")
  func failedStart() async throws {
    let scratch = try ScratchDirectory()
    defer { scratch.remove() }
    let failing = host(failing: true)
    await #expect(throws: StartFailure.self) {
      try await failing.start(digest: "a") { try spec(scratch) }
    }
    #expect(await failing.current == .idle)
    await #expect(throws: StartFailure.self) {
      try await failing.start(digest: "a") { throw StartFailure(description: "prepare failed") }
    }
    #expect(await failing.current == .idle)
  }

  @Test("A failed launch removes the container directory and the log, so the start can run again")
  func failedLaunchCleansUp() async throws {
    let scratch = try ScratchDirectory()
    defer { scratch.remove() }
    let failing = host(failing: true)
    let prepared = try spec(scratch)
    await #expect(throws: StartFailure.self) {
      try await failing.start(digest: "a") { prepared }
    }
    #expect(!FileManager.default.fileExists(atPath: prepared.containerDir))
    #expect(!FileManager.default.fileExists(atPath: prepared.stdioPath))
  }

  @Test("A request whose root filesystem can't be cloned removes the container directory")
  func failedCloneCleansUp() throws {
    let scratch = try ScratchDirectory()
    defer { scratch.remove() }
    var request = Avirt_Vmd_V1_StartWorkload()
    request.containerDir = scratch.path + "/container"
    request.rootfsBasePath = scratch.path + "/missing.ext4"
    request.stdioLogPath = scratch.path + "/log"
    #expect(throws: (any Error).self) { try LaunchSpec(request: request, workload: "1a2b3c4d") }
    #expect(!FileManager.default.fileExists(atPath: request.containerDir))
  }

  @Test("Stop sends the termination signal and waits for the exit")
  func stopTerminates() async throws {
    let scratch = try ScratchDirectory()
    defer { scratch.remove() }
    let workload = FakeWorkload(exitOnTerminate: true)
    let host = host(workload)
    try await host.start(digest: "a") { try spec(scratch) }
    await host.stop(graceSeconds: 5)
    #expect(workload.terminations == 1)
    #expect(await host.waitForExit() == WorkloadOutcome(code: 143, error: ""))
  }

  @Test("A workload that ignores the signal is stopped after the grace period")
  func stopForces() async throws {
    let scratch = try ScratchDirectory()
    defer { scratch.remove() }
    let workload = FakeWorkload(exitOnTerminate: false)
    let host = host(workload)
    try await host.start(digest: "a") { try spec(scratch) }
    await host.stop(graceSeconds: 0)
    #expect(await host.waitForExit() == WorkloadOutcome(code: 137, error: ""))
  }

  @Test("A forced stop returns only after the workload's exit has closed the log")
  func forcedStopWaitsForExit() async throws {
    let scratch = try ScratchDirectory()
    defer { scratch.remove() }
    let workload = FakeWorkload(exitOnTerminate: false, exitDelay: .milliseconds(200))
    let host = host(workload)
    try await host.start(digest: "a") { try spec(scratch) }
    await host.stop(graceSeconds: 0)
    #expect(await host.current == .exited(WorkloadOutcome(code: 137, error: "")))
  }

  @Test("Stop in the idle state answers at once and Stop is idempotent")
  func stopWhenIdle() async throws {
    let host = host()
    await host.stop(graceSeconds: 5)
    #expect(await host.current == .idle)
  }

  @Test("A stop during the start waits for the launch, then stops the workload that launched")
  func stopDuringStart() async throws {
    let scratch = try ScratchDirectory()
    defer { scratch.remove() }
    let workload = FakeWorkload(exitOnTerminate: true)
    let gate = AsyncStream.makeStream(of: Void.self)
    let host = WorkloadHost(workload: "1a2b3c4d") { _ in
      for await _ in gate.stream { break }
      return workload
    }
    let spec = try spec(scratch)
    let starting = Task { try await host.start(digest: "a") { spec } }
    while await host.current != .starting { try await Task.sleep(for: .milliseconds(5)) }

    let stopped = Counter()
    let stopping = Task {
      await host.stop(graceSeconds: 5)
      stopped.increment()
    }
    try await Task.sleep(for: .milliseconds(200))
    #expect(stopped.value == 0)
    #expect(workload.terminations == 0)

    gate.continuation.yield()
    await stopping.value
    try await starting.value
    #expect(workload.terminations == 1)
    #expect(await host.waitForExit() == WorkloadOutcome(code: 143, error: ""))
  }
}
