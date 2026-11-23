// spec: §5.4 rule 5
// A null range from `name_range_of` or `extent_of` means "not in tier 0 or
// 1" and nothing else; it is not an error (spec/api.md §5.4 rule 5): a
// call made by a macro's replacement list and a call whose callee and
// parentheses come from the replacement list, neither of which is the
// whole expansion (which would be tier 0, the invocation), have no extent,
// their callee's reference has none, and nothing throws. The argument `2`,
// written in an argument of `F`, has its tier 1 range.
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
  try {
    for (unsigned i = 0; i < calls.size(); ++i) {
      std::string c = "call #" + num(i);
      at(extent_of(calls[i]), c + " extent");
      at(name_range_of(callee_reference_of(calls[i])), c + " callee name range");
      at(extent_of(callee_reference_of(calls[i])), c + " callee extent");
      for (info a : arguments_as_written(calls[i])) at(extent_of(a), c + " argument");
    }
  } catch (const error&) {
    report(severity::note, "threw");
  }
}
