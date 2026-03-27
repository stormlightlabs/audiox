import Dispatch
import Foundation
import FoundationModels

private struct ProbePayload: Codable {
    let available: Bool
    let reason: String?
    let supportsLocale: Bool
}

private struct MetadataPayload: Codable {
    let title: String?
    let summary: String?
    let tags: [String]
    let error: String?
}

private func exportJSON<T: Encodable>(_ value: T) -> UnsafeMutablePointer<CChar>? {
    let encoder = JSONEncoder()
    guard let data = try? encoder.encode(value), let string = String(data: data, encoding: .utf8) else {
        return strdup("{\"error\":\"failed_to_encode_bridge_payload\"}")
    }

    return strdup(string)
}

@available(macOS 26.0, *)
private func probePayload() -> ProbePayload {
    let model = SystemLanguageModel(useCase: .general)
    let supportsLocale = model.supportsLocale(Locale.current)

    switch model.availability {
    case .available:
        if supportsLocale {
            return ProbePayload(available: true, reason: nil, supportsLocale: true)
        }
        return ProbePayload(available: false, reason: "unsupported_locale", supportsLocale: false)
    case .unavailable(let reason):
        let normalizedReason = switch reason {
        case .deviceNotEligible:
            "device_not_eligible"
        case .appleIntelligenceNotEnabled:
            "apple_intelligence_not_enabled"
        case .modelNotReady:
            "model_not_ready"
        @unknown default:
            "unknown_unavailable_reason"
        }
        return ProbePayload(available: false, reason: normalizedReason, supportsLocale: supportsLocale)
    }
}

@_cdecl("murmur_probe_apple_intelligence")
public func murmur_probe_apple_intelligence() -> UnsafeMutablePointer<CChar>? {
    if #available(macOS 26.0, *) {
        return exportJSON(probePayload())
    }

    return exportJSON(ProbePayload(available: false, reason: "foundation_models_requires_macos_26", supportsLocale: false))
}

@available(macOS 26.0, *)
private func generateMetadataPayload(transcript: String, fallbackTitle: String) async -> MetadataPayload {
    let probe = probePayload()
    guard probe.available else {
        return MetadataPayload(title: nil, summary: nil, tags: [], error: probe.reason ?? "apple_intelligence_unavailable")
    }

    let clippedTranscript = String(transcript.prefix(16_000))
    let prompt = """
    Extract document metadata from this transcript.

    Return:
    - a concise title
    - a 2 to 3 sentence summary
    - 3 to 7 short lowercase tags

    If the transcript is sparse, use "\(fallbackTitle)" as context for the title.

    Transcript:
    \(clippedTranscript)
    """

    let model = SystemLanguageModel(useCase: .general)
    let session = LanguageModelSession(
        model: model,
        instructions: "You extract concise metadata for personal knowledge management. Return only grounded results from the transcript."
    )

    do {
        let schema = try GenerationSchema(
            root: DynamicGenerationSchema(
                name: "TranscriptMetadata",
                description: "Structured metadata for a transcript.",
                properties: [
                    .init(
                        name: "title",
                        description: "A concise descriptive title no longer than 12 words.",
                        schema: DynamicGenerationSchema(type: String.self)
                    ),
                    .init(
                        name: "summary",
                        description: "A clear summary using exactly 2 or 3 sentences.",
                        schema: DynamicGenerationSchema(type: String.self)
                    ),
                    .init(
                        name: "tags",
                        description: "Between 3 and 7 short lowercase keyword tags without hashtags.",
                        schema: DynamicGenerationSchema(
                            arrayOf: DynamicGenerationSchema(type: String.self),
                            minimumElements: 3,
                            maximumElements: 7
                        )
                    )
                ]
            ),
            dependencies: []
        )
        let response = try await session.respond(
            to: prompt,
            schema: schema,
            options: GenerationOptions(temperature: 0.2, maximumResponseTokens: 256)
        )
        let title = try response.content.value(String.self, forProperty: "title")
            .trimmingCharacters(in: CharacterSet.whitespacesAndNewlines)
        let summary = try response.content.value(String.self, forProperty: "summary")
            .trimmingCharacters(in: CharacterSet.whitespacesAndNewlines)
        let tags = try response.content.value([String].self, forProperty: "tags")
            .map { $0.trimmingCharacters(in: CharacterSet.whitespacesAndNewlines).lowercased() }
            .filter { !$0.isEmpty }

        return MetadataPayload(
            title: title.isEmpty ? nil : title,
            summary: summary.isEmpty ? nil : summary,
            tags: tags,
            error: nil
        )
    } catch {
        return MetadataPayload(title: nil, summary: nil, tags: [], error: error.localizedDescription)
    }
}

@_cdecl("murmur_generate_apple_metadata")
public func murmur_generate_apple_metadata(
    _ transcriptPtr: UnsafePointer<CChar>?,
    _ fallbackTitlePtr: UnsafePointer<CChar>?
) -> UnsafeMutablePointer<CChar>? {
    guard let transcriptPtr else {
        return exportJSON(MetadataPayload(title: nil, summary: nil, tags: [], error: "missing_transcript"))
    }

    let transcript = String(cString: transcriptPtr)
    let fallbackTitle = fallbackTitlePtr.map(String.init(cString:)) ?? ""

    if #available(macOS 26.0, *) {
        let semaphore = DispatchSemaphore(value: 0)
        var payload = MetadataPayload(title: nil, summary: nil, tags: [], error: "generation_cancelled")

        Task {
            payload = await generateMetadataPayload(transcript: transcript, fallbackTitle: fallbackTitle)
            semaphore.signal()
        }

        semaphore.wait()
        return exportJSON(payload)
    }

    return exportJSON(MetadataPayload(title: nil, summary: nil, tags: [], error: "foundation_models_requires_macos_26"))
}

@_cdecl("murmur_free_bridge_string")
public func murmur_free_bridge_string(_ pointer: UnsafeMutablePointer<CChar>?) {
    free(pointer)
}
