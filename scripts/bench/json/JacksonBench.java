// Jackson databind: parse to JsonNode and write it back, MB/s, median of 5 batches (after a warm-up of the same size).
// build: javac --release 21 -cp JACKSON_JARS JacksonBench.java ; run: java -Xmx2g -cp .:JACKSON_JARS JacksonBench FILE REPS
import com.fasterxml.jackson.databind.*;
import java.nio.file.*;
import java.util.*;

public class JacksonBench {
  interface Task { void run() throws Exception; }
  static double mbs(long bytes, int reps, Task t) throws Exception {
    for (int i = 0; i < reps; i++) t.run(); // warm-up
    double[] v = new double[5];
    for (int k = 0; k < 5; k++) {
      long t0 = System.nanoTime();
      for (int i = 0; i < reps; i++) t.run();
      v[k] = bytes * (double) reps / ((System.nanoTime() - t0) / 1e9) / 1e6;
    }
    Arrays.sort(v);
    return v[2];
  }
  public static void main(String[] a) throws Exception {
    if (a[0].endsWith(".ndjson")) {
      ObjectMapper m0 = new ObjectMapper();
      long size = 0; long t0 = System.nanoTime();
      try (java.io.BufferedReader r = Files.newBufferedReader(Paths.get(a[0]))) {
        String line; while ((line = r.readLine()) != null) { size += line.length() + 1; m0.readTree(line); }
      }
      System.out.printf("jackson ndjson    %8.1f MB/s%n", size / ((System.nanoTime() - t0) / 1e9) / 1e6);
      return;
    }
    byte[] raw = Files.readAllBytes(Paths.get(a[0]));
    int reps = Integer.parseInt(a[1]);
    ObjectMapper m = new ObjectMapper();
    JsonNode doc = m.readTree(raw);
    byte[] out = m.writeValueAsBytes(doc);
    System.out.printf("jackson parse     %8.1f MB/s%n", mbs(raw.length, reps, () -> m.readTree(raw)));
    System.out.printf("jackson serialise %8.1f MB/s (of output bytes)%n", mbs(out.length, reps, () -> m.writeValueAsBytes(doc)));
  }
}
