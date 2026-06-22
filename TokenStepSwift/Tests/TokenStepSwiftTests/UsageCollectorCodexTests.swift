import Foundation
import XCTest
@testable import TokenStepSwift

final class UsageCollectorCodexTests: XCTestCase {
    func testDefaultCodexCollectorIgnoresArchivedSessions() throws {
        let home = FileManager.default.temporaryDirectory
            .appendingPathComponent("TokenStepCodexTests-\(UUID().uuidString)", isDirectory: true)
        let liveRoot = home.appendingPathComponent(".codex/sessions/2026/06/22", isDirectory: true)
        let archivedRoot = home.appendingPathComponent(".codex/archived_sessions/2026/06/22", isDirectory: true)
        try FileManager.default.createDirectory(at: liveRoot, withIntermediateDirectories: true)
        try FileManager.default.createDirectory(at: archivedRoot, withIntermediateDirectories: true)
        addTeardownBlock {
            try? FileManager.default.removeItem(at: home)
        }

        try codexLines(sessionID: "live-session", totalTokens: 120)
            .joined(separator: "\n")
            .write(to: liveRoot.appendingPathComponent("live.jsonl"), atomically: true, encoding: .utf8)
        try codexLines(sessionID: "archived-session", totalTokens: 900_000_000)
            .joined(separator: "\n")
            .write(to: archivedRoot.appendingPathComponent("archived.jsonl"), atomically: true, encoding: .utf8)

        let snapshot = UsageCollector.collectCodexUsageSnapshotForTests(homeURL: home)

        XCTAssertEqual(snapshot.sources["Codex"]?.status, "ok")
        XCTAssertEqual(snapshot.sources["Codex"]?.files, 1)
        XCTAssertEqual(snapshot.sources["Codex"]?.records, 1)
        XCTAssertEqual(snapshot.totals.tokens, 120)
        XCTAssertEqual(snapshot.daily.count, 1)
        XCTAssertEqual(snapshot.daily.first?.date, "2026-06-22")
        XCTAssertEqual(snapshot.daily.first?.tools["Codex"], 120)
    }

    private func codexLines(sessionID: String, totalTokens: Int) -> [String] {
        [
            jsonLine([
                "type": "session_meta",
                "timestamp": "2026-06-22T05:00:00Z",
                "payload": ["id": sessionID]
            ]),
            jsonLine([
                "type": "turn_context",
                "timestamp": "2026-06-22T05:00:00Z",
                "payload": ["model": "gpt-5"]
            ]),
            jsonLine([
                "type": "event_msg",
                "timestamp": "2026-06-22T05:00:00Z",
                "payload": [
                    "type": "token_count",
                    "info": [
                        "last_token_usage": [
                            "total_tokens": totalTokens
                        ]
                    ]
                ]
            ])
        ]
    }

    private func jsonLine(_ object: [String: Any]) -> String {
        let data = try! JSONSerialization.data(withJSONObject: object, options: [.sortedKeys])
        return String(data: data, encoding: .utf8)!
    }
}
