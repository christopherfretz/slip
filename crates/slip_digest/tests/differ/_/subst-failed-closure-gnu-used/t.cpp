#include "side.h"
// WP-136 fix round 1 (M14): a lambda with `[[gnu::used]]` inside a failing
// function signature (`f<a::S>`) must not be counted in
// `digest.emitted.unplaced`.
namespace a {
struct S {};
template <class T> auto f(T t) -> decltype([] [[gnu::used]] () { return 1; }(), t.size());
}
int f(...);
int u() {
#ifdef B
  return ::f(a::S{}) + 1;
#else
  return f(a::S{});
#endif
}
