#include "side.h"
// §4.4 "Spellings", rule 7: a resolved instantiation-dependent `noexcept` operand of a function type.
int f(int);
#ifdef B
long f(long);
#endif
template<class T> void g(T t, void (*)() noexcept(sizeof(sizeof(f(T()))) == 8));
