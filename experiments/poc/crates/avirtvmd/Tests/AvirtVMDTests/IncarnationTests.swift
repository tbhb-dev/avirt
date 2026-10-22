import Testing

@testable import AvirtVMD

struct IncarnationTests {
  @Test("32 lowercase hexadecimal characters parse to 16 bytes")
  func parses() throws {
    let incarnation = try #require(Incarnation(hex: "00ff10abcdef0123456789abcdef0123"))
    #expect(incarnation.bytes.count == 16)
    #expect(incarnation.bytes.prefix(4) == [0x00, 0xff, 0x10, 0xab])
  }

  @Test(
    "Anything else is refused",
    arguments: [
      "", "00ff", String(repeating: "0", count: 31), String(repeating: "0", count: 33),
      String(repeating: "A", count: 32), String(repeating: "g", count: 32), " " + String(repeating: "0", count: 31),
    ])
  func refuses(text: String) {
    #expect(Incarnation(hex: text) == nil)
  }
}
