/// The 16 random bytes avirthostd generates for each helper start.
///
/// The helper presents them on every connection, so avirthostd can tell it from a leftover helper
/// of the same workload.
struct Incarnation: Equatable, Sendable {
  let bytes: [UInt8]
}

extension Incarnation {
  /// Parses exactly 32 lowercase hexadecimal characters.
  init?(hex: String) {
    let digits = Array(hex.utf8)
    guard digits.count == 32 else { return nil }
    var bytes: [UInt8] = []
    for pair in stride(from: 0, to: digits.count, by: 2) {
      guard let high = Self.nibble(digits[pair]), let low = Self.nibble(digits[pair + 1]) else {
        return nil
      }
      bytes.append(high << 4 | low)
    }
    self.init(bytes: bytes)
  }

  private static func nibble(_ digit: UInt8) -> UInt8? {
    switch digit {
    case UInt8(ascii: "0")...UInt8(ascii: "9"): digit - UInt8(ascii: "0")
    case UInt8(ascii: "a")...UInt8(ascii: "f"): digit - UInt8(ascii: "a") + 10
    default: nil
    }
  }
}
