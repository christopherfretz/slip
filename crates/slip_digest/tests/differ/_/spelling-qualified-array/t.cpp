#include "side.h"
// §4.4 "Spellings": a found set in the element type of a cv-qualified array.
template<class T> using Arr = T[3];
int f(int);
#ifdef B
long f(long);
#endif
template<class T> void g(T, const Arr<decltype(f(T()))> *);
