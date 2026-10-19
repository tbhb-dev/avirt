import Darwin
import NIOCore

enum PeerError: Error {
  /// The channel doesn't expose socket options.
  case unsupportedChannel
}

/// The user ID of the process on the other end of a Unix socket, from the kernel's record of who
/// connected (`LOCAL_PEERCRED`).
func peerUID(of channel: any Channel) async throws -> UInt32 {
  guard let provider = channel as? any SocketOptionProvider else {
    throw PeerError.unsupportedChannel
  }
  let credentials: xucred = try await provider.unsafeGetSocketOption(
    level: NIOBSDSocket.OptionLevel(rawValue: SOL_LOCAL),
    name: NIOBSDSocket.Option(rawValue: LOCAL_PEERCRED)
  ).get()
  return credentials.cr_uid
}
