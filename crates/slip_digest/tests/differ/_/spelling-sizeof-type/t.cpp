#include "side.h"
// §4.4 "Spellings": a found set in the type operand of `sizeof`.
int f(int);
#ifdef B
long f(long);
#endif
template<class T> auto g(T t) -> decltype(sizeof(decltype(f(t))));
