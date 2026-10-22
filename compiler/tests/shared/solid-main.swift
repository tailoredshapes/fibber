// compiler/tests/shared/solid-main.swift: Swift calls render(text) of libsolid.dylib through the generated module map (`import solid`): the JSON of each model given as an argument, then a newline.
import Foundation
import solid

for model in CommandLine.arguments.dropFirst() {
    var cap = 64, json = ""
    while true {
        var buf = [UInt8](repeating: 0, count: cap)
        let need = buf.withUnsafeMutableBufferPointer { o in
            model.withCString { p in p.withMemoryRebound(to: UInt8.self, capacity: Int(strlen(p))) { render($0, Int64(strlen(p)), o.baseAddress, Int64(cap)) } }
        }
        if need <= Int64(cap) { json = String(decoding: buf[0..<Int(need)], as: UTF8.self); break }
        cap = Int(need)
    }
    print(json)
}
