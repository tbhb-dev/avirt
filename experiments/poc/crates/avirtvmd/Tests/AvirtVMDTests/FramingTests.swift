import NIOCore
import NIOEmbedded
import Testing

@testable import AvirtVMD

struct FramingTests {
  private func channel() throws -> EmbeddedChannel {
    let channel = EmbeddedChannel()
    try channel.pipeline.syncOperations.addHandlers(
      ByteToMessageHandler(FrameDecoder()),
      MessageToByteHandler(FrameEncoder())
    )
    return channel
  }

  @Test("A frame survives the encoder and the decoder")
  func roundTrip() throws {
    let sender = try channel()
    let receiver = try channel()
    let body = ByteBuffer(bytes: Array(0...200))
    try sender.writeOutbound(body)
    let wire = try #require(try sender.readOutbound(as: ByteBuffer.self))
    #expect(wire.readableBytes == body.readableBytes + 4)
    #expect(wire.getInteger(at: 0, as: UInt32.self) == UInt32(body.readableBytes))
    try receiver.writeInbound(wire)
    #expect(try receiver.readInbound(as: ByteBuffer.self) == body)
  }

  @Test("A frame split across reads is delivered once it is complete")
  func splitReads() throws {
    let receiver = try channel()
    var wire = ByteBuffer()
    wire.writeInteger(UInt32(5))
    wire.writeString("hello")
    for byte in wire.readableBytesView {
      try receiver.writeInbound(ByteBuffer(bytes: [byte]))
    }
    #expect(try receiver.readInbound(as: ByteBuffer.self) == ByteBuffer(string: "hello"))
    #expect(try receiver.readInbound(as: ByteBuffer.self) == nil)
  }

  @Test("A frame of exactly 1 MiB is accepted")
  func largestFrame() throws {
    let receiver = try channel()
    var wire = ByteBuffer()
    wire.writeInteger(UInt32(maxFrameBody))
    wire.writeRepeatingByte(7, count: maxFrameBody)
    try receiver.writeInbound(wire)
    #expect(try receiver.readInbound(as: ByteBuffer.self)?.readableBytes == maxFrameBody)
  }

  @Test("A length over 1 MiB is refused before any of the body arrives")
  func overLongFrame() throws {
    let receiver = try channel()
    var wire = ByteBuffer()
    wire.writeInteger(UInt32(maxFrameBody + 1))
    #expect(throws: FrameError.tooLong(maxFrameBody + 1)) {
      try receiver.writeInbound(wire)
    }
  }

  @Test("The encoder refuses a body over 1 MiB")
  func overLongBody() throws {
    let sender = try channel()
    let body = ByteBuffer(repeating: 0, count: maxFrameBody + 1)
    #expect(throws: FrameError.tooLong(maxFrameBody + 1)) {
      try sender.writeOutbound(body)
    }
  }
}
