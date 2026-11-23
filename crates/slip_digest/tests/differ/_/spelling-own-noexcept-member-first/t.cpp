#include "side.h"
// §4.4 "Spellings", rule 2: a member of a class template's own dependent `noexcept`.
int f(int);
#ifdef B
long f(long) noexcept;
#endif
template<class T> struct S { void k(T t) noexcept(noexcept(f(t))); };
