import java.io.*;
import java.util.*;

// k-nucleotide: FASTA on stdin as bytes; the sequence after `>THREE`; every k-mer of length 1, 2, 3, 4, 6, 12, 18 counted in a
// java.util.HashMap (key: the k-mer as 2 bits per base in a long, A 0 C 1 G 2 T 3); the frequencies of the 1- and 2-mers and the
// counts of five given k-mers are printed. Same algorithm as k-nucleotide.fib. main ignores its arguments.
// The class is not public: a hyphen is not legal in a Java name, so the file name and the class name differ.
class k_nucleotide {
  static HashMap<Long, Integer> count(byte[] seq, int n, int k) {
    long mask = (1L << (2 * k)) - 1;
    HashMap<Long, Integer> m = new HashMap<>();
    long key = 0;
    for (int i = 0; i < n; i++) {
      key = ((key << 2) | seq[i]) & mask;
      if (i >= k - 1) m.merge(key, 1, Integer::sum);
    }
    return m;
  }

  static String decode(long key, int k) {
    char[] c = new char[k];
    for (int i = 0; i < k; i++) c[k - 1 - i] = "ACGT".charAt((int) ((key >>> (2 * i)) & 3));
    return new String(c);
  }

  static long encode(String s) {
    long key = 0;
    for (int i = 0; i < s.length(); i++) key = key * 4 + "ACGT".indexOf(s.charAt(i));
    return key;
  }

  // 100 * count / total with three decimals, rounded half up, in exact integer arithmetic (as the other twins: %.3f of Java and C
  // disagree on exact ties).
  static String percent3(long count, long total) {
    long v = (count * 200000 + total) / (2 * total), fp = v % 1000;
    return (v / 1000) + "." + (fp < 10 ? "00" : fp < 100 ? "0" : "") + fp;
  }

  static void frequencies(byte[] seq, int n, int k, StringBuilder out) {
    HashMap<Long, Integer> m = count(seq, n, k);
    long total = n - k + 1;
    ArrayList<Long> keys = new ArrayList<>(m.keySet());
    keys.sort((a, b) -> {
      int c = Integer.compare(m.get(b), m.get(a));
      return c != 0 ? c : Long.compare(a, b);
    });
    for (long key : keys)
      out.append(decode(key, k)).append(' ').append(percent3(m.get(key), total)).append('\n');
    out.append('\n');
  }

  static void countOf(byte[] seq, int n, String pat, StringBuilder out) {
    Integer c = count(seq, n, pat.length()).get(encode(pat));
    out.append(c == null ? 0 : c).append('\t').append(pat).append('\n');
  }

  public static void main(String[] args) throws IOException {
    byte[] tbl = new byte[256];
    Arrays.fill(tbl, (byte) -1);
    for (int i = 0; i < 4; i++) { tbl["ACGT".charAt(i)] = (byte) i; tbl["acgt".charAt(i)] = (byte) i; }
    byte[] want = ">THREE".getBytes();
    InputStream in = new FileInputStream(FileDescriptor.in);
    byte[] b = new byte[65536];
    ArrayList<byte[]> chunks = new ArrayList<>();
    int mode = 0, hp = 0, total = 0, len;
    boolean ok = false;
    while (mode < 3 && (len = in.read(b, 0, b.length)) > 0) {
      byte[] tmp = new byte[len];
      int j = 0;
      for (int i = 0; i < len && mode < 3; i++) {
        int c = b[i] & 255;
        if (mode == 1) {
          if (c == '\n') { mode = (ok && hp >= 6) ? 2 : 0; hp = 0; }
          else { if (hp < 6 && c != want[hp]) ok = false; hp++; }
        } else if (c == '>') {
          if (mode == 2) mode = 3; else { mode = 1; hp = 1; ok = true; }
        } else if (mode == 2 && tbl[c] >= 0) tmp[j++] = tbl[c];
      }
      if (j > 0) { chunks.add(Arrays.copyOf(tmp, j)); total += j; }
    }
    byte[] seq = new byte[total];
    int off = 0;
    for (byte[] c : chunks) { System.arraycopy(c, 0, seq, off, c.length); off += c.length; }
    chunks = null;
    StringBuilder out = new StringBuilder();
    frequencies(seq, total, 1, out);
    frequencies(seq, total, 2, out);
    for (String p : new String[] {"GGT", "GGTA", "GGTATT", "GGTATTTTAATT", "GGTATTTTAATTTATAGT"}) countOf(seq, total, p, out);
    System.out.print(out);
    System.out.flush();
  }
}
