import ArgumentParser
import AvirtVMD

@main
struct Avirtvmd: AsyncParsableCommand {
  static let configuration = CommandConfiguration(
    commandName: "avirtvmd",
    abstract: "Hosts one workload's VM for avirthostd.",
    subcommands: [Run.self]
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
