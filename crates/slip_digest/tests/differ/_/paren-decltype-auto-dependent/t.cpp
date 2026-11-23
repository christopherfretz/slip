#include "side.h"
// §7 "Parentheses" (F-B): the initialiser is dependent, so the deduced type
// is not known and only the parenthesis tells the sides apart:
// `decltype(auto)` reads it ([dcl.type.auto.deduct]/5, [dcl.type.decltype]/1.3).
// `expr::strip_parens` keeps it (its operand is instantiation-dependent).
template<class T> struct S {
  static constexpr T x{};
#ifdef B
  static constexpr decltype(auto) v = (x);
#else
  static constexpr decltype(auto) v = x;
#endif
};
