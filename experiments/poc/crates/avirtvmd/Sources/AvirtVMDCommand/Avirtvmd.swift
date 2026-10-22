import ArgumentParser
import AvirtVMD
import Foundation

@main
struct Avirtvmd: AsyncParsableCommand {
  static let configuration = CommandConfiguration(
    commandName: "avirtvmd",
    abstract: "Hosts one workload's VM for avirthostd.",
    subcommands: [Run.self, PrepareCommand.self]
  )
}

struct Run: AsyncParsableCommand {
  static let configuration = CommandConfiguration(
    abstract: "Serves the control channel for one workload until avirthostd's end of stdin closes."
  )

  @Option(help: "The workload's id.")
  var workload: String

  @Option(help: "The workload's directory, where v.sock and vmd.lock live.")
  var dir: String

  func run() async throws {
    let code = await runHelper(workload: workload, directory: dir)
    if code != HelperExit.ok {
      throw ExitCode(code)
    }
  }
}

struct PrepareCommand: AsyncParsableCommand {
  static let configuration = CommandConfiguration(
    commandName: "prepare",
    abstract: "Makes one guest artifact and prints a JSON object on success.",
    subcommands: [Fetch.self, Extract.self, Initfs.self, Rootfs.self]
  )
}

/// Runs a `prepare` step.
///
/// A failure becomes one line on stderr and exit code 70.
func prepared(_ step: () async throws -> String) async throws {
  do {
    print(try await step())
  } catch {
    FileHandle.standardError.write(Data("avirtvmd: \(error)\n".utf8))
    throw ExitCode(HelperExit.failure)
  }
}

struct Fetch: AsyncParsableCommand {
  static let configuration = CommandConfiguration(abstract: "Downloads a file.")

  @Option(help: "The URL to download.")
  var url: String

  @Option(help: "Where to write the file.")
  var out: String

  func run() async throws {
    guard let source = URL(string: url) else { throw ValidationError("--url isn't a URL") }
    try await prepared {
      try await Prepare.fetch(url: source, out: URL(fileURLWithPath: out))
      return "{}"
    }
  }
}

struct Extract: AsyncParsableCommand {
  static let configuration = CommandConfiguration(abstract: "Extracts one member of an archive.")

  @Option(help: "The archive.")
  var archive: String

  @Option(help: "The member's path in the archive.")
  var member: String

  @Option(help: "Where to write the member.")
  var out: String

  func run() async throws {
    try await prepared {
      try Prepare.extract(
        archive: URL(fileURLWithPath: archive), member: member, out: URL(fileURLWithPath: out))
      return "{}"
    }
  }
}

struct Initfs: AsyncParsableCommand {
  static let configuration = CommandConfiguration(
    abstract: "Writes the init filesystem of an image and prints the image's digest.")

  @Option(help: "The init image reference.")
  var reference: String

  @Option(help: "A scratch directory for the image store.")
  var store: String

  @Option(help: "Where to write the filesystem.")
  var out: String

  func run() async throws {
    try await prepared {
      let digest = try await Prepare.initfs(
        reference: reference, store: URL(fileURLWithPath: store), out: URL(fileURLWithPath: out))
      return "{\"digest\":\"\(digest)\"}"
    }
  }
}

struct Rootfs: AsyncParsableCommand {
  static let configuration = CommandConfiguration(
    abstract: "Unpacks image layers into an ext4 root filesystem.")

  @Option(help: "Where to write the filesystem.")
  var out: String

  @Option(help: "The filesystem's capacity in bytes.")
  var capacity: UInt64

  @Option(name: .customLong("layer"), help: "A layer as <media-type>=<path>. Repeat for each layer, bottom first.")
  var layerArguments: [String]

  func validate() throws {
    for argument in layerArguments where Layer(argument: argument) == nil {
      throw ValidationError("--layer \(argument) isn't <media-type>=<path>")
    }
    guard !layerArguments.isEmpty else { throw ValidationError("give at least one --layer") }
  }

  func run() async throws {
    try await prepared {
      let layers = layerArguments.compactMap { Layer(argument: $0) }
      try await Prepare.rootfs(out: URL(fileURLWithPath: out), capacity: capacity, layers: layers)
      return "{}"
    }
  }
}
