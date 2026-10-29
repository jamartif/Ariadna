import Foundation
#if canImport(Darwin)
import Darwin
#elseif canImport(Glibc)
import Glibc
#endif

/// Resultado de un subproceso. Equivalente a leer `StandardOutput` y `ExitCode`
/// de un `System.Diagnostics.Process` en .NET.
public struct ProcessOutput: Sendable, Equatable {
    public let executable: String
    public let arguments: [String]
    public let exitCode: Int32
    public let stdout: Data
    public let stderr: Data

    public init(executable: String, arguments: [String], exitCode: Int32, stdout: Data, stderr: Data) {
        self.executable = executable
        self.arguments = arguments
        self.exitCode = exitCode
        self.stdout = stdout
        self.stderr = stderr
    }

    public var stdoutString: String { String(decoding: stdout, as: UTF8.self) }
    public var stderrString: String { String(decoding: stderr, as: UTF8.self) }
    public var succeeded: Bool { exitCode == 0 }
}

public struct ProcessRequest: Sendable {
    public var executable: String
    public var arguments: [String]
    public var currentDirectory: URL?
    public var environment: [String: String]?
    public var stdin: Data?
    public var timeout: TimeInterval?

    public init(
        executable: String,
        arguments: [String] = [],
        currentDirectory: URL? = nil,
        environment: [String: String]? = nil,
        stdin: Data? = nil,
        timeout: TimeInterval? = nil
    ) {
        self.executable = executable
        self.arguments = arguments
        self.currentDirectory = currentDirectory
        self.environment = environment
        self.stdin = stdin
        self.timeout = timeout
    }
}

public enum ProcessError: Error, Sendable, Equatable {
    case executableNotFound(String)
    case timedOut(command: String)
    case cancelled
    case failedToLaunch(String)
}

/// Lanza procesos. Los tests de IA sustituyen esto por un doble que no ejecuta nada.
public protocol ProcessRunning: Sendable {
    func run(_ request: ProcessRequest) async throws -> ProcessOutput
}

/// Entorno reducido: PATH y HOME para que git/copilot/codex encuentren su config,
/// pero sin variables cuyo nombre parece un token.
public enum ProcessEnvironment {
    private static let passthrough: Set<String> = [
        "PATH", "HOME", "USER", "LOGNAME", "LANG", "LC_ALL", "LC_CTYPE", "LC_MESSAGES",
        "TERM", "TMPDIR", "TMP", "TEMP", "SHELL", "SSH_AUTH_SOCK", "SSH_AGENT_PID",
        "DISPLAY", "XDG_CONFIG_HOME", "XDG_DATA_HOME", "XDG_CACHE_HOME",
    ]

    public static func sanitized(
        _ source: [String: String] = ProcessInfo.processInfo.environment
    ) -> [String: String] {
        var result: [String: String] = [:]
        for (key, value) in source {
            if passthrough.contains(key) || key.hasPrefix("LC_") {
                result[key] = value
                continue
            }
            let upper = key.uppercased()
            let looksSecret = upper.contains("TOKEN")
                || upper.contains("SECRET")
                || upper.contains("PASSWORD")
                || upper.contains("PASSWD")
                || upper.contains("API_KEY")
            if looksSecret { continue }
            if key.hasPrefix("GIT_") || key.hasPrefix("SSH_") {
                result[key] = value
            }
        }
        return result
    }

    /// PATH del login shell (`$SHELL -l`), que es donde el usuario tiene git, Homebrew, etc.
    /// Si el shell no responde, se conserva el PATH del proceso.
    public static func sanitizedIncludingLoginPATH(
        _ source: [String: String] = ProcessInfo.processInfo.environment
    ) -> [String: String] {
        var environment = sanitized(source)
        let shell = source["SHELL"] ?? "/bin/bash"
        if let path = pathFromLoginShell(shell: shell) {
            environment["PATH"] = path
        }
        return environment
    }

    public static func pathFromLoginShell(shell: String, timeout: TimeInterval = 3) -> String? {
        pathCache.lock.lock()
        if let cached = pathCache.values[shell] {
            pathCache.lock.unlock()
            return cached
        }
        pathCache.lock.unlock()
        guard shell.hasPrefix("/"), !shell.contains("\0"), FileManager.default.isExecutableFile(atPath: shell) else {
            return nil
        }
        let process = Process()
        process.executableURL = URL(fileURLWithPath: shell)
        process.arguments = ["-l", "-c", "printf %s \"$PATH\""]
        let stdout = Pipe()
        process.standardOutput = stdout
        process.standardError = Pipe()
        do {
            try process.run()
        } catch {
            return nil
        }
        let started = Date()
        while process.isRunning && Date().timeIntervalSince(started) < timeout {
            Thread.sleep(forTimeInterval: 0.02)
        }
        if process.isRunning {
            process.terminate()
            if process.isRunning, process.processIdentifier > 0 {
                kill(process.processIdentifier, SIGKILL)
            }
            process.waitUntilExit()
            return nil
        }
        process.waitUntilExit()
        let data = stdout.fileHandleForReading.readDataToEndOfFile()
        let text = String(decoding: data, as: UTF8.self).trimmingCharacters(in: .whitespacesAndNewlines)
        guard process.terminationStatus == 0, !text.isEmpty, !text.contains("\n"), text.contains("/") else {
            return nil
        }
        pathCache.lock.lock()
        pathCache.values[shell] = text
        pathCache.lock.unlock()
        return text
    }

    private static let pathCache = PathCache()
}

private final class PathCache: @unchecked Sendable {
    let lock = NSLock()
    var values: [String: String] = [:]
}

/// Cola de git. Las lecturas (`status`, `diff`, `log`…) pueden ir a la vez.
/// Las mutaciones (add, commit, switch, merge, push…) entran de una en una
/// y no se mezclan con lecturas. Si hay una mutación esperando, las lecturas
/// nuevas esperan también: un commit no se queda detrás de un refresco infinito.
///
/// Es el equivalente a un `ReaderWriterLockSlim` de .NET, con preferencia por quien escribe.
actor GitOperationGate {
    private var activeReaders = 0
    private var writerActive = false
    private var waitingWriters = 0
    private var readerWaiters: [CheckedContinuation<Void, Never>] = []
    private var writerWaiters: [CheckedContinuation<Void, Never>] = []

    func read<T: Sendable>(_ body: @escaping @Sendable () async throws -> T) async throws -> T {
        await acquire(write: false)
        do {
            let value = try await body()
            release(write: false)
            return value
        } catch {
            release(write: false)
            throw error
        }
    }

    func write<T: Sendable>(_ body: @escaping @Sendable () async throws -> T) async throws -> T {
        await acquire(write: true)
        do {
            let value = try await body()
            release(write: true)
            return value
        } catch {
            release(write: true)
            throw error
        }
    }

    private func acquire(write: Bool) async {
        if write {
            waitingWriters += 1
        }
        while !canEnter(write: write) {
            await withCheckedContinuation { (continuation: CheckedContinuation<Void, Never>) in
                if write {
                    writerWaiters.append(continuation)
                } else {
                    readerWaiters.append(continuation)
                }
            }
        }
        if write {
            waitingWriters -= 1
            writerActive = true
        } else {
            activeReaders += 1
        }
    }

    private func release(write: Bool) {
        if write {
            writerActive = false
        } else {
            activeReaders = max(0, activeReaders - 1)
        }
        guard activeReaders == 0, !writerActive else { return }
        if !writerWaiters.isEmpty {
            writerWaiters.removeFirst().resume()
            return
        }
        let pending = readerWaiters
        readerWaiters.removeAll()
        for waiter in pending {
            waiter.resume()
        }
    }

    private func canEnter(write: Bool) -> Bool {
        if write {
            return !writerActive && activeReaders == 0
        }
        return !writerActive && waitingWriters == 0
    }
}

public struct FoundationProcessRunner: ProcessRunning, Sendable {
    public init() {}

    public func run(_ request: ProcessRequest) async throws -> ProcessOutput {
        let box = ProcessBox()
        return try await withTaskCancellationHandler {
            try await Task.detached(priority: .userInitiated) {
                try box.run(request)
            }.value
        } onCancel: {
            box.cancel()
        }
    }
}

/// Caja mutable del `Process` de Foundation. `@unchecked Sendable` porque el
/// candado (`NSLock`, como un `lock` de C#) protege el puntero al proceso
/// entre el hilo que espera y el que cancela.
private final class ProcessBox: @unchecked Sendable {
    private let lock = NSLock()
    private var process: Process?
    private var cancelled = false

    func cancel() {
        lock.lock()
        cancelled = true
        let running = process
        lock.unlock()
        if let running {
            Self.stop(running)
        }
    }

    func run(_ request: ProcessRequest) throws -> ProcessOutput {
        try throwIfCancelled()

        let environment = request.environment ?? ProcessEnvironment.sanitized()
        let path = environment["PATH"] ?? ProcessInfo.processInfo.environment["PATH"] ?? "/usr/bin:/bin"
        let executableURL = try resolve(request.executable, path: path)

        let process = Process()
        process.executableURL = executableURL
        process.arguments = request.arguments
        process.environment = environment
        if let cwd = request.currentDirectory {
            process.currentDirectoryURL = cwd
        }

        let stdoutPipe = Pipe()
        let stderrPipe = Pipe()
        let stdinPipe = Pipe()
        process.standardOutput = stdoutPipe
        process.standardError = stderrPipe
        process.standardInput = stdinPipe

        let stdoutBox = LockedData()
        let stderrBox = LockedData()
        let group = DispatchGroup()
        group.enter()
        DispatchQueue.global(qos: .userInitiated).async {
            stdoutBox.set(stdoutPipe.fileHandleForReading.readDataToEndOfFile())
            group.leave()
        }
        group.enter()
        DispatchQueue.global(qos: .userInitiated).async {
            stderrBox.set(stderrPipe.fileHandleForReading.readDataToEndOfFile())
            group.leave()
        }

        lock.lock()
        if cancelled {
            lock.unlock()
            throw ProcessError.cancelled
        }
        self.process = process
        lock.unlock()

        do {
            try process.run()
        } catch {
            throw ProcessError.failedToLaunch(String(describing: error))
        }

        if let stdin = request.stdin, !stdin.isEmpty {
            stdinPipe.fileHandleForWriting.write(stdin)
        }
        stdinPipe.fileHandleForWriting.closeFile()

        let started = Date()
        var timedOut = false
        while process.isRunning {
            if isCancelled {
                Self.stop(process)
                break
            }
            if let timeout = request.timeout, Date().timeIntervalSince(started) > timeout {
                timedOut = true
                Self.stop(process)
                break
            }
            Thread.sleep(forTimeInterval: 0.02)
        }
        process.waitUntilExit()
        group.wait()

        let command = ([request.executable] + request.arguments).joined(separator: " ")
        if isCancelled {
            throw ProcessError.cancelled
        }
        if timedOut {
            throw ProcessError.timedOut(command: command)
        }

        return ProcessOutput(
            executable: request.executable,
            arguments: request.arguments,
            exitCode: process.terminationStatus,
            stdout: stdoutBox.get(),
            stderr: stderrBox.get()
        )
    }

    private var isCancelled: Bool {
        lock.lock()
        defer { lock.unlock() }
        return cancelled
    }

    private func throwIfCancelled() throws {
        if isCancelled { throw ProcessError.cancelled }
    }

    /// `Process.terminate()` en Linux a veces no corta el hijo.
    /// SIGKILL sí. El timeout de la spec (operaciones largas cancelables) depende de esto.
    private static func stop(_ process: Process) {
        process.terminate()
        if process.isRunning, process.processIdentifier > 0 {
            kill(process.processIdentifier, SIGKILL)
        }
    }

    private func resolve(_ executable: String, path: String) throws -> URL {
        if executable.contains("/") {
            guard FileManager.default.isExecutableFile(atPath: executable) else {
                throw ProcessError.executableNotFound(executable)
            }
            return URL(fileURLWithPath: executable)
        }
        for directory in path.split(separator: ":") where !directory.isEmpty {
            let candidate = "\(directory)/\(executable)"
            if FileManager.default.isExecutableFile(atPath: candidate) {
                return URL(fileURLWithPath: candidate)
            }
        }
        throw ProcessError.executableNotFound(executable)
    }
}

private final class LockedData: @unchecked Sendable {
    private let lock = NSLock()
    private var data = Data()

    func set(_ data: Data) {
        lock.lock()
        self.data = data
        lock.unlock()
    }

    func get() -> Data {
        lock.lock()
        defer { lock.unlock() }
        return data
    }
}
