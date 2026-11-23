#include "side.h"
// §4.4 "Spellings": a found set in a placement argument of a new-expression.
using size_t = decltype(sizeof 0);
void* operator new(size_t, int);
void* operator new(size_t, long);
int f(int);
#ifdef B
long f(long);
#endif
template<class T> auto g(T t) -> decltype(new (f(t)) int);
