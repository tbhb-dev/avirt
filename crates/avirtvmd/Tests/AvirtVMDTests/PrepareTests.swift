import ContainerizationArchive
import ContainerizationEXT4
import Foundation
import SystemPackage
import Testing

@testable import AvirtVMD

struct PrepareTests {
  private func scratch() throws -> URL {
    let url = URL(fileURLWithPath: NSTemporaryDirectory())
      .appending(path: "avirtvmd-prepare-\(UInt32.random(in: 0...UInt32.max))")
    try FileManager.default.createDirectory(at: url, withIntermediateDirectories: true)
    return url
  }

  /// Writes a tar archive of top-level regular files.
  private func writeArchive(_ url: URL, filter: Filter, files: [(path: String, text: String)]) throws {
    let writer = try ArchiveWriter(format: .paxRestricted, filter: filter, file: url)
    for file in files {
      let data = Data(file.text.utf8)
      let entry = WriteEntry()
      entry.path = file.path
      entry.fileType = .regular
      entry.permissions = 0o644
      entry.size = Int64(data.count)
      try writer.writeEntry(entry: entry, data: data)
    }
    try writer.finishEncoding()
  }

  @Test("Layers unpack in order: a later layer wins and a whiteout removes a file")
  func layersUnpackInOrder() async throws {
    let directory = try scratch()
    defer { try? FileManager.default.removeItem(at: directory) }
    let lower = directory.appending(path: "lower.tar.gz")
    let upper = directory.appending(path: "upper.tar")
    try writeArchive(lower, filter: .gzip, files: [("a.txt", "lower a"), ("b.txt", "lower b"), ("c.txt", "lower c")])
    try writeArchive(upper, filter: .none, files: [("a.txt", "upper a"), (".wh.b.txt", "")])
    let out = directory.appending(path: "rootfs.ext4")

    try await Prepare.rootfs(
      out: out,
      capacity: 64 * 1024 * 1024,
      layers: [
        Layer(mediaType: "application/vnd.oci.image.layer.v1.tar+gzip", path: lower.path),
        Layer(mediaType: "application/vnd.oci.image.layer.v1.tar", path: upper.path),
      ])

    let reader = try EXT4.EXT4Reader(blockDevice: FilePath(out.path))
    #expect(String(decoding: try reader.readFile(at: FilePath("/a.txt")), as: UTF8.self) == "upper a")
    #expect(String(decoding: try reader.readFile(at: FilePath("/c.txt")), as: UTF8.self) == "lower c")
    #expect(!reader.exists(FilePath("/b.txt")))
  }

  @Test("A layer with an unsupported media type fails and leaves no file")
  func unsupportedMediaType() async throws {
    let directory = try scratch()
    defer { try? FileManager.default.removeItem(at: directory) }
    let out = directory.appending(path: "rootfs.ext4")
    await #expect(throws: PrepareFailure.self) {
      try await Prepare.rootfs(
        out: out, capacity: 64 * 1024 * 1024, layers: [Layer(mediaType: "text/plain", path: "/nonexistent")])
    }
    #expect(!FileManager.default.fileExists(atPath: out.path))
  }

  @Test("A root filesystem is never written over an existing file")
  func refusesToOverwrite() async throws {
    let directory = try scratch()
    defer { try? FileManager.default.removeItem(at: directory) }
    let out = directory.appending(path: "rootfs.ext4")
    try Data("keep".utf8).write(to: out)
    await #expect(throws: PrepareFailure.self) {
      try await Prepare.rootfs(out: out, capacity: 64 * 1024 * 1024, layers: [])
    }
    #expect(try Data(contentsOf: out) == Data("keep".utf8))
  }

  @Test("One member of a compressed archive is written with mode 0644")
  func extractsOneMember() throws {
    let directory = try scratch()
    defer { try? FileManager.default.removeItem(at: directory) }
    let archive = directory.appending(path: "bundle.tar.gz")
    try writeArchive(archive, filter: .gzip, files: [("one.bin", "first"), ("two.bin", "second")])
    let out = directory.appending(path: "two.bin")

    try Prepare.extract(archive: archive, member: "two.bin", out: out)

    #expect(try String(contentsOf: out, encoding: .utf8) == "second")
    let attributes = try FileManager.default.attributesOfItem(atPath: out.path)
    #expect(attributes[.posixPermissions] as? Int == 0o644)
  }

  @Test("A member that isn't in the archive is an error")
  func missingMember() throws {
    let directory = try scratch()
    defer { try? FileManager.default.removeItem(at: directory) }
    let archive = directory.appending(path: "bundle.tar")
    try writeArchive(archive, filter: .none, files: [("one.bin", "first")])
    #expect(throws: (any Error).self) {
      try Prepare.extract(archive: archive, member: "missing.bin", out: directory.appending(path: "out"))
    }
  }

  @Test(
    "A layer argument is a media type and a path",
    arguments: [
      ("application/vnd.oci.image.layer.v1.tar=/tmp/a=b", "/tmp/a=b"),
      ("x=y", "y"),
    ])
  func layerArgument(argument: String, path: String) {
    #expect(Layer(argument: argument)?.path == path)
  }

  @Test("Anything else is not a layer argument", arguments: ["", "no-separator", "type="])
  func badLayerArgument(argument: String) {
    #expect(Layer(argument: argument) == nil)
  }
}
