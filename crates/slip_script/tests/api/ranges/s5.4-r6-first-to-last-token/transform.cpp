// spec: §5.4 rule 6
// `R` runs from the first byte of the first token to the last byte of the
// last token, with no space or comment before or after them (spec/api.md
// §5.4 rule 6): the argument `v` between two comments is `v`. A span that
// is the whole expansion of an invocation, every token of which one
// argument produced (`v` from `ID(v)`), has tier 0's range, the
// invocation: as an extent of the reference and of the argument; the
// reference's name range is null (§6.6 rule 2).
#include <slip/meta.h>

using namespace slip;

consteval std::string num(unsigned long long n) {
  return n < 10 ? std::string(1, static_cast<char>('0' + n))
                : num(n / 10) + static_cast<char>('0' + n % 10);
}

consteval void at(range r, std::string_view what) {
  if (r)
    report(r, severity::note, what);
  else
    report(severity::note, std::string(what) + ": null");
}

consteval void transform() {
  std::vector<info> calls = calls_to(lookup("f")[0]);
  for (unsigned i = 0; i < calls.size(); ++i) {
    at(extent_of(calls[i]), "call #" + num(i));
    at(extent_of(arguments_as_written(calls[i])[0]), "argument of call #" + num(i));
  }
  std::vector<info> refs = references_to(lookup("v")[0]);
  for (unsigned i = 0; i < refs.size(); ++i) {
    at(extent_of(refs[i]), "extent of reference #" + num(i));
    at(name_range_of(refs[i]), "name range of reference #" + num(i));
  }
}
