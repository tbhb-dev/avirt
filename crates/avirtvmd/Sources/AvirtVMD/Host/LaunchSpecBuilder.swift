import Foundation

extension LaunchSpec {
  /// Prepares the files for a `StartWorkload` and returns what the launcher needs.
  ///
  /// Creates `container_dir` with mode 0700, clones the read-only root filesystem into it with
  /// APFS `clonefile` (a failure fails the request, with no fallback to a full copy), and opens the
  /// stdio log.
  init(request: Avirt_Vmd_V1_StartWorkload, workload: String) throws {
    try FileManager.default.createDirectory(
      atPath: request.containerDir, withIntermediateDirectories: false,
      attributes: [.posixPermissions: 0o700])
    let rootfs = request.containerDir + "/rootfs.ext4"
    try Sys.cloneFile(from: request.rootfsBasePath, to: rootfs)
    self.init(
      workload: workload,
      kernelPath: request.kernelPath,
      initfsPath: request.initfsPath,
      rootfsPath: rootfs,
      imageConfigPath: request.imageConfigPath,
      environment: request.env,
      shares: request.shares.map { ($0.hostPath, $0.guestPath) },
      sockets: request.sockets.map { ($0.hostPath, $0.guestPath, $0.mode) },
      stdio: try StdioLog(path: request.stdioLogPath, workload: workload))
  }
}
