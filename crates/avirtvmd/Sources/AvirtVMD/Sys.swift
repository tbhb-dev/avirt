import Darwin
import Foundation
import Security

/// The calls into the C library that the rest of the package leans on.
enum Sys {
  static func currentUID() -> UInt32 {
    getuid()
  }

  /// Opens `path` and takes an exclusive `flock` on it.
  ///
  /// The file is created with mode 0600 and the lock lasts until the returned descriptor closes. A
  /// process's death releases the lock, even under `kill -9`, which is how avirthostd learns the
  /// helper is gone.
  static func lockFile(at path: String) throws(LockError) -> Int32 {
    let descriptor = unsafe open(path, O_CREAT | O_RDWR | O_CLOEXEC, 0o600)
    guard descriptor >= 0 else {
      throw .unavailable(errno)
    }
    guard flock(descriptor, LOCK_EX | LOCK_NB) == 0 else {
      let reason = errno
      close(descriptor)
      throw reason == EWOULDBLOCK ? .held : .unavailable(reason)
    }
    return descriptor
  }

  /// Creates the log file at `path`, which must not exist yet, with mode 0600.
  static func createLog(at path: String) throws(LockError) -> Int32 {
    let descriptor = unsafe open(path, O_WRONLY | O_CREAT | O_EXCL | O_APPEND | O_CLOEXEC, 0o600)
    guard descriptor >= 0 else {
      throw .unavailable(errno)
    }
    return descriptor
  }

  /// The code of this process.
  static func selfCode() throws -> SecCode {
    var code: SecCode?
    let status = unsafe SecCodeCopySelf([], &code)
    guard status == errSecSuccess, let code else { throw SignatureError.security("read its own code", status) }
    return code
  }

  /// The signing information of `code`: its flags, team identifier, and code directory hash.
  static func signingInformation(of code: SecCode) throws -> [String: Any] {
    var staticCode: SecStaticCode?
    var status = unsafe SecCodeCopyStaticCode(code, [], &staticCode)
    guard status == errSecSuccess, let staticCode else {
      throw SignatureError.security("read the static code", status)
    }
    var information: CFDictionary?
    status = unsafe SecCodeCopySigningInformation(
      staticCode, SecCSFlags(rawValue: kSecCSSigningInformation), &information)
    guard status == errSecSuccess, let information = information as? [String: Any] else {
      throw SignatureError.security("read the signing information", status)
    }
    return information
  }

  /// Compiles `text` into a code requirement.
  static func requirement(_ text: String) throws -> SecRequirement {
    var requirement: SecRequirement?
    let status = unsafe SecRequirementCreateWithString(text as CFString, [], &requirement)
    guard status == errSecSuccess, let requirement else {
      throw SignatureError.security("compile a code requirement", status)
    }
    return requirement
  }

  /// The process that holds `token`, or nil if it can't be found.
  static func process(holding token: audit_token_t) -> SecCode? {
    var token = token
    let bytes = unsafe Data(bytes: &token, count: MemoryLayout<audit_token_t>.size)
    let attributes = [kSecGuestAttributeAudit as String: bytes] as CFDictionary
    var guest: SecCode?
    guard unsafe SecCodeCopyGuestWithAttributes(nil, attributes, [], &guest) == errSecSuccess else { return nil }
    return guest
  }

  static func closeDescriptor(_ descriptor: Int32) {
    _ = close(descriptor)
  }

  /// Writes all of `data`, retrying a short write.
  ///
  /// Returns false if the write fails.
  static func writeAll(_ data: Data, to descriptor: Int32) -> Bool {
    var offset = 0
    while offset < data.count {
      let count = unsafe data.withUnsafeBytes { buffer -> Int in
        guard let base = buffer.baseAddress else { return -1 }
        return unsafe write(descriptor, base + offset, data.count - offset)
      }
      if count < 0 {
        if errno == EINTR { continue }
        return false
      }
      offset += count
    }
    return true
  }

  /// Copies `source` to `destination` as an APFS clone, which shares the data blocks.
  ///
  /// There is no fallback to a full copy.
  static func cloneFile(from source: String, to destination: String) throws(LockError) {
    guard unsafe clonefile(source, destination, 0) == 0 else {
      throw .unavailable(errno)
    }
  }

  static func removeFile(at path: String) {
    _ = unsafe unlink(path)
  }

  /// Renames `source` to `destination`, replacing a file already there.
  static func moveFile(from source: String, to destination: String) {
    _ = unsafe rename(source, destination)
  }

  static func restrictToOwner(_ path: String) throws(LockError) {
    guard unsafe chmod(path, 0o600) == 0 else {
      throw .unavailable(errno)
    }
  }
}

enum LockError: Error {
  /// Another process holds the lock.
  case held
  /// The file or its mode couldn't be set up; holds the `errno`.
  case unavailable(Int32)
}
