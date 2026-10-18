import Containerization
import Darwin
import Foundation
import Synchronization

/// The streams that the log records.
public enum StdioStream: String, Sendable, CaseIterable {
  case stdout
  case stderr
  case console
}

/// The numbers that bound the log.
struct StdioLimits: Sendable {
  /// The bytes the ring holds before it drops chunks.
  var ringBytes = 4 * 1024 * 1024
  /// The longest line in one record.
  var lineBytes = 64 * 1024
  /// How long an unfinished line waits for its end.
  var flushDelay = Duration.milliseconds(200)
  /// The bytes the file takes before the writer stops writing.
  var fileBytes = 256 * 1024 * 1024
}

/// Writes a workload's stdout, stderr, and console to an NDJSON file without ever blocking the
/// caller.
///
/// A writer copies each chunk into a bounded ring. One thread drains the ring, frames lines, and
/// writes records. A chunk that doesn't fit counts toward `dropped_bytes`, which the log records
/// before the stream's next record.
public final class StdioLog: Sendable {
  private struct Chunk {
    let stream: StdioStream
    let received: Date
    let bytes: Data
  }

  private struct Shared {
    var chunks: [Chunk] = []
    var queuedBytes = 0
    var dropped: [StdioStream: Int] = [:]
    var closed = false
  }

  private let shared = Mutex(Shared())
  private let wake = DispatchSemaphore(value: 0)
  private let finished = DispatchSemaphore(value: 0)
  private let limits: StdioLimits

  /// Opens `path` (which must not exist) with mode 0600 and starts the writer thread.
  public convenience init(path: String, workload: String) throws {
    try self.init(path: path, workload: workload, limits: StdioLimits())
  }

  init(path: String, workload: String, limits: StdioLimits) throws {
    self.limits = limits
    let descriptor = try Sys.createLog(at: path)
    let output = StdioOutput(
      descriptor: descriptor, workload: workload, limits: limits)
    Thread.detachNewThread { [self] in
      drain(into: output)
      finished.signal()
    }
  }

  /// A writer for `stream`, for Containerization to hand output to.
  public func writer(for stream: StdioStream) -> any Writer {
    StreamWriter(log: self, stream: stream)
  }

  /// Copies a chunk into the ring, or counts it as dropped.
  ///
  /// It never blocks on the file.
  func enqueue(_ bytes: Data, on stream: StdioStream) {
    let accepted = shared.withLock { state -> Bool in
      guard !state.closed else { return false }
      if state.queuedBytes + bytes.count > limits.ringBytes {
        state.dropped[stream, default: 0] += bytes.count
        return true
      }
      state.chunks.append(Chunk(stream: stream, received: Date(), bytes: bytes))
      state.queuedBytes += bytes.count
      return true
    }
    if accepted { wake.signal() }
  }

  /// Writes what is queued, flushes the unfinished lines, and stops the writer thread.
  public func close() {
    shared.withLock { $0.closed = true }
    wake.signal()
    finished.wait()
  }

  private func drain(into output: StdioOutput) {
    while true {
      let (chunks, dropped, closed) = shared.withLock { state -> ([Chunk], [StdioStream: Int], Bool) in
        let taken = (state.chunks, state.dropped, state.closed)
        state.chunks = []
        state.queuedBytes = 0
        state.dropped = [:]
        return taken
      }
      for (stream, count) in dropped.sorted(by: { $0.key.rawValue < $1.key.rawValue }) {
        output.recordDropped(count, on: stream, at: Date())
      }
      for chunk in chunks {
        output.consume(chunk.bytes, on: chunk.stream, at: chunk.received)
      }
      if closed && chunks.isEmpty && dropped.isEmpty {
        output.flushAll(final: true)
        return
      }
      if chunks.isEmpty {
        if wake.wait(timeout: .now() + limits.flushDelay.seconds) == .timedOut {
          output.flushAll(final: false)
        }
      }
    }
  }
}

extension Duration {
  fileprivate var seconds: Double {
    Double(components.seconds) + Double(components.attoseconds) / 1e18
  }
}

private struct StreamWriter: Writer {
  let log: StdioLog
  let stream: StdioStream

  func write(_ data: Data) throws {
    log.enqueue(data, on: stream)
  }

  func close() throws {}
}

/// The file side of the log.
///
/// It frames lines and encodes records. Only the writer thread uses it.
final class StdioOutput: @unchecked Sendable {
  private let descriptor: Int32
  private let workload: String
  private let limits: StdioLimits
  private var written = 0
  private var pending: [StdioStream: (bytes: Data, since: Date)] = [:]
  private let stamp = ISO8601DateFormatter()
  private let encoder = JSONEncoder()

  init(descriptor: Int32, workload: String, limits: StdioLimits) {
    self.descriptor = descriptor
    self.workload = workload
    self.limits = limits
    stamp.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
    encoder.outputFormatting = [.sortedKeys, .withoutEscapingSlashes]
  }

  deinit {
    Sys.closeDescriptor(descriptor)
  }

  /// Splits `bytes` into lines and writes the complete ones.
  func consume(_ bytes: Data, on stream: StdioStream, at received: Date) {
    var buffer = pending[stream]?.bytes ?? Data()
    buffer.append(bytes)
    while let newline = buffer.firstIndex(of: 0x0A) {
      let line = buffer[buffer.startIndex..<newline]
      emit(line, on: stream, at: received, more: false)
      buffer = Data(buffer[buffer.index(after: newline)...])
    }
    while buffer.count >= limits.lineBytes {
      let head = buffer.prefix(limits.lineBytes)
      emit(head, on: stream, at: received, more: true)
      buffer = Data(buffer.dropFirst(limits.lineBytes))
    }
    pending[stream] = buffer.isEmpty ? nil : (buffer, received)
  }

  /// Writes the unfinished lines as partial, or as complete when the log is closing.
  func flushAll(final: Bool) {
    for (stream, entry) in pending {
      emit(entry.bytes, on: stream, at: entry.since, more: !final)
    }
    pending = [:]
  }

  func recordDropped(_ count: Int, on stream: StdioStream, at time: Date) {
    write(
      Record(
        ts: stamp.string(from: time), workload: workload, ns: "default", shortID: workload,
        stream: stream.rawValue, partial: nil, text: nil, b64: nil, droppedBytes: count))
  }

  /// Writes a line in pieces of at most `lineBytes`, every piece but the last marked partial.
  private func emit(_ line: Data, on stream: StdioStream, at time: Date, more: Bool) {
    var rest = line
    repeat {
      let piece = rest.prefix(limits.lineBytes)
      rest = Data(rest.dropFirst(limits.lineBytes))
      let partial = more || !rest.isEmpty
      let text = String(data: piece, encoding: .utf8)
      write(
        Record(
          ts: stamp.string(from: time), workload: workload, ns: "default", shortID: workload,
          stream: stream.rawValue, partial: partial, text: text,
          b64: text == nil ? piece.base64EncodedString() : nil, droppedBytes: nil))
    } while !rest.isEmpty
  }

  private func write(_ record: Record) {
    guard var line = try? encoder.encode(record) else { return }
    line.append(0x0A)
    guard written + line.count <= limits.fileBytes else { return }
    if Sys.writeAll(line, to: descriptor) { written += line.count }
  }
}

private struct Record: Encodable {
  let ts: String
  let workload: String
  let ns: String
  let shortID: String
  let stream: String
  let partial: Bool?
  let text: String?
  let b64: String?
  let droppedBytes: Int?

  enum CodingKeys: String, CodingKey {
    case ts, workload, ns, stream, partial, text, b64
    case shortID = "short_id"
    case droppedBytes = "dropped_bytes"
  }
}
