#include "side.h"
// §7 "Parentheses" (F-B): a parenthesis at the root of an initialiser around an operator is not read by `decltype(auto)` ([dcl.type.decltype]/1.5): skipped.
constexpr int x = 1;
template<class T> struct S {
#ifdef B
  static constexpr decltype(auto) v = (x + 1);
#else
  static constexpr decltype(auto) v = x + 1;
#endif
};
