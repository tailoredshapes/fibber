import java.io.*;
import java.math.*;
import java.nio.file.*;

// The Java side of the fib.decimal differential test: reads gen.py's lines and prints, per line, the text of the result of java.math.BigDecimal for the same
// operation (toString of the result; `true`/`false`, a number, or the bits of a double in hex for the conversions). harness.fib prints the same.
public class Ref {
    static final RoundingMode[] MODES = {RoundingMode.UP, RoundingMode.DOWN, RoundingMode.CEILING, RoundingMode.FLOOR, RoundingMode.HALF_UP, RoundingMode.HALF_DOWN, RoundingMode.HALF_EVEN};
    static double bits(String h) { return Double.longBitsToDouble(Long.parseUnsignedLong(h, 16)); }
    public static void main(String[] args) throws Exception {
        PrintStream out = new PrintStream(new BufferedOutputStream(new FileOutputStream(args[1]), 1 << 20), false, "UTF-8");
        for (String line : Files.readAllLines(Paths.get(args[0]))) {
            String[] f = line.split(" ");
            String r;
            try {
                switch (f[0]) {
                    case "add": r = new BigDecimal(f[1]).add(new BigDecimal(f[2])).toString(); break;
                    case "sub": r = new BigDecimal(f[1]).subtract(new BigDecimal(f[2])).toString(); break;
                    case "mul": r = new BigDecimal(f[1]).multiply(new BigDecimal(f[2])).toString(); break;
                    case "cmp": r = "" + new BigDecimal(f[1]).compareTo(new BigDecimal(f[2])); break;
                    case "eq": r = "" + new BigDecimal(f[1]).equals(new BigDecimal(f[2])); break;
                    case "div": r = new BigDecimal(f[1]).divide(new BigDecimal(f[2]), new MathContext(Integer.parseInt(f[3]), MODES[Integer.parseInt(f[4])])).toString(); break;
                    case "divs": r = new BigDecimal(f[1]).divide(new BigDecimal(f[2]), Integer.parseInt(f[3]), MODES[Integer.parseInt(f[4])]).toString(); break;
                    case "divx": r = new BigDecimal(f[1]).divide(new BigDecimal(f[2])).toString(); break;
                    case "setscale": r = new BigDecimal(f[1]).setScale(Integer.parseInt(f[2]), MODES[Integer.parseInt(f[3])]).toString(); break;
                    case "round": r = new BigDecimal(f[1]).round(new MathContext(Integer.parseInt(f[2]), MODES[Integer.parseInt(f[3])])).toString(); break;
                    case "pow": r = new BigDecimal(f[1]).pow(Integer.parseInt(f[2])).toString(); break;
                    case "strip": r = new BigDecimal(f[1]).stripTrailingZeros().toString(); break;
                    case "plain": r = new BigDecimal(f[1]).toPlainString(); break;
                    case "prec": r = "" + new BigDecimal(f[1]).precision(); break;
                    case "str": r = new BigDecimal(f[1]).toString(); break;
                    case "f64": r = new BigDecimal(bits(f[1])).toString(); break;
                    case "valof": r = BigDecimal.valueOf(bits(f[1])).toString(); break;
                    case "tof64": r = String.format("%016x", Double.doubleToLongBits(new BigDecimal(f[1]).doubleValue())); break;
                    case "toi64": r = "" + new BigDecimal(f[1]).longValue(); break;
                    default: r = "?";
                }
            } catch (ArithmeticException e) { r = "EXC"; }
            out.println(r);
        }
        out.close();
    }
}
