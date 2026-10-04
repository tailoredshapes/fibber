import java.io.BufferedOutputStream;
import java.io.PrintStream;
import java.math.BigInteger;

/** pidigits: the streaming spigot of the Benchmarks Game with java.math.BigInteger, as its Java entries write it. */
public class pidigits {
    static BigInteger numer = BigInteger.ONE, accum = BigInteger.ZERO, denom = BigInteger.ONE;
    static final BigInteger TWO = BigInteger.valueOf(2), THREE = BigInteger.valueOf(3), FOUR = BigInteger.valueOf(4), TEN = BigInteger.TEN;

    static int extractDigit(BigInteger nth) {
        return nth.multiply(numer).add(accum).divide(denom).intValue();
    }

    static void nextTerm(int k) {
        BigInteger y2 = BigInteger.valueOf(k * 2L + 1);
        accum = accum.add(numer.multiply(TWO)).multiply(y2);
        numer = numer.multiply(BigInteger.valueOf(k));
        denom = denom.multiply(y2);
    }

    static void eliminateDigit(int d) {
        accum = accum.subtract(denom.multiply(BigInteger.valueOf(d))).multiply(TEN);
        numer = numer.multiply(TEN);
    }

    public static void main(String[] args) {
        int n = Integer.parseInt(args[0]);
        PrintStream out = new PrintStream(new BufferedOutputStream(System.out, 1 << 16));
        StringBuilder line = new StringBuilder();
        int i = 0, k = 0;
        while (i < n) {
            k++;
            nextTerm(k);
            if (numer.compareTo(accum) > 0) continue;
            int d = extractDigit(THREE);
            if (d != extractDigit(FOUR)) continue;
            line.append((char) ('0' + d));
            i++;
            if (i % 10 == 0) {
                out.print(line + "\t:" + i + "\n");
                line.setLength(0);
            }
            eliminateDigit(d);
        }
        if (line.length() > 0) {
            while (line.length() < 10) line.append(' ');
            out.print(line + "\t:" + n + "\n");
        }
        out.flush();
    }
}
