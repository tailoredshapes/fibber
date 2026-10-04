import java.util.Locale;

/** matmul: C = A*B, n x n, loop order i k j; integer-valued entries, so the sum is exact in any order. */
public class matmul {
  public static void main(String[] args) {
    int n = Integer.parseInt(args[0]);
    double[] a = new double[n * n], b = new double[n * n], c = new double[n * n];
    for (int i = 0; i < n; i++)
      for (int j = 0; j < n; j++) { a[i * n + j] = (i + j) % 7; b[i * n + j] = (i + 2 * j) % 5; }
    for (int i = 0; i < n; i++)
      for (int k = 0; k < n; k++) {
        double aik = a[i * n + k];
        for (int j = 0; j < n; j++) c[i * n + j] += aik * b[k * n + j];
      }
    double s = 0.0;
    for (int i = 0; i < n * n; i++) s += c[i];
    System.out.println(String.format(Locale.ROOT, "%.1f", s));
  }
}
