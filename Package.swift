// swift-tools-version: 6.0
//
// Ariadna. Destino de despliegue: macOS 15.
// En Linux el manifiesto no fija plataformas: SwiftPM rechaza la lista vacía
// y un destino solo-Apple a veces impide `swift test` del núcleo.
//
// La capa de IA (Copilot/Codex) es Fase 3. No entra en el build salvo que
// se exporte ARIADNA_AI=1 antes de `swift build` / `swift test`.

import PackageDescription

let enableAI = Context.environment["ARIADNA_AI"] == "1"

var products: [Product] = [
    .library(name: "GitCore", targets: ["GitCore"]),
    .executable(name: "Ariadna", targets: ["App"]),
]

var targets: [Target] = [
    .target(name: "GitCore", path: "Sources/GitCore"),
    .executableTarget(
        name: "App",
        dependencies: enableAI ? ["GitCore", "AriadnaAI"] : ["GitCore"],
        path: "Sources/App",
        exclude: ["Info.plist"],
        swiftSettings: enableAI ? [.define("ARIADNA_AI")] : []
    ),
    .testTarget(
        name: "GitCoreTests",
        dependencies: ["GitCore"],
        path: "Tests/GitCoreTests"
    ),
]

if enableAI {
    products.append(.library(name: "AriadnaAI", targets: ["AriadnaAI"]))
    targets.append(
        .target(
            name: "AriadnaAI",
            dependencies: ["GitCore"],
            path: "Sources/AriadnaAI"
        )
    )
    targets.append(
        .testTarget(
            name: "AriadnaAITests",
            dependencies: ["AriadnaAI", "GitCore"],
            path: "Tests/AriadnaAITests"
        )
    )
}

#if os(macOS)
let package = Package(
    name: "Ariadna",
    platforms: [.macOS(.v15)],
    products: products,
    targets: targets
)
#else
let package = Package(
    name: "Ariadna",
    products: products,
    targets: targets
)
#endif
