import Containerization
import ContainerizationArchive
import ContainerizationEXT4
import ContainerizationOCI
import Foundation
import SystemPackage

/// Why a `prepare` step failed, as one line for stderr.
public struct PrepareFailure: Error, CustomStringConvertible {
  /// The line avirtvmd prints on stderr.
  public let description: String

  init(_ description: String) {
    self.description = description
  }
}

/// One layer of a root filesystem: the OCI media type and the file that holds the layer.
public struct Layer: Sendable, Equatable {
  /// The OCI media type, which names the layer's compression.
  public let mediaType: String
  /// The file that holds the layer.
  public let path: String

  /// A layer of `mediaType` stored at `path`.
  public init(mediaType: String, path: String) {
    self.mediaType = mediaType
    self.path = path
  }

  /// Parses `<media-type>=<path>`.
  public init?(argument: String) {
    guard let separator = argument.firstIndex(of: "=") else { return nil }
    let path = String(argument[argument.index(after: separator)...])
    guard !path.isEmpty else { return nil }
    self.init(mediaType: String(argument[..<separator]), path: path)
  }

  /// The archive filter for this layer's compression.
  func filter() throws -> Filter {
    switch mediaType {
    case MediaTypes.imageLayer, MediaTypes.dockerImageLayer:
      return .none
    case MediaTypes.imageLayerGzip, MediaTypes.dockerImageLayerGzip:
      return .gzip
    case MediaTypes.imageLayerZstd, MediaTypes.dockerImageLayerZstd:
      return .zstd
    default:
      throw PrepareFailure("layer media type \(mediaType) isn't supported")
    }
  }
}

/// The one-shot steps that avirthostd runs to make guest artifacts.
///
/// Each writes only the path it is given.
public enum Prepare {
  /// Downloads `url` to `out`.
  public static func fetch(url: URL, out: URL) async throws {
    let (downloaded, response) = try await URLSession.shared.download(from: url)
    if let http = response as? HTTPURLResponse, http.statusCode != 200 {
      throw PrepareFailure("\(url.absoluteString) answered \(http.statusCode)")
    }
    try FileManager.default.moveItem(at: downloaded, to: out)
  }

  /// Writes the archive member at `member` to `out` with mode 0644.
  public static func extract(archive: URL, member: String, out: URL) throws {
    let reader = try ArchiveReader(file: archive)
    let (_, data) = try reader.extractFile(path: member)
    guard
      FileManager.default.createFile(
        atPath: out.path, contents: data, attributes: [.posixPermissions: 0o644])
    else {
      throw PrepareFailure("can't write \(out.path)")
    }
  }

  /// Pulls the init image into the image store at `store`, writes its root filesystem to `out`, and
  /// returns the image's digest.
  public static func initfs(reference: String, store: URL, out: URL) async throws -> String {
    let images = try ImageStore(path: store)
    let initImage = try await images.getInitImage(reference: reference)
    _ = try await initImage.initBlock(at: out, for: .linuxArm)
    return try await images.get(reference: reference).digest
  }

  /// Unpacks `layers`, in order, into a new ext4 file at `out` of at least `capacity` bytes.
  public static func rootfs(out: URL, capacity: UInt64, layers: [Layer]) async throws {
    guard !FileManager.default.fileExists(atPath: out.path) else {
      throw PrepareFailure("\(out.path) exists already")
    }
    let filters = try layers.map { try $0.filter() }
    var finished = false
    defer {
      if !finished { try? FileManager.default.removeItem(at: out) }
    }
    let formatter = try EXT4.Formatter(FilePath(out.path), minDiskSize: capacity)
    for (layer, filter) in zip(layers, filters) {
      let reader = try ArchiveReader(
        format: .paxRestricted, filter: filter, file: URL(fileURLWithPath: layer.path))
      try await formatter.unpack(reader: reader)
    }
    try formatter.close()
    finished = true
  }
}
