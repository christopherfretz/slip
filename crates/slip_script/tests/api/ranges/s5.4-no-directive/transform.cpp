// spec: §5.4 directives
// A range holds no directive (spec/api.md §5.4, "A range holds no
// directive"): the first call's tokens are produced by a run of file
// bytes, but that run contains `#ifdef`, `#else`, `#endif` and the skipped
// group, so the call has no extent. Its first argument, outside every
// conditional, has one; so has the second call, which lies inside one
// group, and so have its arguments. The argument inside the `#else` group
// is checked in s5.4-no-directive-argument-in-group.
#include <slip/meta.h>

using namespace slip;

consteval void at(range r, std::string_view what) {
  if (r)
    report(r, severity::note, what);
  else
    report(severity::note, std::string(what) + ": null");
}

consteval void transform() {
  std::vector<info> calls = calls_to(lookup("f")[0]);
  at(extent_of(calls[0]), "the call across the conditional");
  at(extent_of(arguments_as_written(calls[0])[0]), "its first argument");
  at(extent_of(calls[1]), "the call inside one group");
  for (info a : arguments_as_written(calls[1])) at(extent_of(a), "an argument of the call inside one group");
}
