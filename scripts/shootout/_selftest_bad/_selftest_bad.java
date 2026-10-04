public class _selftest_bad {
    public static void main(String[] a) {
        long n = Long.parseLong(a[0]), acc = 0;
        for (long i = 0; i < n; i++) acc += (i * i) % 1000003;
        System.out.println(acc + 1);
        System.out.println("done");
    }
}
