import java.io.IOException;
import java.io.OutputStream;

/** fasta (Computer Language Benchmarks Game): repeat sequence, then two weighted random sequences; 60 columns. */
public class fasta {
    static final int IM = 139968, IA = 3877, IC = 29573;
    static final int CAP = 65536;
    static final byte[] ALU = ("GGCCGGGCGCGGTGGCTCACGCCTGTAATCCCAGCACTTTGG"
        + "GAGGCCGAGGCGGGCGGATCACCTGAGGTCAGGAGTTCGAGA"
        + "CCAGCCTGGCCAACATGGTGAAACCCCGTCTCTACTAAAAAT"
        + "ACAAAAATTAGCCGGGCGTGGTGGCGCGCGCCTGTAATCCCA"
        + "GCTACTCGGGAGGCTGAGGCAGGAGAATCGCTTGAACCCGGG"
        + "AGGCGGAGGTTGCAGTGAGCCGAGATCGCGCCACTGCACTCC"
        + "AGCCTGGGCGACAGAGCGAGACTCCGTCTCAAAAA").getBytes();

    static final byte[] buf = new byte[CAP];
    static int pos = 0;
    static OutputStream out = System.out;

    static void flush() throws IOException {
        out.write(buf, 0, pos);
        pos = 0;
    }

    static void putStr(String s) throws IOException {
        for (int i = 0; i < s.length(); i++) buf[pos++] = (byte) s.charAt(i);
    }

    static void repeat(int n) throws IOException {
        int m = ALU.length, k = 0;
        for (int left = n; left > 0; ) {
            int w = Math.min(left, 60);
            for (int j = 0; j < w; j++) buf[pos + j] = ALU[(k + j) % m];
            buf[pos + w] = '\n';
            pos += w + 1;
            if (pos > CAP - 61) flush();
            left -= w;
            k = (k + w) % m;
        }
    }

    static double[] cumulative(double[] p) {
        double[] c = new double[p.length];
        double acc = 0.0;
        for (int i = 0; i < p.length; i++) { acc += p[i]; c[i] = acc; }
        return c;
    }

    static int random(double[] cum, byte[] syms, int n, int seed) throws IOException {
        int last = seed, m = cum.length - 1;
        for (int left = n; left > 0; ) {
            int w = Math.min(left, 60);
            for (int j = 0; j < w; j++) {
                last = (last * IA + IC) % IM;
                double r = (double) last / 139968.0;
                int i = 0;
                while (i < m && r >= cum[i]) i++;
                buf[pos + j] = syms[i];
            }
            buf[pos + w] = '\n';
            pos += w + 1;
            if (pos > CAP - 61) flush();
            left -= w;
        }
        return last;
    }

    public static void main(String[] args) throws IOException {
        int n = args.length > 0 ? Integer.parseInt(args[0]) : 1000;
        byte[] iubS = "acgtBDHKMNRSVWY".getBytes();
        double[] iubC = cumulative(new double[] {0.27, 0.12, 0.12, 0.27, 0.02, 0.02, 0.02, 0.02, 0.02, 0.02, 0.02, 0.02, 0.02, 0.02, 0.02});
        byte[] hsS = "acgt".getBytes();
        double[] hsC = cumulative(new double[] {0.3029549426680, 0.1979883004921, 0.1975473066391, 0.3015094502008});
        putStr(">ONE Homo sapiens alu\n");
        repeat(n * 2);
        putStr(">TWO IUB ambiguity codes\n");
        int s = random(iubC, iubS, n * 3, 42);
        putStr(">THREE Homo sapiens frequency\n");
        random(hsC, hsS, n * 5, s);
        flush();
        out.flush();
    }
}
