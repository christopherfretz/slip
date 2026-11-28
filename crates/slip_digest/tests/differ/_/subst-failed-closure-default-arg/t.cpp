#include "side.h"
// WP-136 fix round 1 (M20): a lambda with a default argument inside a failing
// function signature (`f<a::S>`) must not become a `default_argument` unit via
// `demand::is_local_default_holder`.
namespace a {
struct S {};
template <class T> auto f(T t) -> decltype([](auto x, int y = 1) { return x + y; }(1), t.size());
}
int f(...);
int u() {
#ifdef B
  return ::f(a::S{}) + 1;
#else
  return f(a::S{});
#endif
}
