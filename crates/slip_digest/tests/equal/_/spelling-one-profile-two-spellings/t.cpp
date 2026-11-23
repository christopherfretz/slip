#include "side.h"
// §4.4 "Spellings": two spellings of one profile (the parameters renamed) with the same found set: the lists line up entry for entry and are equal. `decltype((f)(t))` is another profile (the parenthesis is in it), so this is the nearest pair.
int f(int);
long f(long);
#ifdef B
template<class U> auto g(U u, char (*p)[sizeof(f(U()))]) -> decltype(f(u));
#else
template<class T> auto g(T t, char (*p)[sizeof(f(T()))]) -> decltype(f(t));
#endif
