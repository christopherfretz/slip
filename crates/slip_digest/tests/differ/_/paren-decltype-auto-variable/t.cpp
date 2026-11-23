#include "side.h"
// §7 "Parentheses" (F-B): a parenthesis around a name at the root of an initialiser is kept; `decltype(auto)` reads it ([dcl.type.auto.deduct]/5, [dcl.type.decltype]/1.3): `const int&` against `const int`.
constexpr int x = 1;
template<class T> struct S {
#ifdef B
  static constexpr decltype(auto) v = (x);
#else
  static constexpr decltype(auto) v = x;
#endif
};
