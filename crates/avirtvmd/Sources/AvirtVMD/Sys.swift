import Darwin
import Foundation

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
