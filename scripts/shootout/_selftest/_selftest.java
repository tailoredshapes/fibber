public class _selftest {
    public static void main(String[] a) {
        long n = Long.parseLong(a[0]), acc = 0;
        for (long i = 0; i < n; i++) acc += (i * i) % 1000003;
        System.out.println(acc);
        System.out.println("done");
    }
}
