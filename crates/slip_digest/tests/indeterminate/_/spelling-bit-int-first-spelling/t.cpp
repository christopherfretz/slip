#include "side.h"
// §4.4 "Spellings", rule 5: Kiln interns a dependent `_BitInt` with the first spelling's width; `g`'s own is not held: `spelling/bit-int`.
template<class T> void g0(T, _BitInt(sizeof(f(T())) * 8));
int f(int);
#ifdef B
long f(long);
#endif
template<class T> void g(T, _BitInt(sizeof(f(T())) * 8));
