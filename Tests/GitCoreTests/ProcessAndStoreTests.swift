import XCTest
@testable import GitCore

final class ProcessAndStoreTests: XCTestCase {
    func testSanitizedEnvironmentDropsTokenLikeVariables() {
        let cleaned = ProcessEnvironment.sanitized([
            "PATH": "/usr/bin",
            "HOME": "/home/javier",
            "GIT_AUTHOR_NAME": "Javier",
            "GIT_TOKEN": "no-debe-pasar",
            "OPENAI_API_KEY": "tampoco",
            "SSH_AUTH_SOCK": "/tmp/agent",
            "AWS_SECRET_ACCESS_KEY": "secreto",
            "RANDOM_THING": "no",
        ])
        XCTAssertEqual(cleaned["PATH"], "/usr/bin")
        XCTAssertEqual(cleaned["HOME"], "/home/javier")
        XCTAssertEqual(cleaned["GIT_AUTHOR_NAME"], "Javier")
        XCTAssertEqual(cleaned["SSH_AUTH_SOCK"], "/tmp/agent")
        XCTAssertNil(cleaned["GIT_TOKEN"])
        XCTAssertNil(cleaned["OPENAI_API_KEY"])
        XCTAssertNil(cleaned["AWS_SECRET_ACCESS_KEY"])
        XCTAssertNil(cleaned["RANDOM_THING"])
    }

    func testProcessRunnerEchoAndStdin() async throws {
        let runner = FoundationProcessRunner()
        let echo = try await runner.run(ProcessRequest(executable: "/bin/echo", arguments: ["hola"]))
        XCTAssertEqual(echo.exitCode, 0)
        XCTAssertEqual(echo.stdoutString, "hola\n")

        let cat = try await runner.run(
            ProcessRequest(executable: "/bin/cat", stdin: Data("abc".utf8), timeout: 5)
        )
        XCTAssertEqual(cat.stdoutString, "abc")
    }

    func testProcessRunnerTimeoutAndMissingBinary() async throws {
        let runner = FoundationProcessRunner()
        do {
            _ = try await runner.run(ProcessRequest(executable: "/bin/sleep", arguments: ["5"], timeout: 0.3))
            XCTFail("el sleep tenía que cortarse")
        } catch let error as ProcessError {
            guard case .timedOut = error else {
                return XCTFail("esperaba timeout, llegó \(error)")
            }
        }

        do {
            _ = try await runner.run(ProcessRequest(executable: "no-existe-gitforge-bin"))
            XCTFail("tenía que faltar el binario")
        } catch let error as ProcessError {
            guard case .executableNotFound = error else {
                return XCTFail("esperaba executableNotFound, llegó \(error)")
            }
        }
    }

    func testRecentRepositoryRoundTrip() throws {
        let url = FileManager.default.temporaryDirectory.appendingPathComponent("gitforge-recents-\(UUID().uuidString).json")
        defer { try? FileManager.default.removeItem(at: url) }
        let store = RecentRepositoryStore(fileURL: url)
        let repo = Repository(path: "/tmp/demo", gitDir: "/tmp/demo/.git")
        let date = Date(timeIntervalSince1970: 1_700_000_000)
        try store.remember(repo, branch: "main", at: date)
        try store.setFavorite(path: repo.path, isFavorite: true)
        try store.remember(repo, branch: "feature", at: date.addingTimeInterval(60))
        let loaded = try store.load()
        XCTAssertEqual(loaded.count, 1)
        XCTAssertEqual(loaded[0].name, "demo")
        XCTAssertEqual(loaded[0].lastBranch, "feature")
        XCTAssertTrue(loaded[0].isFavorite)
        XCTAssertEqual(loaded[0].lastOpened.timeIntervalSince1970, date.addingTimeInterval(60).timeIntervalSince1970, accuracy: 1)
    }
}
