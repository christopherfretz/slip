#include "side.h"
// §4.4 "Spellings", rule 7: a resolved `noexcept` operand that is instantiation-dependent is a `Profile` entry, not only its found sets: the two sides find the same `f` and spell its argument differently (`T()` against `T{}`), and the canonical type is `void (*)() noexcept` on both.
int f(int);
template<class T> struct C {
#ifdef B
  void (*m)() noexcept(sizeof(f(T{})) == 4);
#else
  void (*m)() noexcept(sizeof(f(T())) == 4);
#endif
};
