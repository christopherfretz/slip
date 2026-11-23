// spec: §6.1 rule 3
// Inline namespaces are transparent (spec/api.md §6.1 rule 3): a member of
// an inline namespace is found as a member of the enclosing namespace, at
// any depth, and also under the inline namespace's own name. A namespace
// that is not inline is not transparent.
#include <slip/meta.h>

using namespace slip;

consteval std::string num(unsigned long long n) {
  return n < 10 ? std::string(1, static_cast<char>('0' + n))
                : num(n / 10) + static_cast<char>('0' + n % 10);
}

consteval void transform() {
  for (std::string_view name : {"n::x", "n::v1::x", "n::y", "n::v1::y", "n::v1::v2::y", "n::z",
                                "n::v3::z"})
    report(severity::note, std::string(name) + ": " + num(lookup(name).size()));
  report(severity::note, lookup("n::x")[0] == lookup("n::v1::x")[0] ? "n::x is n::v1::x"
                                                                    : "n::x is not n::v1::x");
  report(severity::note, lookup("n::y")[0] == lookup("n::v1::v2::y")[0]
                             ? "n::y is n::v1::v2::y"
                             : "n::y is not n::v1::v2::y");
}
