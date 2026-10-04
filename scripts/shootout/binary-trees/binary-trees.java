// binary-trees, the Computer Language Benchmarks Game: allocate and walk complete binary trees, no pooling.
// usage: java -cp DIR binarytrees N
// The file is binary-trees.java (the suite's layout) and the class is binarytrees: a hyphen is not legal in a Java
// identifier, so the class is not public and javac accepts the mismatch.
class binarytrees {
    static final class Node {
        final Node l, r;
        Node(Node l, Node r) { this.l = l; this.r = r; }
    }

    static Node make(int d) {
        return d == 0 ? new Node(null, null) : new Node(make(d - 1), make(d - 1));
    }

    static int check(Node t) {
        return t.l == null ? 1 : 1 + check(t.l) + check(t.r);
    }

    public static void main(String[] args) {
        int n = args.length > 0 ? Integer.parseInt(args[0]) : 10;
        int minDepth = 4;
        int maxDepth = Math.max(minDepth + 2, n);
        int stretch = maxDepth + 1;
        System.out.println("stretch tree of depth " + stretch + "\t check: " + check(make(stretch)));
        Node longLived = make(maxDepth);
        for (int d = minDepth; d <= maxDepth; d += 2) {
            int iters = 1 << (maxDepth - d + minDepth);
            long sum = 0;
            for (int i = 0; i < iters; i++) sum += check(make(d));
            System.out.println(iters + "\t trees of depth " + d + "\t check: " + sum);
        }
        System.out.println("long lived tree of depth " + maxDepth + "\t check: " + check(longLived));
    }
}
