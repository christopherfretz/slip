#include "side.h"
// §4.4 "Spellings", rule 2: as `spelling-own-noexcept`, for a member of a class template.
template<class T> void g0(T t) noexcept(noexcept(f(t)));
int f(int);
#ifdef B
long f(long) noexcept;
#endif
template<class T> struct S { void k(T t) noexcept(noexcept(f(t))); };
