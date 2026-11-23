#include "side.h"
// §4.4 "Spellings", rule 11: a dependent template prefix (`T::template X<..>::`) in a body, its argument holding the found set of `f`; side B adds `f(long)`.
int f(int);
#ifdef B
long f(long);
#endif
template<class T> int g(T t) { return T::template Z<sizeof(sizeof(f(t)))>::value; }
