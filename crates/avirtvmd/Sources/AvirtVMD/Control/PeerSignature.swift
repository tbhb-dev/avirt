import Darwin
import Foundation
import Security

/// The only binary the control socket admits in a Developer ID build.
let hostSigningIdentifier = "dev.avirt.hostd"

enum SignatureError: Error {
  /// A Security call failed for a reason other than the code not meeting a requirement.
  case security(String, OSStatus)
  /// The code meets the Developer ID requirement but names no team.
  case noTeam
}

/// A code requirement that tasks can share.
///
/// It keeps the text, which the Security framework compiles for each check, because a compiled
/// requirement isn't `Sendable`.
struct PeerRequirement: Sendable {
  private let text: String

  /// Fails if the text isn't a valid requirement.
  init(_ text: String) throws {
    _ = try Sys.requirement(text)
    self.text = text
  }

  /// The result of checking `code` against the requirement.
  fileprivate func check(_ code: SecCode) throws -> OSStatus {
    SecCodeCheckValidity(code, [], try Sys.requirement(text))
  }
}

/// What the helper requires of a process that connects to `v.sock`.
enum PeerPolicy: Sendable {
  /// Any process of this user.
  case sameUser
  /// A process that satisfies the requirement.
  case signed(PeerRequirement)
}

/// What the helper's own code signature says about it.
struct SigningInfo {
  /// The team identifier in the signature, if it has one.
  let teamID: String?
  /// Whether the code satisfies Apple's Developer ID application requirement.
  let meetsDeveloperID: Bool
}

/// The Developer ID application requirement: the Developer ID certificate chain.
private let developerIDRequirementText = """
  anchor apple generic
  and certificate 1[field.1.2.840.113635.100.6.2.6] exists
  and certificate leaf[field.1.2.840.113635.100.6.1.13] exists
  """

/// The requirement for the peer in a Developer ID build whose own team identifier is `team`.
func hostRequirementText(team: String) -> String {
  """
  \(developerIDRequirementText)
  and certificate leaf[subject.OU] = "\(team)"
  and identifier "\(hostSigningIdentifier)"
  """
}

/// The policy that follows from the helper's own signature.
///
/// A Developer ID build admits only the host that shares its team. Any other signature, ad hoc
/// included, admits any process of the same user.
func peerPolicy(for info: SigningInfo) throws -> PeerPolicy {
  guard info.meetsDeveloperID else { return .sameUser }
  guard let team = info.teamID else { throw SignatureError.noTeam }
  return .signed(try PeerRequirement(hostRequirementText(team: team)))
}

/// Reads the helper's own code signature.
func ownSigningInfo() throws -> SigningInfo {
  let code = try Sys.selfCode()
  let information = try Sys.signingInformation(of: code)
  let team = information[kSecCodeInfoTeamIdentifier as String] as? String
  return try signingInfo(team: team, validity: PeerRequirement(developerIDRequirementText).check(code))
}

/// The signing information for a team identifier and the status of checking the Developer ID requirement.
///
/// Only a status that says the code meets or fails the requirement is an answer. Any other status
/// throws, so the helper never guesses.
func signingInfo(team: String?, validity: OSStatus) throws -> SigningInfo {
  switch validity {
  case errSecSuccess:
    return SigningInfo(teamID: team, meetsDeveloperID: true)
  case errSecCSReqFailed, errSecCSUnsigned:
    return SigningInfo(teamID: team, meetsDeveloperID: false)
  case let failure:
    throw SignatureError.security("check its own code", failure)
  }
}

/// Whether the process that holds `token` satisfies `requirement`.
///
/// Any failure to find or check the process counts as not satisfying it.
func peerSatisfies(_ requirement: PeerRequirement, token: audit_token_t) -> Bool {
  guard let guest = Sys.process(holding: token) else { return false }
  return (try? requirement.check(guest)) == errSecSuccess
}
