// regex-redux, the Computer Language Benchmarks Game, single-threaded: strip the FASTA headers and newlines
// with a regex, count the matches of nine IUPAC variants, apply five substitutions in turn, print the lengths.
// The file is regex_redux.java because a Java class name has no hyphen; the benchmark is `regex-redux`.
import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.util.regex.Matcher;
import java.util.regex.Pattern;

public class regex_redux {
    static final String[] VARIANTS = {
        "agggtaaa|tttaccct",
        "[cgt]gggtaaa|tttaccc[acg]",
        "a[act]ggtaaa|tttacc[agt]t",
        "ag[act]gtaaa|tttac[agt]ct",
        "agg[act]taaa|ttta[agt]cct",
        "aggg[acg]aaa|ttt[cgt]ccct",
        "agggt[cgt]aa|tt[acg]accct",
        "agggta[cgt]a|t[acg]taccct",
        "agggtaa[cgt]|[acg]ttaccct"
    };
    static final String[][] SUBST = {
        {"tHa[Nt]", "<4>"},
        {"aND|caN|Ha[DS]|WaS", "<3>"},
        {"a[NSt]|BY", "<2>"},
        {"<[^>]*>", "|"},
        {"\\|[^|][^|]*\\|", "-"}
    };

    public static void main(String[] args) throws IOException {
        String s = new String(System.in.readAllBytes(), StandardCharsets.ISO_8859_1);
        int ilen = s.length();
        s = Pattern.compile(">.*\n|\n").matcher(s).replaceAll("");
        int clen = s.length();
        StringBuilder out = new StringBuilder();
        for (String p : VARIANTS) {
            Matcher m = Pattern.compile(p).matcher(s);
            int count = 0;
            while (m.find()) count++;
            out.append(p).append(' ').append(count).append('\n');
        }
        for (String[] sub : SUBST) s = Pattern.compile(sub[0]).matcher(s).replaceAll(sub[1]);
        out.append('\n').append(ilen).append('\n').append(clen).append('\n').append(s.length()).append('\n');
        System.out.print(out);
    }
}
