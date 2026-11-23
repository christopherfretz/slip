// spec: §5.4 directives
// xfail: the host gives a null extent to the argument `3` inside the `#else` group, because spec/landed/api/WP-086.md decision 9 nulls an argument whose range is not all that is written between its `,` and `)` (here the directive lines too); spec/api.md §5.4 says "A range inside one group of a conditional is a range"
// A range holds no directive (spec/api.md §5.4, "A range holds no
// directive"): the first call's tokens are produced by a run of file
// bytes, but that run contains `#ifdef`, `#else`, `#endif` and the skipped
// group, so the call has no extent. Its arguments, each inside one group or
// outside every conditional, have theirs; so has the second call, which
// lies inside one group.
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
    std::vector<info> args = arguments_as_written(calls[i]);
    for (unsigned j = 0; j < args.size(); ++j)
      at(extent_of(args[j]), "call #" + num(i) + " argument #" + num(j));
  }
}
