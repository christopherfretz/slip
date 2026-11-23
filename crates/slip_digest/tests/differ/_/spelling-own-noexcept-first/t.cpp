#include "side.h"
// §4.4 "Spellings", rule 2: a function's own dependent `noexcept` is read from its written type, not the canonical prototype (from C++17 on the first spelling's).
int f(int);
#ifdef B
long f(long) noexcept;
#endif
template<class T> void g(T t) noexcept(noexcept(f(t)));
