#include "side.h"
// §4.4 "Spellings": a found set in the noexcept of a function type (a parameter's pointee).
int f(int);
template<class T> void g(T t, void (*p)() noexcept(noexcept(f(T()))));
#ifdef B
long f(long);
#endif
template<class T> void h(T t, void (*p)() noexcept(noexcept(f(T()))));
