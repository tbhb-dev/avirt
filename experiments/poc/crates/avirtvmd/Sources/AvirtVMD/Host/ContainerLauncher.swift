import Containerization
import ContainerizationOCI
import Foundation
import SystemPackage

/// A workload container that Containerization runs in its own VM.
struct ContainerWorkload: VirtualWorkload {
  let container: LinuxContainer

  func wait() async throws -> Int32 {
    try await container.wait().exitCode
  }

  func terminate() async throws {
    try await container.kill(.term)
  }

  func stop() async throws {
    try await container.stop()
  }
}

/// Boots the workload container in a VM with no network.
///
/// The container's stdout and stderr go to the stdio log. So does the VM console, which a thread
/// reads from the boot log pipe.
func launchContainer(_ spec: LaunchSpec) async throws -> any VirtualWorkload {
  let kernel = Kernel(path: URL(fileURLWithPath: spec.kernelPath), platform: .linuxArm)
  let initialFilesystem = Mount.block(
    format: "ext4", source: spec.initfsPath, destination: "/", options: ["ro"])
  let vmm = VZVirtualMachineManager(kernel: kernel, initialFilesystem: initialFilesystem)
  let image = try JSONDecoder().decode(
    ContainerizationOCI.Image.self,
    from: Data(contentsOf: URL(fileURLWithPath: spec.imageConfigPath)))
  let console = Pipe()
  let container = try LinuxContainer(
    spec.workload,
    rootfs: .block(format: "ext4", source: spec.rootfsPath, destination: "/", options: []),
    vmm: vmm
  ) { configuration in
    if let imageConfig = image.config {
      configuration.process = LinuxProcessConfiguration(from: imageConfig)
    }
    configuration.process.environmentVariables += spec.environment
    configuration.process.terminal = false
    configuration.process.stdout = spec.stdio.writer(for: .stdout)
    configuration.process.stderr = spec.stdio.writer(for: .stderr)
    configuration.useInit = true
    configuration.sockets = spec.sockets.map { socket in
      UnixSocketConfiguration(
        source: URL(fileURLWithPath: socket.host),
        destination: URL(fileURLWithPath: socket.guest),
        permissions: FilePermissions(rawValue: CModeT(socket.mode)),
        direction: .into)
    }
    configuration.mounts += spec.shares.map { .share(source: $0.host, destination: $0.guest) }
    configuration.bootLog = BootLog.fileHandle(console.fileHandleForWriting)
  }
  let consoleWriter = spec.stdio.writer(for: .console)
  Thread.detachNewThread {
    while let chunk = try? console.fileHandleForReading.read(upToCount: 4096), !chunk.isEmpty {
      try? consoleWriter.write(chunk)
    }
  }
  try await container.create()
  try await container.start()
  return ContainerWorkload(container: container)
}
