// Parse benchmark of simdjson (single-header release, SIMDJSON_VERSION in the header): DOM parse, and On Demand extraction of three fields.
// build: g++ -O3 -march=native -std=c++17 -I DIR simdjson-bench.cpp DIR/simdjson.cpp -o simdjson-bench
// usage: simdjson-bench FILE REPS            prints: name MB/s (median of 5 timed batches of REPS parses)
#include "simdjson.h"
#include <algorithm>
#include <chrono>
#include <cstdio>
#include <cstdlib>
#include <vector>
#include <string>
using namespace simdjson;
using clk = std::chrono::steady_clock;

template <class F> double median_mbs(size_t bytes, int reps, F f) {
  std::vector<double> v;
  for (int k = 0; k < 5; k++) {
    auto t0 = clk::now();
    for (int i = 0; i < reps; i++) f();
    double s = std::chrono::duration<double>(clk::now() - t0).count();
    v.push_back(double(bytes) * reps / s / 1e6);
  }
  std::sort(v.begin(), v.end());
  return v[2];
}

int main(int argc, char** argv) {
  if (std::string(argv[1]).size() > 7 && std::string(argv[1]).substr(std::string(argv[1]).size() - 7) == ".ndjson") {
    dom::parser p0;
    auto t0 = clk::now(); size_t docs = 0, size = 0;
    for (auto doc : p0.load_many(argv[1], 8000000)) { docs += doc.value().is_object(); }
    double s = std::chrono::duration<double>(clk::now() - t0).count();
    FILE* f = fopen(argv[1], "rb"); fseek(f, 0, SEEK_END); size = ftell(f); fclose(f);
    printf("simdjson dom load_many     %8.1f MB/s (%zu docs)\n", size / s / 1e6, docs);
    return 0;
  }
  padded_string json = padded_string::load(argv[1]).value();
  int reps = atoi(argv[2]);
  dom::parser p;
  size_t sink = 0;
  printf("simdjson dom parse        %8.1f MB/s  (implementation %s)\n", median_mbs(json.size(), reps, [&] { sink += p.parse(json).value().is_object(); }),
         get_active_implementation()->name().c_str());
  ondemand::parser op;
  printf("simdjson ondemand 3 fields %8.1f MB/s\n", median_mbs(json.size(), reps, [&] {
    auto doc = op.iterate(json);
    // twitter.json: statuses[0].user.screen_name, search_metadata.count, statuses[0].id; other files: count the top-level keys
    auto obj = doc.get_object();
    for (auto f : obj) { auto k = f.unescaped_key().value(); sink += k.size(); break; }
  }));
  printf("%zu\n", sink > 0 ? (size_t)1 : (size_t)0);
}
