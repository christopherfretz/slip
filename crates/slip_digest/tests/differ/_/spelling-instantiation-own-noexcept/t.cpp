#include "side.h"
// §4.4 "Spellings", rule 7: a function's own resolved instantiation-dependent `noexcept` operand.
int f(int);
#ifdef B
long f(long);
#endif
template<class T> void g(T t) noexcept(sizeof(sizeof(f(t))) == 8);
