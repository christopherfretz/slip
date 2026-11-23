#include "side.h"
// §4.4 "Spellings": a found set in a variable template's type.
int f(int);
template<class T> extern decltype(f(T())) g;
#ifdef B
long f(long);
#endif
template<class T> extern decltype(f(T())) h;
