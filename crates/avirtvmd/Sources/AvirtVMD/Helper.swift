import Darwin
import Foundation
import NIOCore
import NIOPosix

/// Exit codes of `avirtvmd run`, following `sysexits.h`.
public enum HelperExit {
  /// avirthostd closed stdin or asked the helper to stop.
  public static let ok: Int32 = 0
  /// Bad flags, or a first stdin line that isn't an incarnation.
  public static let usage: Int32 = 64
  /// A precondition outside avirtvmd: the lock is held, or the socket path is in use.
  public static let precondition: Int32 = 69
  /// A step failed for a reason inside avirtvmd, such as a bad archive.
  public static let failure: Int32 = 70
}

/// How long avirthostd has to write the incarnation line.
private let incarnationTimeout = Duration.seconds(5)

/// Runs the helper for one workload until avirthostd closes `stdin` or asks it to stop.
///
/// - Parameters:
///   - workload: The workload's id, presented in the hello.
///   - directory: The workload's directory, which holds `vmd.lock` and `v.sock`.
///   - stdin: The descriptor avirthostd writes the incarnation to and then keeps open.
/// - Returns: The process exit code, one of the `HelperExit` values.
public func runHelper(workload: String, directory: String, stdin: Int32 = STDIN_FILENO) async -> Int32 {
  await runHelper(workload: workload, directory: directory, stdin: stdin, launcher: launchContainer)
}

/// [`runHelper`] with another way to start the workload's VM, for tests.
func runHelper(
  workload: String, directory: String, stdin: Int32, launcher: @escaping Launcher
) async -> Int32 {
  let watcher = StdinWatcher(descriptor: stdin)
  guard case .line(let text) = await watcher.readFirstLine(timeout: incarnationTimeout),
    let incarnation = Incarnation(hex: text)
  else {
    log("the first line of stdin must be 32 lowercase hexadecimal characters")
    return HelperExit.usage
  }

  let lock: Int32
  do {
    lock = try Sys.lockFile(at: directory + "/vmd.lock")
  } catch {
    log("can't take \(directory)/vmd.lock: \(error)")
    return HelperExit.precondition
  }
  defer { close(lock) }

  let group = MultiThreadedEventLoopGroup(numberOfThreads: 2)
  let socketPath = directory + "/v.sock"
  let listener: NIOAsyncChannel<ControlChannel, Never>
  let (stopRequests, stopSink) = AsyncStream.makeStream(of: Shutdown.self)
  let host = WorkloadHost(workload: workload, launcher: launcher)
  let server = ControlServer(
    identity: Identity(workload: workload, incarnation: incarnation),
    host: host,
    requestStop: { stopSink.yield(.stopRequested) }
  )
  do {
    listener = try await ControlServer.bind(path: socketPath, group: group)
    try Sys.restrictToOwner(socketPath)
  } catch {
    log("can't bind \(socketPath): \(error)")
    try? await group.shutdownGracefully()
    return HelperExit.precondition
  }

  await withTaskGroup(of: Void.self) { tasks in
    tasks.addTask {
      do {
        try await server.serve(on: listener)
      } catch is CancellationError {
        // The helper is shutting down.
      } catch {
        log("the control listener failed: \(error)")
      }
    }
    tasks.addTask { for await _ in watcher.shutdown { return } }
    tasks.addTask { for await _ in stopRequests { return } }
    await tasks.next()
    tasks.cancelAll()
  }

  // Stop the workload before exiting, so a VM never outlives its helper.
  await host.stop(graceSeconds: 5)
  Sys.removeFile(at: socketPath)
  try? await group.shutdownGracefully()
  return HelperExit.ok
}
