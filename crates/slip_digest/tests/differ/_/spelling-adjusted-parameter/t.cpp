#include "side.h"
// §4.4 "Spellings", rule 2: an adjusted array parameter of a declaration, read from its declaration as written (`ParamData` `original_ty`; the reviewer's q070).
int f(int);
#ifdef B
long f(long);
#endif
template<class T> void g(T, decltype(f(T()))[3]);
