import Foundation
import Vision
import AppKit

// Reads text in pictures with macOS's built-in Vision framework. Nothing is
// sent anywhere: the recognition runs on this computer.
//
// This program stays running and answers requests, one per line on stdin:
//     <image path> \t <x y w h [fast]> \t <x y w h [fast]> ...   (crops in pixels, origin top-left)
// For each crop (or the whole picture when none are given) it prints one JSON
// object per recognised line, tagged with the crop's index "r", then a line
// {"done":true}.

func recognise(_ cg: CGImage, region: Int, fast: Bool) {
    let request = VNRecognizeTextRequest()
    request.recognitionLevel = fast ? .fast : .accurate
    request.usesLanguageCorrection = false
    let handler = VNImageRequestHandler(cgImage: cg, options: [:])
    guard (try? handler.perform([request])) != nil else { return }
    let H = Double(cg.height), W = Double(cg.width)
    for obs in (request.results ?? []) {
        guard let top = obs.topCandidates(1).first else { continue }
        let b = obs.boundingBox
        let line: [String: Any] = ["r": region, "text": top.string, "conf": top.confidence,
            "x": b.minX * W, "y": (1 - b.maxY) * H, "w": b.width * W, "h": b.height * H]
        if let data = try? JSONSerialization.data(withJSONObject: line),
           let text = String(data: data, encoding: .utf8) { print(text) }
    }
}

while let request = readLine() {
    let parts = request.components(separatedBy: "\t")
    if let path = parts.first, let image = NSImage(contentsOfFile: path),
       let cg = image.cgImage(forProposedRect: nil, context: nil, hints: nil) {
        let crops = parts.dropFirst()
        if crops.isEmpty {
            recognise(cg, region: 0, fast: false)
        } else {
            for (i, crop) in crops.enumerated() {
                let n = crop.split(separator: " ").compactMap { Double($0) }
                if n.count >= 4, let piece = cg.cropping(to: CGRect(x: n[0], y: n[1], width: n[2], height: n[3])) {
                    // A fifth number of 1 asks for quick recognition (short labels).
                    recognise(piece, region: i, fast: n.count >= 5 && n[4] == 1)
                }
            }
        }
    }
    print("{\"done\":true}")
    fflush(stdout)
}
