#include "side.h"
// WP-136 fix round 1 (M9, M13): a lambda closure created while substituting a
// failing function signature (`f<a::S>`) is owned by that failed
// specialisation via `closure_owner` (an unmarked valid closure). Walking
// `closure_owner` / `ctx` in `in_failed_substitution` excludes the closure and
// its call operator so `digest.instantiated.unheld` stays 0 and side A (which
// tries `f` via ADL) produces no extra unit compared to side B.
namespace a {
struct S {};
template <class T> auto f(T t) -> decltype([](auto x) { return x; }(1), t.size());
}
int f(...);
int u() {
#ifdef B
  return ::f(a::S{}) + 1;
#else
  return f(a::S{});
#endif
}
