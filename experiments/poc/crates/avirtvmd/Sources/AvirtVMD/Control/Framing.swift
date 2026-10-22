import NIOCore

/// The largest frame body either side sends or accepts.
let maxFrameBody = 1 << 20

enum FrameError: Error, Equatable {
  case tooLong(Int)
}

/// Splits the byte stream into frames.
///
/// A frame is a 4-byte big-endian length, then that many bytes. A length over `maxFrameBody` is
/// an error, raised before any of the body is buffered.
struct FrameDecoder: ByteToMessageDecoder {
  typealias InboundOut = ByteBuffer

  mutating func decode(context: ChannelHandlerContext, buffer: inout ByteBuffer) throws -> DecodingState {
    guard let announced = buffer.getInteger(at: buffer.readerIndex, as: UInt32.self) else {
      return .needMoreData
    }
    let length = Int(announced)
    guard length <= maxFrameBody else {
      throw FrameError.tooLong(length)
    }
    guard buffer.readableBytes >= 4 + length else {
      return .needMoreData
    }
    buffer.moveReaderIndex(forwardBy: 4)
    guard let body = buffer.readSlice(length: length) else {
      return .needMoreData
    }
    context.fireChannelRead(wrapInboundOut(body))
    return .continue
  }
}

/// Prefixes each outgoing frame body with its length.
struct FrameEncoder: MessageToByteEncoder {
  typealias OutboundIn = ByteBuffer

  func encode(data: ByteBuffer, out: inout ByteBuffer) throws {
    guard data.readableBytes <= maxFrameBody else {
      throw FrameError.tooLong(data.readableBytes)
    }
    out.writeInteger(UInt32(data.readableBytes))
    out.writeImmutableBuffer(data)
  }
}
