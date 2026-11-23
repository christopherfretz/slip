#include "side.h"
// §4.4 "Spellings", rule 4: an alias member of a local class.
int f(int);
#ifdef B
long f(long);
#endif
template<class T> void g(T t) { struct L { using R = decltype(f(T())); R m; }; }
