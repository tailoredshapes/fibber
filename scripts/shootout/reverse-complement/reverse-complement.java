import java.io.*;
import java.util.ArrayList;

// reverse-complement: FASTA on stdin as bytes (64 KB blocks), every sequence reverse-complemented in 60-column lines.
// The residues of a sequence are kept as a list of complemented blocks and written back to front at the next header.
// Same algorithm as reverse-complement.fib. The input is the fasta output; main ignores its arguments.
// The class is not public: a hyphen is not legal in a Java name, so the file name and the class name differ.
class reverse_complement {
  static final byte[] out = new byte[65536];
  static int pos = 0;
  static OutputStream os = new FileOutputStream(FileDescriptor.out);

  static void put(int b) throws IOException {
    if (pos == out.length) { os.write(out, 0, pos); pos = 0; }
    out[pos++] = (byte) b;
  }

  static void emit(ArrayList<byte[]> chunks) throws IOException {
    int col = 0;
    for (int k = chunks.size() - 1; k >= 0; k--) {
      byte[] c = chunks.get(k);
      for (int i = c.length - 1; i >= 0; i--) {
        put(c[i]);
        if (++col == 60) { put('\n'); col = 0; }
      }
    }
    if (col > 0) put('\n');
  }

  public static void main(String[] args) throws IOException {
    byte[] tbl = new byte[256];
    String from = "ACGTUMRWSYKVHDBN", to = "TGCAAKYWSRMBDHVN";
    for (int i = 0; i < 16; i++) { tbl[from.charAt(i)] = (byte) to.charAt(i); tbl[from.charAt(i) + 32] = (byte) to.charAt(i); }
    InputStream in = new FileInputStream(FileDescriptor.in);
    byte[] b = new byte[65536];
    ArrayList<byte[]> chunks = new ArrayList<>();
    int mode = 0;
    int n;
    while ((n = in.read(b, 0, b.length)) > 0) {
      byte[] tmp = new byte[n];
      int j = 0;
      for (int i = 0; i < n; i++) {
        int c = b[i] & 255;
        if (mode == 1) { put(c); if (c == '\n') mode = 0; }
        else if (c == '\n') { }
        else if (c == '>') {
          if (j > 0) { chunks.add(java.util.Arrays.copyOf(tmp, j)); j = 0; }
          emit(chunks); chunks = new ArrayList<>();
          mode = 1; put(c);
        } else tmp[j++] = tbl[c];
      }
      if (j > 0) chunks.add(java.util.Arrays.copyOf(tmp, j));
    }
    emit(chunks);
    os.write(out, 0, pos);
    os.flush();
  }
}
