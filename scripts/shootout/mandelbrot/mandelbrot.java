import java.io.BufferedOutputStream;
import java.io.IOException;
import java.io.OutputStream;

/** mandelbrot: the Benchmarks Game's, N x N pixels, 50 iterations, P4 bitmap on stdout. */
public class mandelbrot {
    static boolean inside(double cr, double ci) {
        double zr = 0, zi = 0, tr = 0, ti = 0;
        for (int i = 0; i < 50 && tr + ti <= 4.0; i++) {
            zi = 2.0 * zr * zi + ci;
            zr = tr - ti + cr;
            tr = zr * zr;
            ti = zi * zi;
        }
        return tr + ti <= 4.0;
    }

    public static void main(String[] args) throws IOException {
        int n = Integer.parseInt(args[0]);
        int w = (n + 7) / 8;
        byte[] row = new byte[w];
        OutputStream out = new BufferedOutputStream(System.out, 1 << 16);
        out.write(("P4\n" + n + " " + n + "\n").getBytes());
        for (int y = 0; y < n; y++) {
            double ci = 2.0 * (double) y / (double) n - 1.0;
            for (int bx = 0; bx < w; bx++) {
                int acc = 0;
                for (int k = 0; k < 8; k++) {
                    int x = bx * 8 + k;
                    int bit = x < n && inside(2.0 * (double) x / (double) n - 1.5, ci) ? 1 : 0;
                    acc = (acc << 1) | bit;
                }
                row[bx] = (byte) acc;
            }
            out.write(row, 0, w);
        }
        out.flush();
    }
}
