import Darwin
import Foundation
import NIOCore
import NIOPosix
import Security
import Testing

@testable import AvirtVMD

struct PeerSignatureTests {
  @Test("The policy follows the helper's own signature")
  func policyForEachSigningKind() throws {
    // Ad hoc, or signed with another certificate: the user ID check alone.
    for team in [nil, "ABCDE12345"] as [String?] {
      guard case .sameUser = try peerPolicy(for: SigningInfo(teamID: team, meetsDeveloperID: false)) else {
        Issue.record("a build outside Developer ID didn't get the same-user policy")
        return
      }
    }
    guard case .signed = try peerPolicy(for: SigningInfo(teamID: "ABCDE12345", meetsDeveloperID: true)) else {
      Issue.record("a Developer ID build with a team didn't get the signed policy")
      return
    }
    #expect(throws: SignatureError.self) {
      try peerPolicy(for: SigningInfo(teamID: nil, meetsDeveloperID: true))
    }
  }

  @Test("The host requirement has the documented text and compiles")
  func hostRequirementCompiles() throws {
    let text = hostRequirementText(team: "ABCDE12345")
    #expect(
      text == """
        anchor apple generic
        and certificate 1[field.1.2.840.113635.100.6.2.6] exists
        and certificate leaf[field.1.2.840.113635.100.6.1.13] exists
        and certificate leaf[subject.OU] = "ABCDE12345"
        and identifier "dev.avirt.hostd"
        """)
    _ = try PeerRequirement(text)
  }

  @Test("The test process meets a requirement that names its own code directory hash")
  func ownProcessMeetsItsCdhash() async throws {
    let requirement = try PeerRequirement(try ownCdhashRequirementText())
    #expect(peerSatisfies(requirement, token: try await ownAuditToken()))
  }

  @Test("The test process fails the host requirement")
  func ownProcessFailsTheHostRequirement() async throws {
    let requirement = try PeerRequirement(hostRequirementText(team: "ABCDE12345"))
    #expect(!peerSatisfies(requirement, token: try await ownAuditToken()))
  }

  @Test("The helper reads the signature of its own process")
  func ownSigningInfoReadsTheTestProcess() throws {
    _ = try ownSigningInfo()
    #expect(!(try ownCdhashRequirementText()).isEmpty)
  }

  @Test("Only a status that meets or fails the Developer ID requirement answers")
  func unexpectedStatusThrows() throws {
    #expect(try signingInfo(team: "ABCDE12345", validity: errSecSuccess).meetsDeveloperID)
    #expect(try !signingInfo(team: nil, validity: errSecCSReqFailed).meetsDeveloperID)
    #expect(try !signingInfo(team: nil, validity: errSecCSUnsigned).meetsDeveloperID)
    #expect(throws: SignatureError.self) { try signingInfo(team: nil, validity: errSecCSSignatureFailed) }
  }
}
