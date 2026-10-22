// compiler/tests/shared/main.swift: a Swift command-line tool that `import kernels` (the module map `fibc build --shared --header` wrote beside kernels.h) and calls every
// export of libkernels.dylib; it prints the lines host.c prints, which shared-mac.sh compares with `fibc run` of oracle.fib. The string helpers are the retry rule of the ABI.
import Foundation
import kernels

var failed = false
func check(_ ok: Bool, _ what: String) { if !ok { FileHandle.standardError.write(Data("CHECK FAILED: \(what)\n".utf8)); failed = true } }

// a str parameter is (pointer, length); a str result goes to (out, cap) and the call returns the length it needs: call again with a buffer of that length when it is above cap
func withBytes<R>(_ s: String, _ f: (UnsafePointer<UInt8>?, Int64) -> R) -> R {
    let u = Array(s.utf8)
    return u.withUnsafeBufferPointer { f($0.baseAddress, Int64(u.count)) }
}
func retry(_ body: (UnsafeMutablePointer<UInt8>?, Int64) -> Int64) -> (String, Int) {
    var cap = 4, calls = 0
    while true {
        var buf = [UInt8](repeating: 0, count: cap)
        let need = buf.withUnsafeMutableBufferPointer { body($0.baseAddress, Int64(cap)) }
        calls += 1
        if need <= Int64(cap) { return (String(decoding: buf[0..<Int(need)], as: UTF8.self), calls) }
        cap = Int(need)
    }
}
func f1(_ x: Double) -> String { String(format: "%.1f", x) }

print("add=\(add(40, 2))")
print("widths=\(widths(100000, -5, 300))")
print("narrow=\(narrow(7))")
print("scale=\(f1(scale(2.5, 3.0)))")
print("half32=\(f1(Double(half32(5.0))))")
print("both=\(both(1, 1) == 1 ? "true" : "false"),\(both(1, 0) == 1 ? "true" : "false")")
let xs: [Double] = [1, 2, 3], ys: [Double] = [4, 5, 6]
check(xs.withUnsafeBufferPointer { a in ys.withUnsafeBufferPointer { b in dot(UnsafeMutableRawPointer(mutating: a.baseAddress!), UnsafeMutableRawPointer(mutating: b.baseAddress!), 3) } } == 32.0, "dot")
print("sum_squares=\(sum_squares(1000))")
var v = [Int64](repeating: 9, count: 5)
v.withUnsafeMutableBytes { iota($0.baseAddress, 4) }
check(v == [0, 1, 2, 3, 9], "iota writes n elements and no more")

print("greet=\(retry { o, c in withBytes("world") { p, n in greet(p, n, o, c) } }.0)")
print("join2=\(retry { o, c in withBytes("left") { p, n in withBytes("right") { q, m in join2(p, n, q, m, o, c) } } }.0)")
print("count_bytes=\(withBytes("héllo") { p, n in count_bytes(p, n) })")
print("banner=\(retry { o, c in banner(o, c) }.0)")
print("echo=\(retry { o, c in withBytes("same") { p, n in echo(p, n, o, c) } }.0)")
print("mixed=\(retry { o, c in withBytes("seven") { p, n in mixed(7, p, n, 1.5, o, c) } }.0)")
print("empty_str=[\(retry { o, c in empty_str(o, c) }.0)]")

let (g, calls) = retry { o, c in withBytes("retry") { p, n in greet(p, n, o, c) } }
check(g == "hello, retry!" && calls == 2, "the retry rule: the first call (cap 4) is too small, the second fits")
var out = [UInt8](repeating: 0xAA, count: 16)
let need = out.withUnsafeMutableBufferPointer { o in withBytes("world") { p, n in greet(p, n, o.baseAddress, 5) } }
check(need == 13 && Array(out[0..<5]) == Array("hello".utf8) && out[5] == 0xAA, "truncation: the first cap bytes, the full length returned, nothing past cap")
check(withBytes("world") { p, n in greet(p, n, nil, 0) } == 13, "a nil buffer returns the length")
check(count_bytes(nil, 0) == 0, "nil with length 0 is the empty string")
for _ in 0..<100000 { _ = retry { o, c in withBytes(String(repeating: "q", count: 1000)) { p, n in echo(p, n, o, c) } } }
exit(failed ? 1 : 0)
