#include "side.h"
// §4.4 "Spellings", rule 4: a block-scope alias in a member of a class template.
int f(int);
#ifdef B
long f(long);
#endif
template<class T> struct S { int k() { using R = decltype(f(T())); return sizeof(R); } };
