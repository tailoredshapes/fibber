import java.util.Locale;

/** dot-product: sum of x[i]*y[i] over arrays of N doubles, repeated REPS times (default 1). Exact in any summation order. */
public class dotproduct {
  public static void main(String[] args) {
    int n = Integer.parseInt(args[0]);
    long reps = args.length > 1 ? Long.parseLong(args[1]) : 1;
    double[] x = new double[n], y = new double[n];
    for (int i = 0; i < n; i++) { x[i] = i % 7; y[i] = i % 3 + 0.5; }
    double total = 0.0;
    for (long r = 0; r < reps; r++) {
      double s = 0.0;
      for (int i = 0; i < n; i++) s += x[i] * y[i];
      total += s;
    }
    System.out.println(String.format(Locale.ROOT, "%.1f", total));
  }
}
