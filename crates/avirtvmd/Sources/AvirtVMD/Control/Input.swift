import Foundation

enum Shutdown: Sendable {
  /// avirthostd's end of stdin closed.
  case stdinClosed
  /// avirthostd asked the helper to stop.
  case stopRequested
}

enum FirstLine: Sendable {
  case line(String)
  case endOfInput
  case timedOut
}

/// Reads avirthostd's end of stdin on a thread of its own.
///
/// The first line is the incarnation. After that nothing is expected, and the end of input means
/// avirthostd is gone.
struct StdinWatcher {
  let shutdown: AsyncStream<Shutdown>
  private let firstLine: AsyncStream<FirstLine>
  private let firstLineSink: AsyncStream<FirstLine>.Continuation

  /// Longest first line: 32 hexadecimal characters and the newline.
  private static let longestLine = 33

  init(descriptor: Int32) {
    let (firstLine, lineSink) = AsyncStream.makeStream(of: FirstLine.self)
    let (shutdown, shutdownSink) = AsyncStream.makeStream(of: Shutdown.self)
    self.firstLine = firstLine
    self.firstLineSink = lineSink
    self.shutdown = shutdown
    Thread.detachNewThread {
      let input = FileHandle(fileDescriptor: descriptor, closeOnDealloc: false)
      lineSink.yield(Self.readLine(from: input))
      lineSink.finish()
      while let chunk = try? input.read(upToCount: 4096), !chunk.isEmpty {}
      shutdownSink.yield(.stdinClosed)
      shutdownSink.finish()
    }
  }

  /// The first line, or why there isn't one.
  ///
  /// Gives up after `timeout`.
  func readFirstLine(timeout: Duration) async -> FirstLine {
    let sink = firstLineSink
    let timer = Task {
      try? await Task.sleep(for: timeout)
      sink.yield(.timedOut)
    }
    defer { timer.cancel() }
    for await outcome in firstLine {
      return outcome
    }
    return .endOfInput
  }

  /// Reads one byte at a time so nothing after the newline is consumed.
  private static func readLine(from input: FileHandle) -> FirstLine {
    var line: [UInt8] = []
    while line.count <= longestLine {
      guard let byte = try? input.read(upToCount: 1), let value = byte.first else {
        return .endOfInput
      }
      if value == UInt8(ascii: "\n") {
        return .line(String(decoding: line, as: UTF8.self))
      }
      line.append(value)
    }
    return .line(String(decoding: line, as: UTF8.self))
  }
}
