// swift-tools-version: 6.0
//
// El manifiesto se evalúa en la máquina que compila.
// En macOS fijamos el despliegue a macOS 14 (Sonoma), que es el objetivo del spec.
// En Linux dejamos la lista vacía para que `swift build` / `swift test` no exijan SDK de Apple.

import PackageDescription

let products: [Product] = [
    .library(name: "GitForgeCore", targets: ["GitForgeCore"]),
    .library(name: "GitForgeAI", targets: ["GitForgeAI"]),
    .executable(name: "GitForge", targets: ["GitForgeApp"]),
]

let targets: [Target] = [
    .target(name: "GitForgeCore", path: "Sources/GitForgeCore"),
    .target(
        name: "GitForgeAI",
        dependencies: ["GitForgeCore"],
        path: "Sources/GitForgeAI"
    ),
    .executableTarget(
        name: "GitForgeApp",
        dependencies: ["GitForgeCore", "GitForgeAI"],
        path: "Sources/GitForgeApp"
    ),
    .testTarget(
        name: "GitForgeCoreTests",
        dependencies: ["GitForgeCore"],
        path: "Tests/GitForgeCoreTests"
    ),
    .testTarget(
        name: "GitForgeAITests",
        dependencies: ["GitForgeAI", "GitForgeCore"],
        path: "Tests/GitForgeAITests"
    ),
]

#if os(macOS)
let package = Package(
    name: "GitForge",
    platforms: [.macOS(.v14)],
    products: products,
    targets: targets
)
#else
let package = Package(
    name: "GitForge",
    products: products,
    targets: targets
)
#endif
