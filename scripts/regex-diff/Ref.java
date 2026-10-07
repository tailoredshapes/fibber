import java.io.*;
import java.nio.charset.StandardCharsets;
import java.nio.file.*;
import java.util.*;
import java.util.regex.*;

// The Java side of the fib.regex differential test (scripts/regex-diff/run.sh): reads the lines of gen.py (`MODE HEX(pattern) HEX(input)`) and prints, per line,
//   ERR                                  the pattern is refused (PatternSyntaxException)
//   NO                                   no match
//   CPSTART HEX(group0) HEX(group1) ..   the match: the code point index of its start, then the text of each group (`-` for a group that took no part)
// find is `Matcher.find()` from 0; matches is `Matcher.matches()`. The same shape is printed by harness.fib, and the two files must be equal.
public class Ref {
    static String hex(String s) {
        if (s.isEmpty()) return "=";
        StringBuilder b = new StringBuilder();
        for (byte x : s.getBytes(StandardCharsets.UTF_8)) b.append(String.format("%02x", x));
        return b.toString();
    }
    static String unhex(String h) {
        if (h.equals("-")) return "";
        byte[] out = new byte[h.length() / 2];
        for (int i = 0; i < out.length; i++) out[i] = (byte) Integer.parseInt(h.substring(2 * i, 2 * i + 2), 16);
        return new String(out, StandardCharsets.UTF_8);
    }
    public static void main(String[] args) throws Exception {
        PrintStream out = new PrintStream(new BufferedOutputStream(new FileOutputStream(args[1]), 1 << 20), false, "UTF-8");
        Map<String, Pattern> cache = new HashMap<>();
        Set<String> bad = new HashSet<>();
        for (String line : Files.readAllLines(Paths.get(args[0]))) {
            String[] f = line.split(" ");
            String pat = unhex(f[1]);
            String text = unhex(f[2]);
            Pattern p = cache.get(pat);
            if (p == null && !bad.contains(pat)) {
                try { p = Pattern.compile(pat); cache.put(pat, p); } catch (PatternSyntaxException e) { bad.add(pat); } catch (StackOverflowError e) { bad.add(pat); }
            }
            if (p == null) { out.println("ERR"); continue; }
            Matcher m = p.matcher(text);
            boolean ok;
            try { ok = f[0].equals("f") ? m.find() : m.matches(); } catch (StackOverflowError e) { out.println("ERR"); continue; }
            if (!ok) { out.println("NO"); continue; }
            StringBuilder b = new StringBuilder();
            b.append(text.codePointCount(0, m.start()));
            for (int g = 0; g <= m.groupCount(); g++) b.append(' ').append(m.group(g) == null ? "-" : hex(m.group(g)));
            out.println(b);
        }
        out.close();
    }
}
