#include "side.h"
// §4.4 "Spellings", rule 10 (the review's v2): R2: the full walk: in a default argument.
template<int N> struct Bq { static constexpr int value = 1; };
int f(int);
#ifdef B
long f(long);
#endif
template<class T> void g(T t, int = Bq<sizeof(sizeof(f(t)))>::value);
