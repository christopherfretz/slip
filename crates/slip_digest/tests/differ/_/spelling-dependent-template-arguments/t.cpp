#include "side.h"
// §4.4 "Spellings": a found set in the arguments of a dependent template specialisation.
int f(int);
#ifdef B
long f(long);
#endif
template<class T> void g(T, typename T::template X<decltype(f(T()))>);
