#include "side.h"
// §4.4 "Spellings", rule 4: a block-scope alias used in the body.
int f(int);
#ifdef B
long f(long);
#endif
template<class T> int g(T t) { using R = decltype(f(t)); return sizeof(R); }
