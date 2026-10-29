import XCTest
@testable import GitCore

final class ParserTests: XCTestCase {
    func testPorcelainOrdinaryUntrackedAndBranchHeaders() {
        let sample = "# branch.oid abcdef\0# branch.head main\0# branch.upstream origin/main\0# branch.ab +1 -2\01 .M N... 100644 100644 100644 aaa bbb file.txt\0? sub/weird name.txt\0"
        let status = PorcelainParser.parse(sample)
        XCTAssertEqual(status.branchName, "main")
        XCTAssertEqual(status.headOID, "abcdef")
        XCTAssertEqual(status.upstream, "origin/main")
        XCTAssertEqual(status.ahead, 1)
        XCTAssertEqual(status.behind, 2)
        XCTAssertFalse(status.isDetached)
        XCTAssertEqual(status.entries.count, 2)

        let modified = status.entries[0]
        XCTAssertEqual(modified.path, "file.txt")
        XCTAssertEqual(modified.indexStatus, .unmodified)
        XCTAssertEqual(modified.workTreeStatus, .modified)
        XCTAssertTrue(modified.hasUnstagedChange)
        XCTAssertFalse(modified.isStaged)

        let untracked = status.entries[1]
        XCTAssertEqual(untracked.path, "sub/weird name.txt")
        XCTAssertTrue(untracked.isUntracked)
    }

    func testPorcelainRenameWithSpacesAndUnicode() {
        let sample = "2 R. N... 100644 100644 100644 aaa bbb R100 notas/cañón archivo.txt\0viejo nombre.txt\0"
        let entry = PorcelainParser.parse(sample).entries[0]
        XCTAssertEqual(entry.path, "notas/cañón archivo.txt")
        XCTAssertEqual(entry.originalPath, "viejo nombre.txt")
        XCTAssertEqual(entry.indexStatus, .renamed)
        XCTAssertFalse(entry.path.contains("\0"))
    }

    func testPorcelainRenameAndConflict() {
        let rename = "2 RM N... 100644 100644 100644 aaa bbb R100 renamed.txt\0file.txt\0"
        let renamed = PorcelainParser.parse(rename).entries[0]
        XCTAssertEqual(renamed.path, "renamed.txt")
        XCTAssertEqual(renamed.originalPath, "file.txt")
        XCTAssertEqual(renamed.indexStatus, .renamed)
        XCTAssertEqual(renamed.workTreeStatus, .modified)
        XCTAssertTrue(renamed.isStaged)
        XCTAssertTrue(renamed.hasUnstagedChange)

        let conflict = "u UU N... 100644 100644 100644 100644 a b c conflict.txt\0"
        let entry = PorcelainParser.parse(conflict).entries[0]
        XCTAssertTrue(entry.isConflicted)
        XCTAssertEqual(entry.conflictKind, .bothModified)
        XCTAssertFalse(entry.isStaged)
    }

    func testPorcelainDetachedAndUnborn() {
        let detached = "# branch.oid deadbeef\0# branch.head (detached)\0"
        let status = PorcelainParser.parse(detached)
        XCTAssertTrue(status.isDetached)
        XCTAssertNil(status.branchName)
        XCTAssertEqual(status.headOID, "deadbeef")

        let unborn = "# branch.oid (initial)\0# branch.head main\0"
        let fresh = PorcelainParser.parse(unborn)
        XCTAssertTrue(fresh.isUnborn)
        XCTAssertNil(fresh.headOID)
        XCTAssertEqual(fresh.branchName, "main")
    }

    func testCommitLogParserKeepsMultilineBody() {
        let raw = "abc\0parent\0Ada\0ada@example.com\02026-01-02T03:04:05Z\0asunto\0cuerpo\nlinea 2\0HEAD -> main\u{1e}"
            .replacingOccurrences(of: "\0", with: "\u{1f}")
        let commits = CommitLogParser.parse(raw)
        XCTAssertEqual(commits.count, 1)
        XCTAssertEqual(commits[0].subject, "asunto")
        XCTAssertEqual(commits[0].body, "cuerpo\nlinea 2")
        XCTAssertEqual(commits[0].parents, ["parent"])
        XCTAssertEqual(commits[0].refs, ["HEAD -> main"])
        XCTAssertEqual(commits[0].shortSHA, "abc")
    }

    func testBranchTrackParser() {
        XCTAssertEqual(BranchListParser.parseTrack(" [ahead 2, behind 3]").ahead, 2)
        XCTAssertEqual(BranchListParser.parseTrack(" [ahead 2, behind 3]").behind, 3)
        let text = "main\0abc\0*\0origin/main\0 [ahead 1]\nfeature\0def\0 \0\0\n"
        let branches = BranchListParser.parse(text)
        XCTAssertEqual(branches.count, 2)
        XCTAssertTrue(branches[0].isHead)
        XCTAssertEqual(branches[0].upstream, "origin/main")
        XCTAssertEqual(branches[0].ahead, 1)
        XCTAssertFalse(branches[1].isHead)
        XCTAssertNil(branches[1].upstream)
    }

    func testRemoteParser() {
        let text = "origin\t/tmp/bare (fetch)\norigin\t/tmp/bare (push)\nbackup\tgit@example:repo.git (fetch)\n"
        let remotes = RemoteListParser.parse(text)
        XCTAssertEqual(remotes.map(\.name), ["backup", "origin"])
        XCTAssertEqual(remotes.first { $0.name == "origin" }?.fetchURL, "/tmp/bare")
        XCTAssertEqual(remotes.first { $0.name == "origin" }?.pushURL, "/tmp/bare")
    }

    func testDiffParserKeepsCRLFHunksSeparate() {
        let raw = "diff --git a/crlf.txt b/crlf.txt\n--- a/crlf.txt\n+++ b/crlf.txt\n@@ -1,3 +1,3 @@\n line1\r\n-line2\r\n+CHANGED\r\n@@ -8,3 +8,3 @@\n line8\r\n-line9\r\n+LATE\r\n"
        let files = DiffParser.parse(raw)
        XCTAssertEqual(files.count, 1)
        XCTAssertEqual(files[0].hunks.count, 2)
        XCTAssertEqual(files[0].hunks[0].lines.first(where: { $0.kind == .addition })?.text, "CHANGED\r")
    }

    func testDiffParserBinaryPathWithSpacesDoesNotSwallowTheOtherSide() throws {
        let raw = """
        diff --git a/notas/cañón archivo.txt b/notas/cañón archivo.txt
        Binary files a/notas/cañón archivo.txt and b/notas/cañón archivo.txt differ
        """
        let file = try XCTUnwrap(DiffParser.parse(raw).first)
        XCTAssertEqual(file.path, "notas/cañón archivo.txt")
        XCTAssertNil(file.oldPath)
        XCTAssertTrue(file.isBinary)
        XCTAssertFalse(file.path.contains(" b/"))

        let rename = "diff --git a/viejo nombre.txt b/nuevo nombre.txt\n"
        let renamed = try XCTUnwrap(DiffParser.parse(rename).first)
        XCTAssertEqual(renamed.oldPath, "viejo nombre.txt")
        XCTAssertEqual(renamed.path, "nuevo nombre.txt")
    }

    func testDiffParserTrimsTabAfterSpacedPath() {
        let raw = "diff --git a/notas/cañón archivo.txt b/notas/cañón archivo.txt\n--- a/notas/cañón archivo.txt\t\n+++ b/notas/cañón archivo.txt\t\n@@ -1,2 +1,2 @@\n line1\n-line2\n+LINE2\n"
        let files = DiffParser.parse(raw)
        XCTAssertEqual(files.first?.path, "notas/cañón archivo.txt")
    }

    func testDiffParserTwoHunks() {
        let diff = """
        diff --git a/file.txt b/file.txt
        index 111..222 100644
        --- a/file.txt
        +++ b/file.txt
        @@ -1,3 +1,3 @@
         line1
        -line2
        +LINE2
         line3
        @@ -20,3 +20,3 @@
         line20
        -line21
        +LINE21
         line22
        """
        let files = DiffParser.parse(diff)
        XCTAssertEqual(files.count, 1)
        XCTAssertEqual(files[0].path, "file.txt")
        XCTAssertEqual(files[0].hunks.count, 2)
        XCTAssertEqual(files[0].hunks[0].oldStart, 1)
        XCTAssertEqual(files[0].hunks[0].newStart, 1)
        XCTAssertEqual(files[0].hunks[0].lines.filter { $0.kind == .addition }.map(\.text), ["LINE2"])
        XCTAssertTrue(files[0].hunks[0].unifiedText.hasPrefix("@@ -1,3 +1,3 @@"))
        let patch = files[0].patch(selecting: files[0].hunks[0])
        XCTAssertTrue(patch.contains("diff --git"))
        XCTAssertTrue(patch.contains("+LINE2"))
        XCTAssertFalse(patch.contains("+LINE21"))
    }

    func testConflictMarkersResolveOneHunk() {
        let text = """
        line1
        <<<<<<< HEAD
        from-main
        ||||||| base
        line2
        =======
        from-feature
        >>>>>>> feature
        line3
        """
        let hunks = ConflictMarkers.hunks(in: text)
        XCTAssertEqual(hunks.count, 1)
        XCTAssertEqual(hunks[0].ours, "from-main\n")
        XCTAssertEqual(hunks[0].base, "line2\n")
        XCTAssertEqual(hunks[0].theirs, "from-feature\n")

        let resolved = ConflictMarkers.resolve(text, choices: [0: .ours])
        XCTAssertEqual(resolved, "line1\nfrom-main\nline3")
        XCTAssertFalse(resolved.contains("<<<<<<<"))

        let unchanged = ConflictMarkers.resolve(text, choices: [:])
        XCTAssertTrue(unchanged.contains("<<<<<<<"))
    }

    func testUnmergedIndexParser() {
        let text = "100644 a3d 1\tfile.txt\0100644 30fd 2\tfile.txt\0100644 82c1 3\tfile.txt\0"
        let stages = UnmergedIndexParser.parse(text)
        XCTAssertEqual(stages.count, 3)
        XCTAssertEqual(stages.map(\.stage), [1, 2, 3])
        XCTAssertEqual(stages.map(\.path), ["file.txt", "file.txt", "file.txt"])
    }
}
