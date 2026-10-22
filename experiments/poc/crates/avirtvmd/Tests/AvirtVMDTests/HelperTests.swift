import Foundation
import Testing

@testable import AvirtVMD

struct HelperTests {
  private static let incarnationLine = Data((String(repeating: "0", count: 31) + "1\n").utf8)

  private func waitForSocket(in directory: String) async -> Bool {
    await eventually { FileManager.default.fileExists(atPath: directory + "/v.sock") }
  }

  @Test("The helper binds a 0600 socket and exits 0 when avirthostd's end of stdin closes")
  func exitsWhenStdinCloses() async throws {
    let scratch = try ScratchDirectory()
    defer { scratch.remove() }
    let pipe = Pipe()
    try pipe.fileHandleForWriting.write(contentsOf: Self.incarnationLine)
    let helper = Task {
      await runHelper(workload: "1a2b3c4d", directory: scratch.path, stdin: pipe.fileHandleForReading.fileDescriptor)
    }
    #expect(await waitForSocket(in: scratch.path))
    // bind creates the socket with the default mode, and the helper restricts it right after.
    let restricted = await eventually {
      let attributes = try? FileManager.default.attributesOfItem(atPath: scratch.path + "/v.sock")
      return attributes?[.posixPermissions] as? Int == 0o600
    }
    #expect(restricted)

    try pipe.fileHandleForWriting.close()
    #expect(await helper.value == HelperExit.ok)
    #expect(!FileManager.default.fileExists(atPath: scratch.path + "/v.sock"))
  }

  @Test(
    "A helper that can't tell its own signature exits 70 and binds no socket",
    arguments: [
      { throw SignatureError.security("read its own code", errSecCSUnsigned) },
      { SigningInfo(teamID: nil, meetsDeveloperID: true) },
    ] as [@Sendable () throws -> SigningInfo])
  func unreadableSignatureFails(reader: @Sendable () throws -> SigningInfo) async throws {
    let scratch = try ScratchDirectory()
    defer { scratch.remove() }
    let pipe = Pipe()
    try pipe.fileHandleForWriting.write(contentsOf: Self.incarnationLine)
    let code = await runHelper(
      workload: "1a2b3c4d", directory: scratch.path, stdin: pipe.fileHandleForReading.fileDescriptor,
      launcher: refusingLauncher, signingInfo: reader)
    #expect(code == HelperExit.failure)
    #expect(!FileManager.default.fileExists(atPath: scratch.path + "/v.sock"))
  }

  @Test("A second helper on the same directory exits 69 and leaves the first running")
  func lockIsExclusive() async throws {
    let scratch = try ScratchDirectory()
    defer { scratch.remove() }
    let first = Pipe()
    try first.fileHandleForWriting.write(contentsOf: Self.incarnationLine)
    let running = Task {
      await runHelper(workload: "1a2b3c4d", directory: scratch.path, stdin: first.fileHandleForReading.fileDescriptor)
    }
    #expect(await waitForSocket(in: scratch.path))

    let second = Pipe()
    try second.fileHandleForWriting.write(contentsOf: Self.incarnationLine)
    let code = await runHelper(
      workload: "1a2b3c4d", directory: scratch.path, stdin: second.fileHandleForReading.fileDescriptor)
    #expect(code == HelperExit.precondition)
    #expect(FileManager.default.fileExists(atPath: scratch.path + "/v.sock"))

    try first.fileHandleForWriting.close()
    #expect(await running.value == HelperExit.ok)
  }

  @Test("A first line that isn't an incarnation is a usage error", arguments: ["", "not hex\n", "ABCDEF\n"])
  func badIncarnation(line: String) async throws {
    let scratch = try ScratchDirectory()
    defer { scratch.remove() }
    let pipe = Pipe()
    try pipe.fileHandleForWriting.write(contentsOf: Data(line.utf8))
    try pipe.fileHandleForWriting.close()
    let code = await runHelper(
      workload: "1a2b3c4d", directory: scratch.path, stdin: pipe.fileHandleForReading.fileDescriptor)
    #expect(code == HelperExit.usage)
    #expect(!FileManager.default.fileExists(atPath: scratch.path + "/vmd.lock"))
  }

  @Test("A directory that doesn't exist is a precondition failure")
  func missingDirectory() async throws {
    let pipe = Pipe()
    try pipe.fileHandleForWriting.write(contentsOf: Self.incarnationLine)
    let code = await runHelper(
      workload: "1a2b3c4d", directory: "/nonexistent/avirtvmd", stdin: pipe.fileHandleForReading.fileDescriptor)
    #expect(code == HelperExit.precondition)
  }
}
