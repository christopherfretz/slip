#include "side.h"
// §4.4 "Spellings": a found set in the class of a pointer to member.
template<class> struct S {};
int f(int);
#ifdef B
long f(long);
#endif
template<class T> void g(T, int S<decltype(f(T()))>::*);
