import java.math.BigInteger;

/**
 * The oracle of the BigInt differential cases (cases/stdlib/6000-6099): the same seeded operations as
 * cases/stdlib/support/tl/bigdiff.fib, computed with java.math.BigInteger. For each kind it prints
 * `kind digest` (the digest of every result text the kind produced); with the argument `trace` and a kind
 * it prints the result texts themselves, one per line, to diff against the fibber trace.
 *   javac -d DIR scripts/bigint/BigDiff.java
 *   java -cp DIR BigDiff            # the digests, which the case embeds
 *   java -cp DIR BigDiff trace K    # the lines of kind K
 * The generator is tl.rng's xorshift64; its `below` is (next >>> 1) % n.
 */
public class BigDiff {
    static final int KINDS = 14;
    static final int[] ITERS = {600, 600, 500, 500, 400, 400, 400, 300, 400, 400, 400, 300, 300, 60};

    static long state;
    static long next() {
        long x = state;
        x ^= x << 13;
        x ^= x >>> 7;
        x ^= x << 17;
        state = x;
        return x;
    }
    static long below(long n) { return (next() >>> 1) % n; }
    static void seed(long s) { state = s == 0 ? 88172645463325252L : s; }

    static final BigInteger ONE = BigInteger.ONE, ZERO = BigInteger.ZERO;
    static BigInteger two(int k) { return ONE.shiftLeft(k); }

    static BigInteger randBits(int limbs) {
        BigInteger v = ZERO;
        for (int i = 0; i < limbs; i++) v = v.shiftLeft(31).add(BigInteger.valueOf(below(2147483648L)));
        return v;
    }
    static final long[] EDGE_LIMBS = {0, 1, 2, 1073741823L, 1073741824L, 1073741825L, 2147483646L, 2147483647L};
    static BigInteger edgeBits(int limbs) {
        BigInteger v = ZERO;
        for (int i = 0; i < limbs; i++) v = v.shiftLeft(31).add(BigInteger.valueOf(EDGE_LIMBS[(int) below(8)]));
        return v;
    }
    static BigInteger signed(BigInteger v) { return below(2) == 1 ? v.negate() : v; }

    static BigInteger genBig() {
        long kind = below(11);
        if (kind == 0) return ZERO;
        if (kind == 1) return BigInteger.valueOf(below(19) - 9);
        if (kind == 2) {
            int k = (int) below(260);
            long d = below(3) - 1;
            return signed(two(k).add(BigInteger.valueOf(d)));
        }
        if (kind == 3) return signed(randBits(1 + (int) below(6)));
        if (kind == 4) return signed(randBits(1 + (int) below(40)));
        if (kind == 5) {
            int n = 1 + (int) below(8);
            int w = below(2) == 0 ? 31 : 32;
            return signed(two(w * n).subtract(ONE));
        }
        if (kind == 6) return signed(randBits(40 + (int) below(120)));
        if (kind == 7) {
            BigInteger[] e = {BigInteger.valueOf(Long.MIN_VALUE), BigInteger.valueOf(Long.MAX_VALUE),
                BigInteger.valueOf(Long.MIN_VALUE + 1), BigInteger.valueOf(Long.MAX_VALUE - 1), two(63), two(63).negate().subtract(ONE),
                two(62), two(32), two(31), two(31).subtract(ONE)};
            return e[(int) below(10)];
        }
        if (kind == 8) {
            int k = (int) below(80);
            return signed(BigInteger.TEN.pow(k).add(BigInteger.valueOf(below(3) - 1)));
        }
        if (kind == 10) return signed(edgeBits(1 + (int) below(8)));
        return BigInteger.valueOf(below(2147483648L)).multiply(BigInteger.valueOf(below(2147483648L))).multiply(BigInteger.valueOf(below(3) - 1));
    }

    static BigInteger nonzero(BigInteger b) { return b.signum() == 0 ? ONE : b; }
    static String cmp(BigInteger a, BigInteger b) { return Integer.toString(a.compareTo(b)); }

    static boolean trace;
    static long digest;
    static void out(String s) {
        if (trace) System.out.println(s);
        for (int i = 0; i < s.length(); i++) {
            digest ^= (s.charAt(i) & 255);
            step();
        }
        digest ^= 256;
        step();
    }
    static void step() {
        digest ^= digest << 13;
        digest ^= digest >>> 7;
        digest ^= digest << 17;
    }

    static BigInteger jmod(BigInteger a, BigInteger b) {
        BigInteger r = a.remainder(b);
        return (r.signum() != 0 && r.signum() != b.signum()) ? r.add(b) : r;
    }

    static String ratio(BigInteger a, BigInteger b) {
        BigInteger g = a.gcd(b);
        if (b.signum() < 0) g = g.negate();
        BigInteger n = a.divide(g), d = b.divide(g);
        return d.equals(ONE) ? n.toString() : n + "/" + d;
    }

    static void one(int kind) {
        BigInteger a = genBig(), b = nonzero(genBig());
        switch (kind) {
            case 0: out(a.add(b).toString()); break;
            case 1: out(a.subtract(b).toString()); break;
            case 2: out(a.multiply(b).toString()); break;
            case 3: out(a.divide(b).toString()); out(a.remainder(b).toString()); break;
            case 4: out(cmp(a, b)); out(Boolean.toString(a.equals(b))); out(Boolean.toString(a.compareTo(b) < 0)); break;
            case 5: {
                int k = (int) below(200);
                out(a.shiftLeft(k).toString());
                out(a.shiftRight(k).toString());
                break;
            }
            case 6: out(a.gcd(b).toString()); break;
            case 7: {
                int e = (int) below(6);
                out(a.pow(e).toString());
                break;
            }
            case 8: out(a.toString()); out(a.negate().toString()); out(a.abs().toString()); break;
            case 9: {
                out(a.bitLength() <= 63 ? Long.toString(a.longValue()) : "nofit");
                out(a.abs().bitLength() <= 1000 ? Double.toString(a.doubleValue()) : "big");
                out(Integer.toString(a.abs().bitLength()));
                break;
            }
            case 10: {
                long m = below(2147483648L);
                out(a.multiply(BigInteger.valueOf(m)).toString());
                out(jmod(a, b).toString());
                break;
            }
            case 11: out(a.abs().sqrt().toString()); break;
            case 12: out(ratio(a, b)); break;
            default: {
                BigInteger x = below(2) == 1 ? edgeBits(40 + (int) below(100)) : randBits(40 + (int) below(160));
                BigInteger y = below(2) == 1 ? edgeBits(1 + (int) below(100)) : randBits(1 + (int) below(200));
                if (below(2) == 1) y = randBits(40 + (int) below(60));
                y = nonzero(y);
                out(x.multiply(y).toString());
                out(x.divide(y).toString());
                out(x.remainder(y).toString());
                out(x.multiply(x).toString());
            }
        }
    }

    public static void main(String[] args) {
        trace = args.length > 0 && args[0].equals("trace");
        for (int kind = 0; kind < KINDS; kind++) {
            if (trace && kind != Integer.parseInt(args[1])) continue;
            seed(1000 + kind);
            digest = 1;
            for (int i = 0; i < ITERS[kind]; i++) one(kind);
            if (!trace) System.out.println(kind + " " + digest);
        }
    }
}
