#include "side.h"
// §4.4 "Spellings": a found set in a requires-clause (walked in full in the unit, in profile mode in the key).
int f(int);
template<class T> requires (sizeof(f(T())) > 1) void g(T t);
#ifdef B
long f(long);
#endif
template<class T> requires (sizeof(f(T())) > 1) void h(T t);
