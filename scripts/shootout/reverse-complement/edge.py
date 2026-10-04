import random, sys
random.seed(7)
alpha = "ACGTUMRWSYKVHDBNacgtumrwsykvhdbn"
def seq(n, w):
    s = "".join(random.choice(alpha) for _ in range(n))
    return "".join(s[i:i+w] + "\n" for i in range(0, n, w))
# a header straddling the 65536 boundary, an empty sequence, irregular line widths, a last line with no newline
out = ">first\n" + seq(65536 - 7 - 5, 61)
out += ">straddling header that is long enough to cross the block edge\n" + seq(130000, 7)
out += ">empty\n>after empty\n" + seq(59, 60) + seq(60, 60) + seq(61, 59)
out += ">tail no newline\n" + seq(121, 60).rstrip("\n")
open(sys.argv[1] + "/edge1.txt", "w").write(out)
# a '>' exactly on the first byte of the second block
head = ">x\n"
body = seq(65536 - len(head) - 1 - 1000, 60)
pad = "A" * (65536 - len(head) - len(body))
out2 = head + body + pad
out2 = out2[:65535] + "\n>second\n" + seq(500, 60)
open(sys.argv[1] + "/edge2.txt", "w").write(out2)
