/* compiler/tests/shared/solid-host.c: calls render(text) of libsolid (solid-lib.fib) through the GENERATED header, with the retry rule of the str ABI:
 * a first buffer too small for the JSON gets a second call with the length the first returned. Prints the JSON and a newline, as `fibc run` of render-main.fib does.
 * Exit 2 if the library is wrong about its own length (the second call must fit). */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdint.h>
#include "solid.h"

int main(int argc, char **argv) {
  if (argc != 2) { fprintf(stderr, "usage: solid-host MODEL\n"); return 1; }
  int64_t cap = 64; uint8_t *buf = malloc(cap); int calls = 0; int64_t need;
  for (;;) {
    need = render((const uint8_t *)argv[1], (int64_t)strlen(argv[1]), buf, cap); calls++;
    if (need <= cap) break;
    cap = need; buf = realloc(buf, cap);
    if (calls > 2) { fprintf(stderr, "the second call did not fit\n"); return 2; }
  }
  fwrite(buf, 1, (size_t)need, stdout); fputc('\n', stdout);
  fprintf(stderr, "# render: %lld bytes in %d call(s)\n", (long long)need, calls);
  return 0;
}
