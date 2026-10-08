import sys, struct
# lzstat.py FRAME.lz4 : sequence statistics of the blocks of an LZ4 frame
d = open(sys.argv[1], 'rb').read()
flg = d[4]; p = 6
if flg & 8: p += 8
if flg & 1: p += 4
p += 1
bchk = (flg >> 4) & 1
nseq = 0; slow_lit = 0; slow_ml = 0; off_lt8 = 0; off_lt16 = 0; litsum = 0; mlsum = 0; outb = 0; inb = 0
lithist = [0] * 16
while True:
    w = struct.unpack_from('<I', d, p)[0]; p += 4
    if w == 0: break
    sz = w & 0x7fffffff
    if w & 0x80000000:
        p += sz + (4 if bchk else 0); continue
    b = d[p:p + sz]; p += sz + (4 if bchk else 0)
    i = 0
    while i < sz:
        t = b[i]; i += 1
        lit = t >> 4; ml = t & 15
        if lit == 15:
            while True:
                e = b[i]; i += 1; lit += e
                if e != 255: break
        i += lit
        nseq += 1; litsum += lit; lithist[min(lit, 15)] += 1
        if lit >= 15: slow_lit += 1
        if i >= sz: break
        off = b[i] | (b[i + 1] << 8); i += 2
        if ml == 15:
            slow_ml += 1
            while True:
                e = b[i]; i += 1; ml += e
                if e != 255: break
        ml += 4; mlsum += ml
        if off < 8: off_lt8 += 1
        if off < 16: off_lt16 += 1
n = nseq
print("seqs %d  avg lit %.1f avg ml %.1f  avg out/seq %.1f  lit>=15 %.1f%%  ml>=19 %.1f%%  off<8 %.1f%% off<16 %.1f%%" % (n, litsum / n, mlsum / n, (litsum + mlsum) / n, 100 * slow_lit / n, 100 * slow_ml / n, 100 * off_lt8 / n, 100 * off_lt16 / n))
print("lit hist", [round(100 * x / n, 1) for x in lithist])
