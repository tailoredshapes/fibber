/** fannkuch-redux, single-threaded: the same rotate-and-count algorithm as fannkuch-redux.fib, .c and .clj */
public class fannkuch_redux {
    public static void main(String[] args) {
        int n = args.length > 0 ? Integer.parseInt(args[0]) : 7;
        int[] p = new int[n], p1 = new int[n], cnt = new int[n];
        long checksum = 0, maxflips = 0, pc = 0;
        int r = n;
        for (int i = 0; i < n; i++) p1[i] = i;
        while (true) {
            while (r != 1) { cnt[r - 1] = r; r--; }
            System.arraycopy(p1, 0, p, 0, n);
            long flips = 0;
            int k;
            while ((k = p[0]) != 0) {
                for (int i = 0, j = k; i < j; i++, j--) { int t = p[i]; p[i] = p[j]; p[j] = t; }
                flips++;
            }
            if (flips > maxflips) maxflips = flips;
            checksum += (pc % 2 == 0) ? flips : -flips;
            while (true) {
                if (r == n) {
                    System.out.println(checksum);
                    System.out.println("Pfannkuchen(" + n + ") = " + maxflips);
                    return;
                }
                int p0 = p1[0];
                for (int i = 0; i < r; i++) p1[i] = p1[i + 1];
                p1[r] = p0;
                if (--cnt[r] > 0) break;
                r++;
            }
            pc++;
        }
    }
}
