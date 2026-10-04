// spectral-norm (Benchmarks Game algorithm), single-threaded. Output: %.9f
public class spectralnorm {
    static double a(int i, int j) {
        return 1.0 / ((i + j) * (i + j + 1) / 2 + i + 1);
    }

    static void mulAv(int n, double[] v, double[] av) {
        for (int i = 0; i < n; i++) {
            double s = 0.0;
            for (int j = 0; j < n; j++) s += a(i, j) * v[j];
            av[i] = s;
        }
    }

    static void mulAtv(int n, double[] v, double[] atv) {
        for (int i = 0; i < n; i++) {
            double s = 0.0;
            for (int j = 0; j < n; j++) s += a(j, i) * v[j];
            atv[i] = s;
        }
    }

    static void mulAtAv(int n, double[] v, double[] tmp, double[] atav) {
        mulAv(n, v, tmp);
        mulAtv(n, tmp, atav);
    }

    public static void main(String[] args) {
        int n = Integer.parseInt(args[0]);
        double[] u = new double[n], v = new double[n], tmp = new double[n];
        java.util.Arrays.fill(u, 1.0);
        for (int i = 0; i < 10; i++) {
            mulAtAv(n, u, tmp, v);
            mulAtAv(n, v, tmp, u);
        }
        double vbv = 0, vv = 0;
        for (int i = 0; i < n; i++) {
            vbv += u[i] * v[i];
            vv += v[i] * v[i];
        }
        System.out.println(String.format("%.9f", Math.sqrt(vbv / vv)));
    }
}
