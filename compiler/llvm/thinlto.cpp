// compiler/llvm/thinlto.cpp: the in-process ThinLTO of the partitioned build (FIB_LIRO_THIN=1, docs/design/compile-performance.md).
//
// LLVM-C has no writer for a module summary and no ThinLTO link; libLTO's `thinlto_*` C API is a separate shared library (tools/lto,
// libLTO.so / libLTO.dylib, no static archive) built over the shared libLLVM. What both need is in the C++ API of the LLVM fibc already
// links: buildModuleSummaryIndex and WriteBitcodeToFile (Analysis, BitWriter), the thin link's analyses (FunctionImport, LTO) and
// lto::thinBackend (libLLVMLTO). This file is the small piece of C++ over them, compiled by the build into one object that the link of
// fibc takes whole (scripts/thinlto-shim.sh); there is no linker plugin and no tool outside fibc, on ELF or Mach-O.
//
// fibc does not name a symbol of this file, so a fibc linked without it still links (FIB_LIRO_THIN=1 then says so). Its functions are
// static; a constructor registers them with LLVM's own process symbol table (sys::DynamicLibrary::AddSymbol), which llvm.thinlto reads
// back through LLVM-C's LLVMSearchForAddressOfSymbol. Each has the shape of a `bsearch` comparison, int f(const void *, const void *),
// so that fibber, which cannot call an address, calls it through libc's bsearch over one element (as native.call reaches its shim): the
// first argument is a block of 64-bit words, the request and its results.
#include "llvm/ADT/StringExtras.h"
#include "llvm/Analysis/ModuleSummaryAnalysis.h"
#include "llvm/Analysis/ProfileSummaryInfo.h"
#include "llvm/Bitcode/BitcodeReader.h"
#include "llvm/Bitcode/BitcodeWriter.h"
#include "llvm/IR/LLVMContext.h"
#include "llvm/IR/Module.h"
#include "llvm/IR/ModuleSummaryIndex.h"
#include "llvm/LTO/Config.h"
#include "llvm/LTO/LTO.h"
#include "llvm/LTO/LTOBackend.h"
#include "llvm/Support/CachePruning.h"
#include "llvm/Support/Caching.h"
#include "llvm/Support/CommandLine.h"
#include "llvm/Support/DynamicLibrary.h"
#include "llvm/Support/FileSystem.h"
#include "llvm/Support/MemoryBuffer.h"
#include "llvm/Support/Path.h"
#include "llvm/Support/Process.h"
#include "llvm/Support/SHA1.h"
#include "llvm/Support/ThreadPool.h"
#include "llvm/Support/raw_ostream.h"
#include "llvm/Support/raw_sha1_ostream.h"
#include "llvm/Target/TargetMachine.h"
#include "llvm/Transforms/IPO/FunctionAttrs.h"
#include "llvm/Transforms/IPO/FunctionImport.h"

#include <atomic>
#include <cstdint>
#include <cstdlib>
#include <cstring>
#include <map>
#include <memory>
#include <mutex>
#include <set>
#include <string>
#include <vector>

using namespace llvm;

namespace {

char *copyString(const std::string &S) {
  char *P = static_cast<char *>(std::malloc(S.size() + 1));
  std::memcpy(P, S.data(), S.size());
  P[S.size()] = 0;
  return P;
}

int64_t asWord(const void *P) { return static_cast<int64_t>(reinterpret_cast<intptr_t>(P)); }
template <typename T> T fromWord(int64_t W) { return reinterpret_cast<T>(static_cast<intptr_t>(W)); }

// Request words of `summary`: [0] the LLVMModuleRef, after the per-unit half of the pipeline (thinlto-pre-link<On>). Results: [1] the
// bitcode (malloc'd), [2] its length, [3] an error message (malloc'd) or 0. The bitcode carries the module summary and the module hash,
// as ThinLTOBitcodeWriterPass writes a module without type metadata, and the symbol table; on any object format.
int summary(const void *Req, const void *) {
  int64_t *W = const_cast<int64_t *>(static_cast<const int64_t *>(Req));
  Module *M = fromWord<Module *>(W[0]);
  W[1] = W[2] = W[3] = 0;
  ProfileSummaryInfo PSI(*M);
  ModuleSummaryIndex Index = buildModuleSummaryIndex(*M, nullptr, &PSI);
  SmallVector<char, 0> Buffer;
  raw_svector_ostream OS(Buffer);
  ModuleHash Hash = {{0}};
  WriteBitcodeToFile(*M, OS, /*ShouldPreserveUseListOrder=*/false, &Index, /*GenerateHash=*/true, &Hash);
  char *Out = static_cast<char *>(std::malloc(Buffer.size() ? Buffer.size() : 1));
  std::memcpy(Out, Buffer.data(), Buffer.size());
  W[1] = asWord(Out);
  W[2] = static_cast<int64_t>(Buffer.size());
  return 0;
}

// The import threshold (LLVM's `import-instr-limit`) is a process-wide option; the occurrences are reset first so that a second link in
// one process may set it again.
void setImportLimit(int64_t Limit) {
  if (Limit < 0)
    return;
  std::string Arg = "-import-instr-limit=" + std::to_string(Limit);
  const char *Argv[] = {"fibc", Arg.c_str()};
  cl::ResetAllOptionOccurrences();
  cl::ParseCommandLineOptions(2, Argv, "", &errs());
}

// The units of one link: their bitcode, identifiers (the digest of the bitcode: a cached unit is copied to a new file name in each build,
// and the GUIDs of local symbols depend on the identifier) and the combined summary index of the thin link.
struct Units {
  std::vector<std::unique_ptr<MemoryBuffer>> Buffers;
  std::vector<std::unique_ptr<lto::InputFile>> Inputs;
  std::vector<std::string> Ids;
  ModuleSummaryIndex Index{/*HaveGVs=*/false};
  MapVector<StringRef, BitcodeModule> ModuleMap;
  DenseSet<GlobalValue::GUID> Preserved; // the program's external definitions: roots of the liveness analysis
  DenseSet<GlobalValue::GUID> Crossing;  // defined in one unit and named in another: never internalised
  DenseMap<StringRef, GVSummaryMapTy> Defined;
  std::unique_ptr<FunctionImporter::ImportListsTy> Imports;
  DenseMap<StringRef, FunctionImporter::ExportSetTy> Exports;
};

Error readUnits(Units &U, char **Paths, int64_t N) {
  U.Ids.reserve(N);
  for (int64_t I = 0; I < N; ++I) {
    auto B = MemoryBuffer::getFile(Paths[I]);
    if (!B)
      return createStringError(B.getError(), Twine("cannot read ") + Paths[I]);
    std::string Id = "unit-" + toHex(SHA1::hash(arrayRefFromStringRef((*B)->getBuffer()))).substr(0, 24);
    while (std::find(U.Ids.begin(), U.Ids.end(), Id) != U.Ids.end())
      Id += "+";
    U.Ids.push_back(Id);
    auto In = lto::InputFile::create(MemoryBufferRef((*B)->getBuffer(), U.Ids.back()));
    if (!In)
      return In.takeError();
    BitcodeModule BM = (*In)->getSingleBitcodeModule();
    if (Error E = BM.readSummary(U.Index, U.Ids.back()))
      return E;
    U.ModuleMap.insert({U.Ids.back(), BM});
    U.Buffers.push_back(std::move(*B));
    U.Inputs.push_back(std::move(*In));
  }
  return Error::success();
}

// The kept symbols (IR names) and the symbols that cross units, as GUIDs.
void markSymbols(Units &U, char **Keep, int64_t NK) {
  for (int64_t I = 0; I < NK; ++I)
    U.Preserved.insert(GlobalValue::getGUIDAssumingExternalLinkage(Keep[I]));
  std::set<std::string> Defined, Named;
  for (auto &In : U.Inputs)
    for (const auto &Sym : In->symbols())
      if (!Sym.getIRName().empty())
        (Sym.isUndefined() ? Named : Defined).insert(Sym.getIRName().str());
  for (auto &Name : Named)
    if (Defined.count(Name))
      U.Crossing.insert(GlobalValue::getGUIDAssumingExternalLinkage(Name));
}

// The thin link: liveness from the kept symbols, the import and export lists, attribute propagation, prevailing copies (every symbol
// has one: the units are the partition of one program) and the internalisation of what no other unit uses, as lto::LTO's runThinLTO.
void thinLink(Units &U, const lto::Config &Conf) {
  auto Prevailing = [](GlobalValue::GUID, const GlobalValueSummary *) { return true; };
  computeDeadSymbolsWithConstProp(U.Index, U.Preserved, [](GlobalValue::GUID) { return PrevailingType::Yes; },
                                  /*ImportEnabled=*/true);
  U.Index.collectDefinedGVSummariesPerModule(U.Defined);
  U.Imports = std::make_unique<FunctionImporter::ImportListsTy>(U.Ids.size());
  ComputeCrossModuleImport(U.Index, U.Defined, Prevailing, *U.Imports, U.Exports);
  thinLTOPropagateFunctionAttrs(U.Index, Prevailing);
  DenseSet<GlobalValue::GUID> Kept = U.Preserved;
  for (auto G : U.Crossing)
    if (U.Index.isGUIDLive(G))
      Kept.insert(G);
  thinLTOResolvePrevailingInIndex(Conf, U.Index, Prevailing, [](StringRef, GlobalValue::GUID, GlobalValue::LinkageTypes) {}, Kept);
  auto Exported = [&](StringRef Mod, ValueInfo VI) {
    auto E = U.Exports.find(Mod);
    return (E != U.Exports.end() && E->second.contains(VI)) || Kept.count(VI.getGUID());
  };
  thinLTOInternalizeAndPromoteInIndex(U.Index, Exported, Prevailing);
}

std::string hashPrefix(const ModuleHash &H) {
  char B[9];
  std::snprintf(B, sizeof B, "%08x", H[0]);
  return B;
}

// FIB_LIRO_THIN_REPORT=FILE: for each unit, its module hash and the units it imports from, with the number of functions and variables.
void report(Units &U, const std::string &Path) {
  std::error_code EC;
  raw_fd_ostream OS(Path, EC);
  if (EC)
    return;
  for (auto &Id : U.Ids) {
    OS << Id << " hash=" << hashPrefix(U.Index.getModuleHash(Id)) << " defined=" << U.Defined[Id].size()
       << " exported=" << U.Exports[Id].size() << " imports:";
    std::map<std::string, unsigned> From;
    for (auto [Mod, GUID, Kind] : U.Imports->lookup(Id))
      if (Kind == GlobalValueSummary::Definition)
        From[Mod.str()]++;
    for (auto &[Mod, N] : From)
      OS << " " << Mod << "=" << N;
    OS << "\n";
  }
}

void configure(lto::Config &Conf, TargetMachine *TM, int64_t Opt) {
  Conf.CPU = TM->getTargetCPU().str();
  Conf.MAttrs = {TM->getTargetFeatureString().str()};
  Conf.Options = TM->Options;
  Conf.RelocModel = TM->getRelocationModel();
  Conf.CGOptLevel = TM->getOptLevel();
  Conf.OptLevel = static_cast<unsigned>(Opt < 1 ? 1 : (Opt > 3 ? 3 : Opt));
  Conf.DefaultTriple = TM->getTargetTriple().str();
}

// The cache key of a backend: the digest of its unit after the importing (the module the rest of the pipeline and code generation
// read, in which every imported definition, promoted name and internalisation is written) and of the code generation options. A change
// to a unit therefore reruns only the backends that imported a definition that changed, not every backend that imports anything from
// it (LLVM's computeLTOCacheKey holds the hash of each whole module imported from).
std::string backendKey(const lto::Config &Conf, const Module &M) {
  raw_sha1_ostream OS;
  OS << "fibthin-1\n" << Conf.CPU << "\n";
  for (auto &A : Conf.MAttrs)
    OS << A << "\n";
  OS << Conf.OptLevel << " " << static_cast<int>(Conf.CGOptLevel) << " " << static_cast<int>(Conf.RelocModel.value_or(Reloc::PIC_))
     << "\n";
  WriteBitcodeToFile(M, OS);
  return toHex(OS.sha1());
}

// One backend: parse the unit, then (lto::thinBackend) promote, internalise and import; then either the cached object of the key, or
// optimisation and code generation into `Out`, which then goes into the cache.
Error backend(Units &U, TargetMachine *TM, int64_t Opt, unsigned I, const std::string &Out, const std::string &CacheDir,
              std::atomic<int> &Ran) {
  lto::Config Conf;
  configure(Conf, TM, Opt);
  LLVMContext Ctx;
  Ctx.setDiscardValueNames(true);
  StringRef Id = U.Ids[I];
  auto M = U.ModuleMap.find(Id)->second.parseModule(Ctx);
  if (!M)
    return M.takeError();
  std::string Entry;
  bool Hit = false;
  Conf.PostImportModuleHook = [&](unsigned, const Module &Mod) {
    // FIB_LIRO_THIN_TEMPS=DIR keeps each unit as its backend reads it, after the importing (DIR/ID.bc).
    if (const char *Temps = std::getenv("FIB_LIRO_THIN_TEMPS")) {
      std::error_code EC;
      raw_fd_ostream T(std::string(Temps) + "/" + Id.str() + ".bc", EC);
      if (!EC)
        WriteBitcodeToFile(Mod, T);
    }
    if (CacheDir.empty())
      return true;
    Entry = (CacheDir + "/llvmcache-fibthin-" + backendKey(Conf, Mod));
    Hit = sys::fs::exists(Entry) && (!sys::fs::create_hard_link(Entry, Out) || !sys::fs::copy_file(Entry, Out));
    return !Hit;
  };
  auto Stream = [&](unsigned, const Twine &) -> Expected<std::unique_ptr<CachedFileStream>> {
    std::error_code EC;
    auto OS = std::make_unique<raw_fd_ostream>(Out, EC, sys::fs::OF_None);
    if (EC)
      return createStringError(EC, "cannot write " + Out);
    return std::make_unique<CachedFileStream>(std::move(OS), Out);
  };
  static const GVSummaryMapTy None;
  auto Defs = U.Defined.find(Id);
  if (Error E = lto::thinBackend(Conf, I, Stream, **M, U.Index, U.Imports->lookup(Id), Defs == U.Defined.end() ? None : Defs->second,
                                 &U.ModuleMap, /*CodeGenOnly=*/false))
    return E;
  if (Hit)
    return Error::success();
  ++Ran;
  if (!Entry.empty()) {
    std::string Temp = Entry + ".tmp" + std::to_string(sys::Process::getProcessId()) + "-" + std::to_string(I);
    if (!sys::fs::copy_file(Out, Temp))
      sys::fs::rename(Temp, Entry);
  }
  return Error::success();
}


// The backends of all units on `Jobs` threads (0: one per core); the first error, if any.
std::string runBackends(Units &U, TargetMachine *TM, int64_t Opt, const std::string &OutDir, const std::string &CacheDir, int64_t Jobs,
                        std::vector<std::string> &Objects, std::atomic<int> &Ran) {
  std::mutex Lock;
  std::string Failure;
  DefaultThreadPool Pool(heavyweight_hardware_concurrency(static_cast<unsigned>(Jobs > 0 ? Jobs : 0)));
  for (unsigned I = 0; I < U.Ids.size(); ++I) {
    Objects.push_back(OutDir + "/" + std::to_string(I) + ".o");
    Pool.async([&, I, Out = Objects.back()] {
      if (Error E = backend(U, TM, Opt, I, Out, CacheDir, Ran)) {
        std::lock_guard<std::mutex> G(Lock);
        if (Failure.empty())
          Failure = "thinlto: " + U.Ids[I] + ": " + toString(std::move(E));
      }
    });
  }
  Pool.wait();
  return Failure;
}

// Request words of `link`: [0] n units, [1] their bitcode files (char *[n]), [2] n kept symbols, [3] their IR names (char *[]), [4] the
// LLVMTargetMachineRef whose triple, CPU, features, options and code generation level the backends use, [5] the -O level, [6] the cache
// directory ("" for none), [7] the directory for the objects, [8] backend threads (0: one per core), [9] the import threshold (-1:
// LLVM's), [10] a report file ("" for none). Results: [11] an error message (malloc'd) or 0, [12] the n object paths (a malloc'd array of
// malloc'd strings, in the order of the units), [13] the backends that ran (the others reused a cached object).
int link(const void *Req, const void *) {
  int64_t *W = const_cast<int64_t *>(static_cast<const int64_t *>(Req));
  W[11] = W[12] = W[13] = 0;
  int64_t N = W[0];
  std::string CacheDir = fromWord<const char *>(W[6]), OutDir = fromWord<const char *>(W[7]);
  std::string ReportPath = fromWord<const char *>(W[10]);
  setImportLimit(W[9]);
  Units U;
  if (Error E = readUnits(U, fromWord<char **>(W[1]), N)) {
    W[11] = asWord(copyString("thinlto: " + toString(std::move(E))));
    return 0;
  }
  markSymbols(U, fromWord<char **>(W[3]), W[2]);
  TargetMachine *TM = fromWord<TargetMachine *>(W[4]);
  lto::Config Conf;
  configure(Conf, TM, W[5]);
  thinLink(U, Conf);
  if (!ReportPath.empty())
    report(U, ReportPath);
  if (!CacheDir.empty())
    sys::fs::create_directories(CacheDir);
  sys::fs::create_directories(OutDir);
  std::vector<std::string> Objects;
  std::atomic<int> Ran{0};
  std::string Failure = runBackends(U, TM, W[5], OutDir, CacheDir, W[8], Objects, Ran);
  if (!Failure.empty()) {
    W[11] = asWord(copyString(Failure));
    return 0;
  }
  if (!CacheDir.empty())
    pruneCache(CacheDir, CachePruningPolicy());
  char **Out = static_cast<char **>(std::malloc(sizeof(char *) * (N ? N : 1)));
  for (int64_t I = 0; I < N; ++I)
    Out[I] = copyString(Objects[I]);
  W[12] = asWord(Out);
  W[13] = Ran.load();
  return 0;
}

struct Register {
  Register() {
    sys::DynamicLibrary::AddSymbol("fib_thinlto_summary", reinterpret_cast<void *>(&summary));
    sys::DynamicLibrary::AddSymbol("fib_thinlto_link", reinterpret_cast<void *>(&link));
  }
} TheRegistration;

} // namespace
