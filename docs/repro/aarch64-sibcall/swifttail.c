/* The same shape in C, through clang's swiftasynccall (LLVM swifttailcc, which shares tailcc's
 * callee-pops lowering): a swifttailcc function with stack-passed arguments ends in a plain
 * call of a C function. No `tail` is written: clang emits an unmarked call and LLVM's own
 * passes mark it `tail` at -O1 and up.
 *
 *   clang --target=aarch64-linux-gnu -O2 -c swifttail.c                   (no hardware needed)
 *   clang -O2 swifttail.c -o swifttail && ./swifttail                     (on an arm64 machine)
 *
 * Prints "ok" when the stack pointer survives; crashes or prints "sp moved" otherwise.
 */
#include <stdio.h>
#include <stdint.h>

#define SWIFTASYNC __attribute__((swiftasynccall))
#define CTX __attribute__((swift_async_context))

volatile int64_t sink;

__attribute__((noinline)) void callee(int x) { sink += x; }

/* ten integer arguments: the 9th and 10th (and the async context) go on the stack */
SWIFTASYNC __attribute__((noinline)) void pop(int64_t a, int64_t b, int64_t c, int64_t d, int64_t e, int64_t f,
                                              int64_t g, int64_t h, int64_t i, int64_t j, void *CTX ctx) {
  sink = a + b + c + d + e + f + g + h + i + j;
  callee(7); /* ordinary call in tail position */
}

SWIFTASYNC __attribute__((noinline)) int64_t drive(int64_t n) {
  int64_t acc = 0;
  for (int64_t k = 0; k < n; k++) {
    pop(1, 2, 3, 4, 5, 6, 7, 8, 9, k, 0);
    acc += sink;
  }
  return acc;
}

int main(void) {
  /* a local array whose checksum must survive */
  volatile int64_t guard[8] = {1, 2, 3, 4, 5, 6, 7, 8};
  void *sp0 = __builtin_frame_address(0);
  int64_t r = drive(1000);
  void *sp1 = __builtin_frame_address(0);
  int64_t sum = 0;
  for (int i = 0; i < 8; i++) sum += guard[i];
  printf("drive=%lld guard=%lld frame %s\n", (long long)r, (long long)sum, sp0 == sp1 ? "same" : "moved");
  puts(sum == 36 && r == 1000 * 52 + 499500 + 0 ? "ok" : "BAD");
  return 0;
}
