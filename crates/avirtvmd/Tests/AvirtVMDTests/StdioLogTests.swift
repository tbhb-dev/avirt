import Foundation
import Testing

@testable import AvirtVMD

struct StdioLogTests {
  private func records(_ path: String) throws -> [[String: Any]] {
    try String(contentsOfFile: path, encoding: .utf8)
      .split(separator: "\n")
      .map { line in
        try #require(
          try JSONSerialization.jsonObject(with: Data(line.utf8)) as? [String: Any])
      }
  }

  private func logPath() throws -> (ScratchDirectory, String) {
    let scratch = try ScratchDirectory()
    return (scratch, scratch.path + "/stdio.ndjson")
  }

  private var quick: StdioLimits {
    var limits = StdioLimits()
    limits.flushDelay = .milliseconds(30)
    return limits
  }

  @Test("Lines are framed across chunks and an unfinished line flushes partial")
  func framesLines() throws {
    let (scratch, path) = try logPath()
    defer { scratch.remove() }
    let log = try StdioLog(path: path, workload: "1a2b3c4d", limits: quick)
    let out = log.writer(for: .stdout)
    try out.write(Data(("hello" + "\n" + "wor").utf8))
    try out.write(Data(("ld" + "\n" + "tail").utf8))
    Thread.sleep(forTimeInterval: 0.3)
    log.close()

    let all = try records(path)
    #expect(all.map { $0["text"] as? String } == ["hello", "world", "tail"])
    #expect(all.map { $0["partial"] as? Bool } == [false, false, true])
    let first = try #require(all.first)
    #expect(first["workload"] as? String == "1a2b3c4d")
    #expect(first["short_id"] as? String == "1a2b3c4d")
    #expect(first["ns"] as? String == "default")
    #expect(first["stream"] as? String == "stdout")
    let stamp = try #require(first["ts"] as? String)
    #expect(stamp.range(of: #"^\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d\.\d{3}Z$"#, options: .regularExpression) != nil)
  }

  @Test("A line unfinished at close is written as complete")
  func closeFlushes() throws {
    let (scratch, path) = try logPath()
    defer { scratch.remove() }
    let log = try StdioLog(path: path, workload: "1a2b3c4d")
    try log.writer(for: .stderr).write(Data("no newline".utf8))
    log.close()
    let all = try records(path)
    #expect(all.count == 1)
    #expect(all.first?["partial"] as? Bool == false)
    #expect(all.first?["stream"] as? String == "stderr")
  }

  @Test("Bytes that aren't UTF-8 are written as base64")
  func invalidUTF8() throws {
    let (scratch, path) = try logPath()
    defer { scratch.remove() }
    let log = try StdioLog(path: path, workload: "1a2b3c4d")
    try log.writer(for: .console).write(Data([0xFF, 0xFE, 0x0A]))
    log.close()
    let record = try #require(try records(path).first)
    #expect(record["text"] == nil)
    #expect(record["b64"] as? String == Data([0xFF, 0xFE]).base64EncodedString())
  }

  @Test("A line over the cap is written in pieces, each partial but the last")
  func longLines() throws {
    let (scratch, path) = try logPath()
    defer { scratch.remove() }
    var limits = StdioLimits()
    limits.lineBytes = 8
    let log = try StdioLog(path: path, workload: "1a2b3c4d", limits: limits)
    try log.writer(for: .stdout).write(Data((String(repeating: "x", count: 20) + "\n").utf8))
    log.close()
    let all = try records(path)
    #expect(all.map { ($0["text"] as? String)?.count } == [8, 8, 4])
    #expect(all.map { $0["partial"] as? Bool } == [true, true, false])
  }

  @Test("A chunk that doesn't fit the ring is dropped and counted before the next record")
  func dropsAreCounted() throws {
    let (scratch, path) = try logPath()
    defer { scratch.remove() }
    var limits = StdioLimits()
    limits.ringBytes = 4
    let log = try StdioLog(path: path, workload: "1a2b3c4d", limits: limits)
    let out = log.writer(for: .stdout)
    try out.write(Data("0123456789\n".utf8))
    Thread.sleep(forTimeInterval: 0.2)
    try out.write(Data("ok\n".utf8))
    log.close()
    let all = try records(path)
    #expect(all.first?["dropped_bytes"] as? Int == 11)
    #expect(all.first?["stream"] as? String == "stdout")
    #expect(all.last?["text"] as? String == "ok")
  }

  @Test("A write never waits for the file")
  func writesDoNotBlock() throws {
    let (scratch, path) = try logPath()
    defer { scratch.remove() }
    let log = try StdioLog(path: path, workload: "1a2b3c4d")
    let out = log.writer(for: .stdout)
    let chunk = Data(repeating: 0x61, count: 1024 * 1024)
    let started = Date()
    for _ in 0..<64 { try out.write(chunk) }
    #expect(Date().timeIntervalSince(started) < 2)
    log.close()
  }

  @Test("The file is private and a second log can't take the same path")
  func privateAndExclusive() throws {
    let (scratch, path) = try logPath()
    defer { scratch.remove() }
    let log = try StdioLog(path: path, workload: "1a2b3c4d")
    let attributes = try FileManager.default.attributesOfItem(atPath: path)
    #expect(attributes[.posixPermissions] as? Int == 0o600)
    #expect(throws: (any Error).self) { try StdioLog(path: path, workload: "1a2b3c4d") }
    log.close()
  }

  @Test("Past the file cap nothing more is written")
  func fileCap() throws {
    let (scratch, path) = try logPath()
    defer { scratch.remove() }
    var limits = StdioLimits()
    limits.fileBytes = 400
    let log = try StdioLog(path: path, workload: "1a2b3c4d", limits: limits)
    let out = log.writer(for: .stdout)
    for index in 0..<20 { try out.write(Data("line \(index)\n".utf8)) }
    log.close()
    let size = try #require(try FileManager.default.attributesOfItem(atPath: path)[.size] as? Int)
    #expect(size <= 400)
    #expect(size > 0)
  }
}
