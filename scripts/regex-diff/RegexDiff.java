// The oracle of the differential test of fib.regex (cases/stdlib/6000-ref-regex-vs-java-util-regex.fib).
// It draws seeded random patterns from the subset fib.regex supports, builds a text for each that is
// biased toward matching (samples of the pattern's own language, mixed with noise), and records what
// java.util.regex does with the pair, with every offset converted to UTF-8 byte offsets as the fibber
// library reports them. One record per line, fields separated by U+00A6:
//   pattern, text, mode (G, or L when the groups are not compared, see `nested`), F (every find()), M (matches()),
//   R (replaceAll), S (split with limit 0)
// In the text field and in R, LF is written `~` and CR `^`, and the fibber side does the same, so a record
// is one line.
//   java RegexDiff.java gen SEED COUNT        the records on stdout
//   java RegexDiff.java case SEED COUNT TEMPLATE   TEMPLATE with the line `@@DATA@@` replaced by the records
//                                                  as a fibber string literal
import java.io.*;
import java.nio.charset.StandardCharsets;
import java.nio.file.*;
import java.util.*;
import java.util.regex.*;

public class RegexDiff {
    static final String LIT = "abcx01 _-.é€あ\n";
    static final String TEXTCH = "abcx01 _-.é€あ\n\r";
    static Random rnd;

    interface Node { void render(StringBuilder sb); String sample(); }

    static int pick(int n) { return rnd.nextInt(n); }
    static boolean chance(int pct) { return rnd.nextInt(100) < pct; }

    static void lit(StringBuilder sb, char c) {
        if (Character.isLetterOrDigit(c)) sb.append(c);
        else if (c == '\n') sb.append("\\n");
        else sb.append('\\').append(c);
    }

    record Lit(char c) implements Node {
        public void render(StringBuilder sb) { lit(sb, c); }
        public String sample() { return String.valueOf(c); }
    }
    record Cls(String how, String chars, boolean neg) implements Node {
        // how: "set" (chars), "d", "w", "s", "D", "W", "S", "dot"
        public void render(StringBuilder sb) {
            switch (how) {
                case "set" -> {
                    sb.append(neg ? "[^" : "[");
                    for (int i = 0; i < chars.length(); i++) {
                        char c = chars.charAt(i);
                        if (i + 2 < chars.length() && chars.charAt(i + 1) == '~' && Character.isLetterOrDigit(c)
                            && Character.isLetterOrDigit(chars.charAt(i + 2))) {
                            sb.append(c).append('-').append(chars.charAt(i + 2)); i += 2;
                        } else if (c == '~') { /* unreachable */ }
                        else lit(sb, c);
                    }
                    sb.append(']');
                }
                case "dot" -> sb.append('.');
                default -> sb.append('\\').append(how);
            }
        }
        boolean has(char c) {
            return switch (how) {
                case "set" -> { String e = expand(); yield (e.indexOf(c) >= 0) != neg; }
                case "dot" -> c != '\n' && c != '\r';
                case "d" -> c >= '0' && c <= '9';
                case "D" -> !(c >= '0' && c <= '9');
                case "w" -> Character.isLetterOrDigit(c) && c < 128 || c == '_';
                case "W" -> !(Character.isLetterOrDigit(c) && c < 128 || c == '_');
                case "s" -> " \t\n\r\f\u000b".indexOf(c) >= 0;
                default -> " \t\n\r\f\u000b".indexOf(c) < 0;
            };
        }
        String expand() {
            StringBuilder b = new StringBuilder();
            for (int i = 0; i < chars.length(); i++) {
                char c = chars.charAt(i);
                if (i + 2 < chars.length() && chars.charAt(i + 1) == '~') {
                    for (char x = c; x <= chars.charAt(i + 2); x++) b.append(x);
                    i += 2;
                } else b.append(c);
            }
            return b.toString();
        }
        public String sample() {
            for (int k = 0; k < 40; k++) { char c = TEXTCH.charAt(pick(TEXTCH.length())); if (has(c)) return String.valueOf(c); }
            return "a";
        }
    }
    record Anchor(String s) implements Node {
        public void render(StringBuilder sb) { sb.append(s); }
        public String sample() { return ""; }
    }
    record Cat(List<Node> items) implements Node {
        public void render(StringBuilder sb) { for (Node n : items) n.render(sb); }
        public String sample() { StringBuilder b = new StringBuilder(); for (Node n : items) b.append(n.sample()); return b.toString(); }
    }
    record Alt(List<Node> arms) implements Node {
        public void render(StringBuilder sb) { for (int i = 0; i < arms.size(); i++) { if (i > 0) sb.append('|'); arms.get(i).render(sb); } }
        public String sample() { return arms.get(pick(arms.size())).sample(); }
    }
    record Group(Node body, boolean capture) implements Node {
        public void render(StringBuilder sb) { sb.append(capture ? "(" : "(?:"); body.render(sb); sb.append(')'); }
        public String sample() { return body.sample(); }
    }
    record Rep(Node body, int min, int max, boolean lazy) implements Node {
        public void render(StringBuilder sb) {
            body.render(sb);
            if (min == 0 && max == -1) sb.append('*');
            else if (min == 1 && max == -1) sb.append('+');
            else if (min == 0 && max == 1) sb.append('?');
            else if (max == min) sb.append('{').append(min).append('}');
            else if (max == -1) sb.append('{').append(min).append(",}");
            else sb.append('{').append(min).append(',').append(max).append('}');
            if (lazy) sb.append('?');
        }
        public String sample() {
            int hi = max == -1 ? min + 2 : max;
            int n = min + pick(hi - min + 1);
            StringBuilder b = new StringBuilder();
            for (int i = 0; i < n; i++) b.append(body.sample());
            return b.toString();
        }
    }

    static Node atom(int depth) {
        int k = pick(100);
        if (k < 40) return new Lit(LIT.charAt(pick(LIT.length())));
        if (k < 55) return new Cls("dot", "", false);
        if (k < 70) {
            StringBuilder cs = new StringBuilder();
            int n = 1 + pick(3);
            for (int i = 0; i < n; i++) {
                if (chance(30)) { String r = chance(40) ? "a~c" : (chance(40) ? "0~1" : (chance(50) ? "b~x" : (chance(50) ? "à~ÿ" : "ぁ~ん"))); cs.append(r); }
                else cs.append(LIT.charAt(pick(LIT.length())));
            }
            return new Cls("set", cs.toString(), chance(30));
        }
        if (k < 80) return new Cls(new String[]{"d", "w", "s", "D", "W", "S"}[pick(6)], "", false);
        if (depth > 0) return new Group(alt(depth - 1), chance(70));
        return new Lit('a');
    }

    static Node piece(int depth) {
        Node a = atom(depth);
        int k = pick(100);
        if (k < 55) return a;
        boolean lazy = chance(25);
        if (k < 68) return new Rep(a, 0, -1, lazy);
        if (k < 78) return new Rep(a, 1, -1, lazy);
        if (k < 86) return new Rep(a, 0, 1, lazy);
        int mn = pick(3);
        int sel = pick(3);
        if (sel == 0) return new Rep(a, mn, mn, lazy);
        if (sel == 1) return new Rep(a, mn, -1, lazy);
        return new Rep(a, mn, mn + pick(3), lazy);
    }

    static Node cat(int depth) {
        List<Node> items = new ArrayList<>();
        int n = 1 + pick(4);
        if (chance(10)) items.add(new Anchor(new String[]{"^", "\\b", "\\A", "^"}[pick(4)]));
        for (int i = 0; i < n; i++) items.add(piece(depth));
        if (chance(12)) items.add(new Anchor(new String[]{"$", "$", "\\B", "\\b", "\\z", "\\Z"}[pick(6)]));
        return new Cat(items);
    }

    static Node alt(int depth) {
        if (chance(70)) return cat(depth);
        List<Node> arms = new ArrayList<>();
        int n = 2 + pick(2);
        for (int i = 0; i < n; i++) arms.add(cat(depth));
        return new Alt(arms);
    }

    static String text(Node n) {
        StringBuilder b = new StringBuilder();
        int parts = pick(5);
        for (int i = 0; i < parts; i++) {
            if (chance(55)) b.append(n.sample());
            int noise = pick(4);
            for (int j = 0; j < noise; j++) b.append(TEXTCH.charAt(pick(TEXTCH.length())));
        }
        return b.toString();
    }

    // Java keeps the capture of an earlier iteration of an outer repeat for a group inside two nested repeats
    // (`((a)*b)*` on "abaab": group 2 is the first iteration's); fib.regex gives the last one, as Perl does.
    // Such patterns are compared on the whole match only (mode L).
    static boolean nested(Node n, int depth) {
        if (n instanceof Group g) return (g.capture && depth >= 2) || nested(g.body, depth);
        if (n instanceof Rep r) return r.max == 0 || nested(r.body, depth + ((r.max == -1 || r.max >= 2) ? 1 : 0));
        if (n instanceof Cat c) { for (Node x : c.items) if (nested(x, depth)) return true; return false; }
        if (n instanceof Alt a) { for (Node x : a.arms) if (nested(x, depth)) return true; return false; }
        return false;
    }

    // Java's matcher backtracks, so some patterns take exponential time on some texts; such a pair is dropped.
    static final class Timed implements CharSequence {
        final String s; int calls;
        final long deadline = System.nanoTime() + 150_000_000L;
        Timed(String s) { this.s = s; }
        public int length() { return s.length(); }
        public char charAt(int i) {
            if ((++calls & 1023) == 0 && System.nanoTime() > deadline) throw new IllegalStateException("slow");
            return s.charAt(i);
        }
        public CharSequence subSequence(int a, int b) { return s.subSequence(a, b); }
        public String toString() { return s; }
    }

    static int bytes(String s, int idx) { return s.substring(0, idx).getBytes(StandardCharsets.UTF_8).length; }
    static String enc(String s) { return s.replace('\n', '~').replace('\r', '^'); }

    static String span(String t, Matcher m, int g) {
        return m.start(g) < 0 ? "n" : bytes(t, m.start(g)) + "-" + bytes(t, m.end(g));
    }
    static String groups(String t, Matcher m, boolean loose) {
        StringBuilder b = new StringBuilder(span(t, m, 0));
        for (int g = 1; g <= m.groupCount() && !loose; g++) b.append(',').append(span(t, m, g));
        return b.toString();
    }

    static String record(String pat, String t, boolean loose) {
        Pattern p = Pattern.compile(pat);
        StringBuilder f = new StringBuilder("F");
        Matcher m = p.matcher(new Timed(t));
        int guard = 0;
        while (m.find() && guard++ < 200) f.append(groups(t, m, loose)).append(';');
        Matcher mm = p.matcher(new Timed(t));
        String mstr = mm.matches() ? "M" + groups(t, mm, loose) : "Mn";
        String tmpl = p.matcher("").groupCount() >= 1 && !loose ? "<$1:$0>" : "<$0>";
        String r = "R" + enc(p.matcher(new Timed(t)).replaceAll(tmpl));
        String[] parts = p.split(new Timed(t), 0);
        String s = "S" + enc(String.join("/", parts));
        return pat + "¦" + enc(t) + "¦" + (loose ? "L" : "G") + "¦" + f + "¦" + mstr + "¦" + r + "¦" + s;
    }

    static List<String> records(long seed, int count) {
        rnd = new Random(seed);
        List<String> out = new ArrayList<>();
        while (out.size() < count) {
            Node n = alt(2);
            StringBuilder sb = new StringBuilder();
            n.render(sb);
            String pat = sb.toString();
            try {
                Pattern.compile(pat);
            } catch (PatternSyntaxException e) { continue; }
            String t = text(n);
            try {
                out.add(record(pat, t, nested(n, 0)));
            } catch (IllegalStateException e) { /* too slow in Java: the pair is dropped */ }
        }
        return out;
    }

    public static void main(String[] a) throws Exception {
        if (a[0].equals("rec")) {
            // java RegexDiff.java rec PATTERN TEXT: the record of one pair (a backslash-n in TEXT is an LF)
            PrintStream p = new PrintStream(System.out, false, StandardCharsets.UTF_8);
            p.println(record(a[1], a[2].replace("\\n", "\n"), false));
            p.flush();
            return;
        }
        long seed = Long.parseLong(a[1]);
        int count = Integer.parseInt(a[2]);
        List<String> recs = records(seed, count);
        PrintStream o = new PrintStream(System.out, false, StandardCharsets.UTF_8);
        if (a[0].equals("gen")) {
            for (String r : recs) o.println(r);
        } else {
            String tpl = Files.readString(Path.of(a[3]));
            StringBuilder lit = new StringBuilder("\"");
            for (String r : recs) lit.append(r.replace("\\", "\\\\").replace("\"", "\\\"")).append("\\n");
            lit.append("\"");
            o.print(tpl.replace("@@DATA@@", lit.toString()).replace("@@COUNT@@", String.valueOf(count)).replace("@@SEED@@", String.valueOf(seed)));
        }
        o.flush();
    }
}
