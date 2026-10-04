public class _selftest_stdin {
    public static void main(String[] a) throws Exception {
        byte[] buf = new byte[65536];
        long total = 0;
        for (int n; (n = System.in.read(buf)) > 0; ) total += n;
        System.out.println(total);
        System.out.println("done");
    }
}
