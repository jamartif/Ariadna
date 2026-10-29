import Foundation
import GitCore

enum CLIAvailability {
    static func probe(
        runner: any ProcessRunning,
        provider: AIProviderID,
        binary: String,
        directory: URL,
        missingHint: String,
        timeout: TimeInterval
    ) async -> AIAvailability {
        do {
            let output = try await runner.run(
                ProcessRequest(
                    executable: binary,
                    arguments: ["--version"],
                    currentDirectory: directory,
                    timeout: timeout
                )
            )
            if output.exitCode == 0 {
                return AIAvailability(provider: provider, state: .available(binary: binary))
            }
            let detail = output.stderrString.isEmpty ? output.stdoutString : output.stderrString
            if AIAuth.isAuthenticationFailure(detail) {
                return AIAvailability(
                    provider: provider,
                    state: .failed(binary: binary, detail: "Ejecuta `\(provider.loginCommand)`. \(GitErrorTruncation.truncate(detail, limit: 400))")
                )
            }
            return AIAvailability(
                provider: provider,
                state: .failed(binary: binary, detail: GitErrorTruncation.truncate(detail, limit: 400))
            )
        } catch let error as ProcessError {
            switch error {
            case .executableNotFound:
                return AIAvailability(provider: provider, state: .missing(binary: binary, hint: missingHint))
            case .timedOut:
                return AIAvailability(provider: provider, state: .failed(binary: binary, detail: "Timeout al comprobar \(binary)."))
            case .cancelled:
                return AIAvailability(provider: provider, state: .failed(binary: binary, detail: "Comprobación cancelada."))
            case .failedToLaunch(let message):
                return AIAvailability(provider: provider, state: .failed(binary: binary, detail: message))
            }
        } catch {
            return AIAvailability(provider: provider, state: .failed(binary: binary, detail: String(describing: error)))
        }
    }
}

enum CLIInvocation {
    static func run(
        runner: any ProcessRunning,
        provider: AIProviderID,
        binary: String,
        arguments: [String],
        directory: URL,
        timeout: TimeInterval,
        loginHint: String
    ) async throws -> ProcessOutput {
        let output: ProcessOutput
        do {
            output = try await runner.run(
                ProcessRequest(
                    executable: binary,
                    arguments: arguments,
                    currentDirectory: directory,
                    timeout: timeout
                )
            )
        } catch let error as ProcessError {
            switch error {
            case .executableNotFound:
                throw AIError.providerUnavailable(provider, hint: "No encuentro `\(binary)`. \(loginHint)")
            case .timedOut:
                throw AIError.timedOut(provider)
            case .cancelled:
                throw AIError.cancelled
            case .failedToLaunch(let message):
                throw AIError.providerFailed(provider, exitCode: -1, detail: message)
            }
        } catch is CancellationError {
            throw AIError.cancelled
        }
        let blob = output.stdoutString + "\n" + output.stderrString
        if AIAuth.isAuthenticationFailure(blob) {
            throw AIError.notAuthenticated(provider, hint: loginHint)
        }
        guard output.exitCode == 0 else {
            let detail = output.stderrString.isEmpty ? output.stdoutString : output.stderrString
            throw AIError.providerFailed(provider, exitCode: output.exitCode, detail: detail)
        }
        return output
    }
}
