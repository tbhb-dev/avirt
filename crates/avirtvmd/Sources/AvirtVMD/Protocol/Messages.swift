import NIOCore
import SwiftProtobuf

typealias HostMessage = Avirt_Vmd_V1_HostMessage
typealias HelperMessage = Avirt_Vmd_V1_HelperMessage
typealias HostHello = Avirt_Vmd_V1_HostHello
typealias HelperHello = Avirt_Vmd_V1_HelperHello
typealias Refusal = Avirt_Vmd_V1_Refusal
typealias RefusalCode = Avirt_Vmd_V1_RefusalCode
typealias Request = Avirt_Vmd_V1_Request
typealias Response = Avirt_Vmd_V1_Response
typealias Event = Avirt_Vmd_V1_Event
typealias WorkloadState = Avirt_Vmd_V1_State

/// The protocol versions this avirtvmd speaks.
///
/// The handshake accepts exactly the versions from `minProtocolVersion` through
/// `maxProtocolVersion`.
let minProtocolVersion: UInt32 = 1
let maxProtocolVersion: UInt32 = 1

extension Message {
  /// The message as one frame body.
  func frameBody() throws -> ByteBuffer {
    ByteBuffer(bytes: try serializedBytes() as [UInt8])
  }

  /// Decodes one frame body.
  init(frameBody body: ByteBuffer) throws {
    try self.init(serializedBytes: Array(body.readableBytesView))
  }
}
