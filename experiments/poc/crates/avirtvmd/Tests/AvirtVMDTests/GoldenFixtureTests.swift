import Foundation
import SwiftProtobuf
import Testing

@testable import AvirtVMD

/// The fixtures are the contract with avirthostd's Rust side.
///
/// Each `.txtpb` file is a message in text format and each `.bin` file is its encoding, produced by
/// `protoc --encode`.
struct GoldenFixtureTests {
  private static let directory =
    Bundle.module.url(forResource: "fixtures", withExtension: nil)
    ?? Bundle.module.bundleURL.appending(path: "fixtures")

  private static let names: [String] = {
    let files = (try? FileManager.default.contentsOfDirectory(atPath: directory.path)) ?? []
    return files.filter { $0.hasSuffix(".txtpb") }.map { String($0.dropLast(".txtpb".count)) }.sorted()
  }()

  @Test("There is one fixture per message kind")
  func coversEveryMessageKind() {
    #expect(Self.names.count == 12)
  }

  @Test("A fixture's text form encodes to its binary form byte for byte", arguments: names)
  func encodingMatches(name: String) throws {
    let text = try String(contentsOf: Self.directory.appending(path: "\(name).txtpb"), encoding: .utf8)
    let binary = try Data(contentsOf: Self.directory.appending(path: "\(name).bin"))
    if name.hasPrefix("host-") {
      let message = try HostMessage(textFormatString: text)
      #expect(try message.serializedData() == binary)
      #expect(try HostMessage(serializedBytes: binary) == message)
    } else {
      let message = try HelperMessage(textFormatString: text)
      #expect(try message.serializedData() == binary)
      #expect(try HelperMessage(serializedBytes: binary) == message)
    }
  }
}
