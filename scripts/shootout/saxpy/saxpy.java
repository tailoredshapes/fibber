import java.util.Locale;

/** saxpy: y = 0.5*x + y over arrays of N doubles, REPS times (default 1); then the sum of y. Exact in any order. */
public class saxpy {
  public static void main(String[] args) {
    int n = Integer.parseInt(args[0]);
    long reps = args.length > 1 ? Long.parseLong(args[1]) : 1;
    double[] x = new double[n], y = new double[n];
    for (int i = 0; i < n; i++) { x[i] = i % 7; y[i] = i % 5; }
    for (long r = 0; r < reps; r++)
      for (int i = 0; i < n; i++) y[i] = 0.5 * x[i] + y[i];
    double s = 0.0;
    for (int i = 0; i < n; i++) s += y[i];
    System.out.println(String.format(Locale.ROOT, "%.1f", s));
  }
}
