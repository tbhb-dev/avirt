import Darwin

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
