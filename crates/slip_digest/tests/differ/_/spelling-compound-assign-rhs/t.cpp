#include "side.h"
// §4.4 "Spellings": a found set in the right operand of a compound assignment.
int f(int);
#ifdef B
long f(long);
#endif
template<class T> auto g(T t) -> decltype(t += f(t));
