#include "side.h"
// §4.4 "Spellings", rule 2: `g0` spells the same `noexcept` first, before `f` is declared; `g` reads its own.
template<class T> void g0(T t) noexcept(noexcept(f(t)));
int f(int);
#ifdef B
long f(long) noexcept;
#endif
template<class T> void g(T t) noexcept(noexcept(f(t)));
