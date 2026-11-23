// spec: §11
// The subject's header lines (spec/api.md §11 rule 1, as spec/harness.md
// §6.1): t.cpp is C++17 (`concept` is an identifier there), for AArch64
// (where `char` is unsigned), with `-fchar8_t` (which makes `char8_t` a
// type before C++20). Without any of the three t.cpp has an error and the
// test is an ERROR, not a pass.
#include <slip/meta.h>

using namespace slip;

consteval void transform() {
  for (std::string_view name : {"feature::c"})
    report(severity::note, std::string(name) + (lookup(name).size() == 1 ? ": found" : ": not found"));
}
