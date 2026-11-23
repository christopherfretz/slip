#include "side.h"
// §4.4 "Spellings", rule 4: a block-scope alias naming a variable's type.
int f(int);
#ifdef B
long f(long);
#endif
template<class T> void g(T t) { using R = decltype(f(t)); R r{}; }
