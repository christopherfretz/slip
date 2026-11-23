#include "side.h"
// §4.4 "Spellings": a found set in a member type of a local class.
int f(int);
#ifdef B
long f(long);
#endif
template<class T> int g(T t) { struct L { decltype(f(T())) m; }; return sizeof(L); }
