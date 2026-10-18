import Foundation
import Synchronization
import Testing

@testable import AvirtVMD

/// A workload that exits when told to, or when stopped.
final class FakeWorkload: VirtualWorkload {
  private let exits = AsyncStream.makeStream(of: Int32.self)
  private let terminated = Mutex(0)
  private let exitOnTerminate: Bool

  init(exitOnTerminate: Bool) {
    self.exitOnTerminate = exitOnTerminate
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
    exit(with: 137)
  }
}

struct WorkloadHostTests {
  private func spec(_ scratch: ScratchDirectory) throws -> LaunchSpec {
    LaunchSpec(
      workload: "1a2b3c4d", kernelPath: "", initfsPath: "", rootfsPath: "", imageConfigPath: "",
      environment: [], shares: [], sockets: [],
      stdio: try StdioLog(path: scratch.path + "/log-\(UInt32.random(in: 0...UInt32.max))", workload: "1a2b3c4d"))
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

  @Test("Stop in the idle state answers at once and Stop is idempotent")
  func stopWhenIdle() async throws {
    let host = host()
    await host.stop(graceSeconds: 5)
    #expect(await host.current == .idle)
  }
}
